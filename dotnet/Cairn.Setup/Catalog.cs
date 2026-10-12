// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Cairn.Setup;

// The game catalog: which games the launcher knows, and everything about each one, read from the network (and the last copy of it on
// disk) instead of being built into the launcher. Two kinds of file:
//   games.json  -- one, in the Cairn-client/games repository: the list of games (each with the repository of its mod), the repositories
//                  of the packs that are not a game's mod, the shared requirement definitions, the blocked mod versions and the notices.
//   game.json   -- one per game, published with the game's own repository (a release asset) or, for a game without a repository yet,
//                  in the games repository (games/<id>/game.json): the name, the Steam ids, the artwork, what it requires, what its mod does.
// Both are data only. Everything in them is checked here: ids, names and paths are plain, text is short, and a picture is a file name
// that is looked up beside the file that names it, never a URL of its own.

/// <summary>A game in games.json: its id and, when it has one, the repository ("owner/name") whose releases carry its mod and its game.json.</summary>
public sealed record GameEntry(string Id, string Repo);

/// <summary>A pack that is not a game's mod (a shared core, content) and the repository that publishes it.</summary>
public sealed record PackSource(string Id, string Repo);

/// <summary>Something a game needs, defined once and named by id: the games that need it say so by naming it. <c>Kind</c> is how it is checked and linked.</summary>
public sealed record Component(string Id, string Kind, string Name, uint AppId, bool HasStorePage)
{
    /// <summary>The only kind the launcher can check and link today; any other kind is shown by name only.</summary>
    public const string SteamApp = "steam-app";
}

/// <summary>A mod version the launcher must not offer (and warns about when it is installed). Either one <c>Version</c> or every version up to <c>Max</c>.</summary>
public sealed record BlockedPack(string Pack, string Version, string Max, string Reason)
{
    public bool Matches(string pack, string version)
    {
        if (!string.Equals(Pack, pack, StringComparison.OrdinalIgnoreCase) || !System.Version.TryParse(version, out var v)) return false;
        if (Version.Length > 0) return System.Version.TryParse(Version, out var one) && one == v;
        return Max.Length > 0 && System.Version.TryParse(Max, out var max) && v <= max;
    }
}

/// <summary>A message for the players: a known issue, or news. Shown on Home and under Help until it is no longer current.</summary>
public sealed record Notice(string Id, string Title, string Text, string Kind, IReadOnlyList<string> Games, string FromLauncher, string UntilLauncher, string Until)
{
    public const string KnownIssue = "known-issue";
    public const string News = "news";

    /// <summary>Whether to show it now, to someone with this launcher version who has (or looks at) these games.</summary>
    public bool IsCurrent(string? launcherVersion, DateTime utcNow, Func<string, bool>? hasGame = null)
    {
        if (Until.Length > 0 && DateTime.TryParse(Until, System.Globalization.CultureInfo.InvariantCulture, System.Globalization.DateTimeStyles.AdjustToUniversal | System.Globalization.DateTimeStyles.AssumeUniversal, out var end) && utcNow > end) return false;
        string v = (launcherVersion ?? "").Split('+', '-')[0];
        if (FromLauncher.Length > 0 && System.Version.TryParse(FromLauncher, out var from) && System.Version.TryParse(v, out var have) && have < from) return false;
        if (UntilLauncher.Length > 0 && System.Version.TryParse(UntilLauncher, out var upTo) && System.Version.TryParse(v, out var h2) && h2 > upTo) return false;
        return Games.Count == 0 || hasGame is null || Games.Any(hasGame);
    }
}

/// <summary>games.json.</summary>
public sealed record GameList(
    IReadOnlyList<GameEntry> Games, IReadOnlyList<PackSource> Packs, IReadOnlyDictionary<string, Component> Components,
    IReadOnlyList<BlockedPack> Blocked, IReadOnlyList<Notice> Notices)
{
    public const string FileName = "games.json";
    public const int CurrentSchema = 1;

    public static readonly GameList Empty = new(Array.Empty<GameEntry>(), Array.Empty<PackSource>(),
        new Dictionary<string, Component>(), Array.Empty<BlockedPack>(), Array.Empty<Notice>());

    public string? RepoOfPack(string id) => Packs.FirstOrDefault(p => string.Equals(p.Id, id, StringComparison.OrdinalIgnoreCase))?.Repo
        ?? Games.FirstOrDefault(g => string.Equals(g.Id, id, StringComparison.OrdinalIgnoreCase) && g.Repo.Length > 0)?.Repo;

    public BlockedPack? BlockOf(string pack, string version) => Blocked.FirstOrDefault(b => b.Matches(pack, version));

    /// <summary>Reads games.json; null when it is not one this launcher understands (bad JSON, another schema, an unsafe or malformed entry).</summary>
    public static GameList? Parse(string json)
    {
        if (json.Length > CatalogLimits.MaxFile) return null;
        try
        {
            using var doc = JsonDocument.Parse(json);
            var r = doc.RootElement;
            if (r.GetProperty("schema").GetInt32() != CurrentSchema) return null;

            var games = new List<GameEntry>();
            foreach (var g in Items(r, "games"))
            {
                string id = Str(g, "id"), repo = Str(g, "repo");
                if (!CatalogLimits.IsId(id) || (repo.Length > 0 && !PackCatalog.IsRepo(repo)) || games.Any(x => x.Id == id)) return null;
                games.Add(new GameEntry(id, repo));
            }
            var packs = new List<PackSource>();
            foreach (var p in Items(r, "packs"))
            {
                string id = Str(p, "id"), repo = Str(p, "repo");
                if (!CatalogLimits.IsId(id) || !PackCatalog.IsRepo(repo)) return null;
                packs.Add(new PackSource(id, repo));
            }
            var components = new Dictionary<string, Component>(StringComparer.OrdinalIgnoreCase);
            if (r.TryGetProperty("components", out var cs) && cs.ValueKind == JsonValueKind.Object)
                foreach (var c in cs.EnumerateObject())
                {
                    var comp = ParseComponent(c.Name, c.Value);
                    if (comp is null) return null;
                    components[c.Name] = comp;
                }
            var blocked = new List<BlockedPack>();
            foreach (var b in Items(r, "blocked"))
            {
                var bp = new BlockedPack(Str(b, "pack"), Str(b, "version"), Str(b, "max"), CatalogLimits.Text(Str(b, "reason"), 300));
                if (!CatalogLimits.IsId(bp.Pack) || (bp.Version.Length == 0) == (bp.Max.Length == 0)
                    || (bp.Version.Length > 0 && !CatalogLimits.IsVersion(bp.Version)) || (bp.Max.Length > 0 && !CatalogLimits.IsVersion(bp.Max))) return null;
                blocked.Add(bp);
            }
            var notices = new List<Notice>();
            foreach (var n in Items(r, "notices"))
            {
                var forGames = new List<string>();
                if (n.TryGetProperty("games", out var ng) && ng.ValueKind == JsonValueKind.Array)
                    foreach (var e in ng.EnumerateArray()) { if (e.ValueKind != JsonValueKind.String || !CatalogLimits.IsId(e.GetString() ?? "")) return null; forGames.Add(e.GetString()!); }
                string kind = Str(n, "kind").Length == 0 ? Notice.News : Str(n, "kind");
                var no = new Notice(Str(n, "id"), CatalogLimits.Text(Str(n, "title"), 120), CatalogLimits.Text(Str(n, "text"), 600), kind, forGames,
                    Str(n, "fromLauncher"), Str(n, "untilLauncher"), Str(n, "until"));
                if (!CatalogLimits.IsId(no.Id) || no.Title.Length == 0 || kind is not (Notice.KnownIssue or Notice.News)
                    || (no.FromLauncher.Length > 0 && !CatalogLimits.IsVersion(no.FromLauncher)) || (no.UntilLauncher.Length > 0 && !CatalogLimits.IsVersion(no.UntilLauncher))) return null;
                notices.Add(no);
            }
            return new GameList(games, packs, components, blocked, notices);
        }
        catch (Exception ex) when (ex is JsonException or KeyNotFoundException or InvalidOperationException or FormatException) { return null; }
    }

    internal static Component? ParseComponent(string id, JsonElement e)
    {
        if (!CatalogLimits.IsId(id) || e.ValueKind != JsonValueKind.Object) return null;
        string kind = Str(e, "kind"), name = CatalogLimits.Text(Str(e, "name"), 80);
        if (name.Length == 0 || !CatalogLimits.IsId(kind)) return null;
        uint appId = 0;
        if (kind == Component.SteamApp)
        {
            if (!e.TryGetProperty("appId", out var a) || a.ValueKind != JsonValueKind.Number || !a.TryGetUInt32(out appId) || appId == 0) return null;
        }
        bool store = !e.TryGetProperty("storePage", out var sp) || sp.ValueKind != JsonValueKind.False;
        return new Component(id, kind, name, appId, store);
    }

    internal static IEnumerable<JsonElement> Items(JsonElement parent, string name) =>
        parent.TryGetProperty(name, out var a) && a.ValueKind == JsonValueKind.Array ? a.EnumerateArray() : Enumerable.Empty<JsonElement>();

    internal static string Str(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Object && e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() ?? "" : "";
}

/// <summary>How well a feature works, as a game's mod page tells it.</summary>
public enum FeatureState { Ready, InTest, Planned }

public sealed record FeatureLine(string Name, string Detail, FeatureState State);
public sealed record FeatureGroup(string Title, IReadOnlyList<FeatureLine> Items);

/// <summary>The pictures of a game, as file names to look up beside the game.json that names them.</summary>
public sealed record GameImages(string Background, string Logo, string Hero, string Poster, IReadOnlyList<string> Screenshots);

/// <summary>game.json: everything about one game that is not code.</summary>
public sealed record GameManifest(
    string Id, string Name, string Developer, string Franchise, bool Supported, uint SteamAppId, uint StoreAppId, string Exe,
    string Description, string Accent, GameImages Images, IReadOnlyList<string> Requires, IReadOnlyList<Component> InlineRequires,
    IReadOnlyList<FeatureGroup> Features, IReadOnlyList<string> Content)
{
    public const string FileName = "game.json";
    public const int CurrentSchema = 1;

    public static GameManifest? Parse(string json)
    {
        if (json.Length > CatalogLimits.MaxFile) return null;
        try
        {
            using var doc = JsonDocument.Parse(json);
            var r = doc.RootElement;
            if (r.GetProperty("schema").GetInt32() != CurrentSchema) return null;
            string id = GameList.Str(r, "id"), name = CatalogLimits.Text(GameList.Str(r, "name"), 80);
            if (!CatalogLimits.IsId(id) || name.Length == 0) return null;
            string accent = GameList.Str(r, "accent");
            if (accent.Length > 0 && !Regex.IsMatch(accent, "^#[0-9A-Fa-f]{6}$")) return null;
            string exe = GameList.Str(r, "exe");
            if (exe.Length > 0 && !(exe.Length <= 80 && exe.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.' or ' ') && !exe.Contains(".."))) return null;

            GameImages images = new("", "", "", "", Array.Empty<string>());
            if (r.TryGetProperty("images", out var im) && im.ValueKind == JsonValueKind.Object)
            {
                var shots = new List<string>();
                foreach (var s in GameList.Items(im, "screenshots")) { string f = s.ValueKind == JsonValueKind.String ? s.GetString() ?? "" : ""; if (!CatalogLimits.IsImagePath(f)) return null; shots.Add(f); }
                if (shots.Count > 8) return null;
                images = new GameImages(GameList.Str(im, "background"), GameList.Str(im, "logo"), GameList.Str(im, "hero"), GameList.Str(im, "poster"), shots);
                foreach (var f in new[] { images.Background, images.Logo, images.Hero, images.Poster })
                    if (f.Length > 0 && !CatalogLimits.IsImagePath(f)) return null;
            }

            var requires = new List<string>();
            var inline = new List<Component>();
            foreach (var q in GameList.Items(r, "requires"))
            {
                if (q.ValueKind == JsonValueKind.String) { string cid = q.GetString() ?? ""; if (!CatalogLimits.IsId(cid)) return null; requires.Add(cid); }
                else if (q.ValueKind == JsonValueKind.Object)
                {
                    var c = GameList.ParseComponent(GameList.Str(q, "id"), q);
                    if (c is null) return null;
                    inline.Add(c);
                }
                else return null;
            }
            if (requires.Count + inline.Count > 12) return null;

            var groups = new List<FeatureGroup>();
            foreach (var g in GameList.Items(r, "features"))
            {
                string title = CatalogLimits.Text(GameList.Str(g, "title"), 40);
                var lines = new List<FeatureLine>();
                foreach (var f in GameList.Items(g, "items"))
                {
                    string state = GameList.Str(f, "status");
                    var st = state switch { "" or "ready" => FeatureState.Ready, "inTest" => FeatureState.InTest, "planned" => FeatureState.Planned, _ => (FeatureState?)null };
                    string fname = CatalogLimits.Text(GameList.Str(f, "name"), 80);
                    if (st is null || fname.Length == 0) return null;
                    lines.Add(new FeatureLine(fname, CatalogLimits.Text(GameList.Str(f, "detail"), 400), st.Value));
                }
                if (title.Length == 0 || lines.Count > 30) return null;
                groups.Add(new FeatureGroup(title, lines));
            }
            if (groups.Count > 12) return null;

            var content = new List<string>();
            foreach (var c in GameList.Items(r, "content")) { string cid = c.ValueKind == JsonValueKind.String ? c.GetString() ?? "" : ""; if (!CatalogLimits.IsId(cid)) return null; content.Add(cid); }

            uint U(string n) => r.TryGetProperty(n, out var v) && v.ValueKind == JsonValueKind.Number && v.TryGetUInt32(out var u) ? u : 0;
            return new GameManifest(id, name, CatalogLimits.Text(GameList.Str(r, "developer"), 80), CatalogLimits.Text(GameList.Str(r, "franchise"), 80),
                !r.TryGetProperty("supported", out var sup) || sup.ValueKind != JsonValueKind.False, U("steamAppId"), U("storeAppId"), exe,
                CatalogLimits.Text(GameList.Str(r, "description"), 400), accent, images, requires, inline, groups, content);
        }
        catch (Exception ex) when (ex is JsonException or KeyNotFoundException or InvalidOperationException or FormatException) { return null; }
    }

    /// <summary>What the game needs, with the shared definitions filled in; a name that no definition has is left out.</summary>
    public IReadOnlyList<Component> ResolveRequirements(GameList list)
    {
        var all = new List<Component>();
        foreach (var id in Requires) if (list.Components.TryGetValue(id, out var c)) all.Add(c);
        all.AddRange(InlineRequires);
        return all;
    }
}

/// <summary>The checks every catalog file passes before anything in it is used.</summary>
public static class CatalogLimits
{
    public const int MaxFile = 256 * 1024;

    public static bool IsId(string s) => s.Length is > 0 and <= 40 && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_');
    public static bool IsVersion(string s) => Regex.IsMatch(s, @"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$");

    /// <summary>A picture is a plain file name (optionally in plain sub-folders) with a picture extension: never a URL, never a path that leaves the folder.</summary>
    public static bool IsImagePath(string s)
    {
        if (s.Length is 0 or > 100 || s.StartsWith('/') || s.Contains("..") || s.Contains(':') || s.Contains('\\')) return false;
        if (!s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.' or '/')) return false;
        string ext = Path.GetExtension(s).ToLowerInvariant();
        return ext is ".jpg" or ".jpeg" or ".png" or ".webp";
    }

    /// <summary>Text for people: control characters out, markup characters out, no longer than <paramref name="max"/>.</summary>
    public static string Text(string s, int max)
    {
        var clean = new string(s.Where(c => !char.IsControl(c) && c is not ('<' or '>')).ToArray()).Trim();
        return clean.Length <= max ? clean : clean[..max].TrimEnd();
    }
}
