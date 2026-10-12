// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Text.Json;

namespace Cairn.Setup;

/// <summary>The packs to download to get one pack, in the order to download them (what it depends on first), or why that cannot be done.</summary>
public sealed record PackPlan(IReadOnlyList<PackInfo> Order, string? Problem)
{
    public bool Ok => Problem is null;
}

/// <summary>Works out which packs a download needs.</summary>
public static class PackPlanner
{
    private const int MaxDepth = 4;

    /// <summary>
    /// <paramref name="pack"/> plus every pack it depends on that is not on this PC at the version it asks for (or newer), dependencies first, each once.
    /// <paramref name="installed"/> says what is on this PC; <paramref name="offered"/> what the catalog offers for a pack id (null when it offers nothing).
    /// A dependency that is not on this PC and not on offer, or a loop of dependencies, is a problem and nothing is planned.
    /// </summary>
    public static PackPlan Plan(PackInfo pack, Func<string, InstalledPack?> installed, Func<string, PackInfo?> offered)
    {
        var order = new List<PackInfo>();
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var path = new List<string>();
        string? problem = Visit(pack, 0);
        return problem is null ? new PackPlan(order, null) : new PackPlan(Array.Empty<PackInfo>(), problem);

        string? Visit(PackInfo p, int depth)
        {
            if (depth > MaxDepth) return $"{p.Id} depends on too many packs";
            if (path.Contains(p.Id, StringComparer.OrdinalIgnoreCase)) return $"{p.Id} depends on itself";
            path.Add(p.Id);
            foreach (var dep in p.Dependencies)
            {
                if (IsEnough(installed(dep.Id), dep.Min) || seen.Contains(dep.Id)) continue;
                var o = offered(dep.Id);
                if (o is null) return $"{p.Id} needs {dep.Id} {dep.Min}, which is not available";
                if (!IsEnough(o.Version, dep.Min)) return $"{p.Id} needs {dep.Id} {dep.Min}, and only {o.Version} is available";
                string? sub = Visit(o, depth + 1);
                if (sub is not null) return sub;
            }
            path.RemoveAt(path.Count - 1);
            if (seen.Add(p.Id)) order.Add(p);
            return null;
        }
    }

    private static bool IsEnough(InstalledPack? installed, string min) => installed is not null && IsEnough(installed.Version, min);

    private static bool IsEnough(string version, string min) =>
        Version.TryParse(version, out var have) && Version.TryParse(min, out var need) && have >= need;
}

/// <summary>Puts a content pack's files where a game or its mod looks for them. Never overwrites a file the player already has.</summary>
public static class ContentInstall
{
    /// <summary>Copies every file under <paramref name="source"/> that is not already under <paramref name="target"/>; returns how many were copied.</summary>
    public static int CopyMissing(string source, string target)
    {
        if (!Directory.Exists(source)) return 0;
        string root = Path.GetFullPath(target) + Path.DirectorySeparatorChar;
        int copied = 0;
        foreach (var file in Directory.EnumerateFiles(source, "*", SearchOption.AllDirectories))
        {
            string dest = Path.GetFullPath(Path.Combine(target, Path.GetRelativePath(source, file)));
            if (!dest.StartsWith(root, StringComparison.OrdinalIgnoreCase) || File.Exists(dest)) continue;
            Directory.CreateDirectory(Path.GetDirectoryName(dest)!);
            File.Copy(file, dest);
            copied++;
        }
        return copied;
    }
}

/// <summary>Which release of a mod repository a player follows: the latest stable release, or the newest one including pre-releases.</summary>
public static class PackChannels
{
    public const string Stable = "stable";
    public const string Beta = "beta";

    public static bool IsKnown(string? channel) => channel is Stable or Beta;

    /// <summary>
    /// The catalog of the repository's newest release, pre-releases included (the GitHub release list is newest first). A pre-release is how a
    /// mod's maintainers offer a version to the players who asked for beta; the stable catalog (<see cref="PackCatalog.FetchAsync"/>) never sees it.
    /// Null when the list cannot be read or the newest release has no catalog.
    /// </summary>
    public static async Task<PackIndex?> FetchBetaAsync(HttpClient http, string? repo, CancellationToken ct = default)
    {
        if (!PackCatalog.IsRepo(repo)) return null;
        try
        {
            using var req = new HttpRequestMessage(HttpMethod.Get, $"https://api.github.com/repos/{repo}/releases?per_page=10");
            req.Headers.Accept.ParseAdd("application/vnd.github+json");
            using var resp = await http.SendAsync(req, ct).ConfigureAwait(false);
            if (!resp.IsSuccessStatusCode) return null;
            using var doc = JsonDocument.Parse(await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false));
            foreach (var rel in doc.RootElement.EnumerateArray())
            {
                if (rel.TryGetProperty("draft", out var d) && d.ValueKind == JsonValueKind.True) continue;
                string tag = rel.TryGetProperty("tag_name", out var t) && t.ValueKind == JsonValueKind.String ? t.GetString() ?? "" : "";
                if (!System.Text.RegularExpressions.Regex.IsMatch(tag, @"^[0-9]+\.[0-9]+\.[0-9]+$")) continue;
                using var idx = await http.GetAsync($"https://github.com/{repo}/releases/download/{tag}/{PackIndex.FileName}", ct).ConfigureAwait(false);
                if (!idx.IsSuccessStatusCode) return null;
                return PackIndex.Parse(await idx.Content.ReadAsStringAsync(ct).ConfigureAwait(false));
            }
            return null;
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or JsonException or InvalidOperationException && !ct.IsCancellationRequested) { return null; }
    }

    /// <summary>The newest published release of the repository, pre-releases included (a bare X.Y.Z tag), or null. Beta follows it for the pack and for the game's own game.json.</summary>
    public static async Task<string?> NewestTagAsync(HttpClient http, string? repo, CancellationToken ct = default)
    {
        if (!PackCatalog.IsRepo(repo)) return null;
        try
        {
            using var req = new HttpRequestMessage(HttpMethod.Get, $"https://api.github.com/repos/{repo}/releases?per_page=10");
            req.Headers.Accept.ParseAdd("application/vnd.github+json");
            using var resp = await http.SendAsync(req, ct).ConfigureAwait(false);
            if (!resp.IsSuccessStatusCode) return null;
            using var doc = JsonDocument.Parse(await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false));
            foreach (var rel in doc.RootElement.EnumerateArray())
            {
                if (rel.TryGetProperty("draft", out var d) && d.ValueKind == JsonValueKind.True) continue;
                string tag = rel.TryGetProperty("tag_name", out var t) && t.ValueKind == JsonValueKind.String ? t.GetString() ?? "" : "";
                if (System.Text.RegularExpressions.Regex.IsMatch(tag, @"^[0-9]+\.[0-9]+\.[0-9]+$")) return tag;
            }
            return null;
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or JsonException or InvalidOperationException && !ct.IsCancellationRequested) { return null; }
    }
    /// <summary>The newer of the two catalogs' packs for the channel: beta follows the newest release, stable only the stable one.</summary>
    public static PackInfo? Choose(string? channel, PackInfo? stable, PackInfo? beta)
    {
        if (channel != Beta || beta is null) return stable;
        if (stable is null) return beta;
        return Version.TryParse(beta.Version, out var b) && Version.TryParse(stable.Version, out var s) && b > s ? beta : stable;
    }
}
