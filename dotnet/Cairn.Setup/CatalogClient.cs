// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
namespace Cairn.Setup;

/// <summary>One game of the catalog: its entry, its game.json, and where its pictures are (a folder URL or path, ending in '/').</summary>
public sealed record CatalogGame(GameEntry Entry, GameManifest Manifest, string BaseUrl)
{
    /// <summary>A picture's address (empty when the game names none).</summary>
    public string Picture(string name) => name.Length == 0 ? "" : BaseUrl + name;
}

/// <summary>What the catalog says right now; <see cref="FromCache"/> when the network could not be reached and the last copy on disk was used.</summary>
public sealed record CatalogSnapshot(GameList List, IReadOnlyList<CatalogGame> Games, bool FromCache);

/// <summary>
/// Reads the catalog: games.json from the games repository, then each game's game.json from the game's own repository (its latest release),
/// falling back to the games repository's <c>games/&lt;id&gt;/</c> folder for a game that has none there. Every success is kept on disk, so a launcher
/// that is offline shows what it last knew. Nothing here throws on a network problem.
/// </summary>
public sealed class CatalogClient
{
    public const string DefaultGamesBase = "https://raw.githubusercontent.com/Cairn-client/games/main/";

    private readonly HttpClient http;
    private readonly string cacheDir;
    private readonly string gamesBase;
    private readonly bool useRepoReleases;

    /// <summary>For a game whose player follows the beta channel, its game.json is read from the newest release (a pre-release too), as its pack is: what a beta offers can be in its game.json.</summary>
    public Func<string, bool> FollowsBeta { get; init; } = _ => false;

    /// <param name="gamesBase">Where games.json and <c>games/&lt;id&gt;/</c> are (a URL ending in '/').</param>
    /// <param name="useRepoReleases">False to read every game.json from <paramref name="gamesBase"/> (a local copy of the catalog, tests).</param>
    public CatalogClient(HttpClient http, string cacheDir, string? gamesBase = null, bool? useRepoReleases = null)
    {
        this.http = http;
        this.cacheDir = cacheDir;
        string b = string.IsNullOrWhiteSpace(gamesBase) ? DefaultGamesBase : gamesBase;
        this.gamesBase = b.EndsWith('/') ? b : b + "/";
        this.useRepoReleases = useRepoReleases ?? string.Equals(this.gamesBase, DefaultGamesBase, StringComparison.Ordinal);
    }

    public static string DefaultCacheDir => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "cache", "catalog");

    /// <summary>The catalog, or null when there is neither a reachable one nor a copy on disk (the caller falls back to what it ships with).</summary>
    public async Task<CatalogSnapshot?> LoadAsync(CancellationToken ct = default)
    {
        bool fromCache = false;
        var text = await GetAsync(gamesBase + GameList.FileName, ct).ConfigureAwait(false);
        var list = text is null ? null : GameList.Parse(text);
        if (list is not null) Save(GameList.FileName, text!);
        else
        {
            fromCache = true;
            list = ReadCache(GameList.FileName) is { } c ? GameList.Parse(c) : null;
            if (list is null) return null;
        }

        var found = new CatalogGame?[list.Games.Count];
        using var gate = new SemaphoreSlim(6);
        await Task.WhenAll(list.Games.Select(async (entry, i) =>
        {
            await gate.WaitAsync(ct).ConfigureAwait(false);
            try { found[i] = await LoadGameAsync(entry, ct).ConfigureAwait(false); }
            finally { gate.Release(); }
        })).ConfigureAwait(false);

        var games = found.Where(g => g is not null).Select(g => g!).ToList();
        return new CatalogSnapshot(list, games, fromCache || games.Count < list.Games.Count);
    }

    private async Task<CatalogGame?> LoadGameAsync(GameEntry entry, CancellationToken ct)
    {
        var bases = new List<string>();
        if (useRepoReleases && entry.Repo.Length > 0)
        {
            if (FollowsBeta(entry.Id) && await PackChannels.NewestTagAsync(http, entry.Repo, ct).ConfigureAwait(false) is { } newest) bases.Add($"https://github.com/{entry.Repo}/releases/download/{newest}/");
            bases.Add($"https://github.com/{entry.Repo}/releases/latest/download/");
        }
        bases.Add($"{gamesBase}games/{entry.Id}/");

        foreach (var b in bases)
        {
            var text = await GetAsync(b + GameManifest.FileName, ct).ConfigureAwait(false);
            var m = text is null ? null : GameManifest.Parse(text);
            if (m is null || !string.Equals(m.Id, entry.Id, StringComparison.OrdinalIgnoreCase)) continue;
            Save($"game-{entry.Id}.json", text!);
            Save($"game-{entry.Id}.base", b);
            return new CatalogGame(entry, m, b);
        }
        // not reachable: the last copy, with the folder it came from
        var cached = ReadCache($"game-{entry.Id}.json");
        var baseOf = ReadCache($"game-{entry.Id}.base");
        var cm = cached is null ? null : GameManifest.Parse(cached);
        return cm is not null && baseOf is { Length: > 0 } && string.Equals(cm.Id, entry.Id, StringComparison.OrdinalIgnoreCase)
            ? new CatalogGame(entry, cm, baseOf) : null;
    }

    private async Task<string?> GetAsync(string url, CancellationToken ct)
    {
        try
        {
            using var resp = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, ct).ConfigureAwait(false);
            if (!resp.IsSuccessStatusCode) return null;
            if (resp.Content.Headers.ContentLength is > CatalogLimits.MaxFile) return null;
            var text = await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
            return text.Length <= CatalogLimits.MaxFile ? text : null;
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or IOException && !ct.IsCancellationRequested) { return null; }
    }

    private void Save(string name, string text)
    {
        try
        {
            Directory.CreateDirectory(cacheDir);
            string path = Path.Combine(cacheDir, name), tmp = path + ".tmp";
            File.WriteAllText(tmp, text);
            File.Move(tmp, path, overwrite: true);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { /* no cache is a lesser launcher, not a broken one */ }
    }

    private string? ReadCache(string name)
    {
        try { var p = Path.Combine(cacheDir, name); return File.Exists(p) ? File.ReadAllText(p) : null; }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return null; }
    }
}
