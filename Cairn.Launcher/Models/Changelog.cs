// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;
using System.Net.Http;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace Cairn.Launcher.Models
{
    /// <summary>The pages of the launcher's window.</summary>
    public enum AppView
    {
        Home,
        Library,
        Settings,
        Versions,
        Help,
        Profile,
    }

    /// <summary>One release and what changed in it (the bullet lines of its notes).</summary>
    public sealed record ReleaseNote(string Tag, DateTime? Published, bool Prerelease, IReadOnlyList<string> Items);

    /// <summary>
    /// The changelog the Versions page shows: the project's GitHub releases, newest first. Release notes here are the changelog and
    /// nothing else (docs/VERSIONING.md), so each bullet line is one change.
    /// </summary>
    public static class Changelog
    {
        public const string Url = "https://api.github.com/repos/Cairn-client/launcher/releases?per_page=30";
        public const string PageUrl = "https://github.com/Cairn-client/launcher/releases";

        /// <summary>Reads GitHub's releases JSON; drafts are skipped, the newest version comes first, a release without bullets has no items.</summary>
        public static IReadOnlyList<ReleaseNote> Parse(string json)
        {
            var list = new List<(Version? V, ReleaseNote N)>();
            try
            {
                using var doc = JsonDocument.Parse(json);
                foreach (var r in doc.RootElement.EnumerateArray())
                {
                    if (r.TryGetProperty("draft", out var d) && d.ValueKind == JsonValueKind.True) continue;
                    string tag = r.TryGetProperty("tag_name", out var t) ? t.GetString() ?? "" : "";
                    if (tag.Length == 0) continue;
                    DateTime? when = r.TryGetProperty("published_at", out var p) && p.ValueKind == JsonValueKind.String
                                     && DateTime.TryParse(p.GetString(), null, System.Globalization.DateTimeStyles.AdjustToUniversal, out var dt) ? dt : null;
                    bool pre = r.TryGetProperty("prerelease", out var pr) && pr.ValueKind == JsonValueKind.True;
                    string body = r.TryGetProperty("body", out var b) && b.ValueKind == JsonValueKind.String ? b.GetString() ?? "" : "";
                    Version.TryParse(tag.TrimStart('v', 'V'), out var v);
                    list.Add((v, new ReleaseNote(tag, when, pre, Items(body))));
                }
            }
            catch (JsonException) { return Array.Empty<ReleaseNote>(); }
            catch (InvalidOperationException) { return Array.Empty<ReleaseNote>(); }
            return list.OrderByDescending(x => x.V ?? new Version(0, 0)).ThenByDescending(x => x.N.Published).Select(x => x.N).ToList();
        }

        /// <summary>The bullet lines of a release's notes, without the bullet and markdown marks.</summary>
        public static IReadOnlyList<string> Items(string body) =>
            (body ?? "").Replace("\r", "").Split('\n')
                .Select(l => l.Trim())
                .Where(l => l.StartsWith("- ") || l.StartsWith("* "))
                .Select(l => l[2..].Replace("**", "").Replace("`", "").Trim())
                .Where(l => l.Length > 0)
                .ToList();

        public static string DefaultCacheFile => System.IO.Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "changelog-cache.json");

        /// <summary>
        /// The releases, or null when they cannot be reached (offline, rate-limited). With a <paramref name="cacheFile"/> the last good answer is
        /// kept on disk and used when the network fails, so the changelog still shows offline.
        /// </summary>
        public static async Task<IReadOnlyList<ReleaseNote>?> FetchAsync(HttpClient http, CancellationToken ct = default, string? cacheFile = null)
        {
            try
            {
                using var req = new HttpRequestMessage(HttpMethod.Get, Url);
                req.Headers.UserAgent.ParseAdd("Cairn-launcher");
                req.Headers.Accept.ParseAdd("application/vnd.github+json");
                using var resp = await http.SendAsync(req, ct).ConfigureAwait(false);
                if (resp.IsSuccessStatusCode)
                {
                    string json = await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false);
                    var notes = Parse(json);
                    if (notes.Count > 0)
                    {
                        if (cacheFile is not null) TryWrite(cacheFile, json);
                        return notes;
                    }
                }
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException && !ct.IsCancellationRequested) { /* offline: the cache */ }
            return cacheFile is null ? null : FromCache(cacheFile);
        }

        /// <summary>The latest releases of one repository (a game's mod), newest first; null when they cannot be reached.</summary>
        public static async Task<IReadOnlyList<ReleaseNote>?> FetchRepoAsync(HttpClient http, string repo, int count = 3, CancellationToken ct = default)
        {
            try
            {
                using var req = new HttpRequestMessage(HttpMethod.Get, $"https://api.github.com/repos/{repo}/releases?per_page={count}");
                req.Headers.UserAgent.ParseAdd("Cairn-launcher");
                req.Headers.Accept.ParseAdd("application/vnd.github+json");
                using var resp = await http.SendAsync(req, ct).ConfigureAwait(false);
                if (!resp.IsSuccessStatusCode) return null;
                var notes = Parse(await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false));
                return notes.Take(count).ToList();
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException && !ct.IsCancellationRequested) { return null; }
        }

        private static IReadOnlyList<ReleaseNote>? FromCache(string file)
        {
            try
            {
                if (!System.IO.File.Exists(file)) return null;
                var notes = Parse(System.IO.File.ReadAllText(file));
                return notes.Count == 0 ? null : notes;
            }
            catch (Exception ex) when (ex is System.IO.IOException or UnauthorizedAccessException) { return null; }
        }

        private static void TryWrite(string file, string json)
        {
            try
            {
                System.IO.Directory.CreateDirectory(System.IO.Path.GetDirectoryName(file)!);
                System.IO.File.WriteAllText(file, json);
            }
            catch (Exception ex) when (ex is System.IO.IOException or UnauthorizedAccessException) { /* a cache that cannot be saved is not an error */ }
        }
    }
}
