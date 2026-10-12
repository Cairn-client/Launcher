// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.IO;
using System.Net.Http;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace Cairn.Launcher.Models
{
    /// <summary>What a game's page shows about the game itself: how Steam players rate it, its Metacritic score, when it came out, its genres.</summary>
    public sealed record GameInfo(
        string? ReviewText, int? PositivePercent, long ReviewCount,
        int? Metacritic, string? MetacriticUrl, string? Released, IReadOnlyList<string> Genres)
    {
        public static readonly GameInfo Empty = new(null, null, 0, null, null, null, Array.Empty<string>());
        public bool IsEmpty => ReviewText is null && Metacritic is null && Released is null && Genres.Count == 0;
    }

    /// <summary>
    /// Steam's public store data (no key needed): the review summary and the store details of an app. Read once a day per game and kept on
    /// disk, so the page still shows something offline. OpenCritic and IGDB need a developer key, so the page only links to them.
    /// </summary>
    public static class GameInfoSource
    {
        public static string ReviewsUrl(uint appId) =>
            $"https://store.steampowered.com/appreviews/{appId}?json=1&language=all&purchase_type=all&num_per_page=0";

        public static string DetailsUrl(uint appId) =>
            $"https://store.steampowered.com/api/appdetails?appids={appId}&filters=metacritic,genres,release_date";

        public static string OpenCriticUrl(string title) => "https://opencritic.com/search?q=" + Uri.EscapeDataString(title);
        public static string IgdbUrl(string title) => "https://www.igdb.com/search?type=1&q=" + Uri.EscapeDataString(title);

        /// <summary>The two Steam answers as one record; either may be missing or unreadable (that part is then left out).</summary>
        public static GameInfo Parse(string? reviewsJson, string? detailsJson, uint appId)
        {
            string? text = null; int? pct = null; long count = 0;
            int? meta = null; string? metaUrl = null, released = null; var genres = new List<string>();

            try
            {
                if (!string.IsNullOrWhiteSpace(reviewsJson))
                {
                    using var doc = JsonDocument.Parse(reviewsJson);
                    if (doc.RootElement.TryGetProperty("query_summary", out var q))
                    {
                        long pos = q.TryGetProperty("total_positive", out var p) && p.TryGetInt64(out var pv) ? pv : 0;
                        long neg = q.TryGetProperty("total_negative", out var n) && n.TryGetInt64(out var nv) ? nv : 0;
                        count = pos + neg;
                        if (count > 0) pct = (int)Math.Round(100.0 * pos / count);
                        string? d = q.TryGetProperty("review_score_desc", out var ds) && ds.ValueKind == JsonValueKind.String ? ds.GetString() : null;
                        // "No user reviews" and the like are not a rating
                        if (count > 0 && !string.IsNullOrWhiteSpace(d)) text = d;
                    }
                }
            }
            catch (JsonException) { /* no reviews */ }

            try
            {
                if (!string.IsNullOrWhiteSpace(detailsJson))
                {
                    using var doc = JsonDocument.Parse(detailsJson);
                    if (doc.RootElement.TryGetProperty(appId.ToString(), out var app)
                        && app.TryGetProperty("success", out var ok) && ok.ValueKind == JsonValueKind.True
                        && app.TryGetProperty("data", out var data))
                    {
                        if (data.TryGetProperty("metacritic", out var m))
                        {
                            if (m.TryGetProperty("score", out var s) && s.TryGetInt32(out var sv)) meta = sv;
                            if (m.TryGetProperty("url", out var u) && u.ValueKind == JsonValueKind.String) metaUrl = CleanUrl(u.GetString());
                        }
                        if (data.TryGetProperty("release_date", out var r) && r.TryGetProperty("date", out var rd) && rd.ValueKind == JsonValueKind.String)
                        {
                            string? date = rd.GetString();
                            bool soon = r.TryGetProperty("coming_soon", out var cs) && cs.ValueKind == JsonValueKind.True;
                            released = string.IsNullOrWhiteSpace(date) ? (soon ? "Coming soon" : null) : (soon ? "Coming " + date : date);
                        }
                        if (data.TryGetProperty("genres", out var g) && g.ValueKind == JsonValueKind.Array)
                            foreach (var item in g.EnumerateArray())
                                if (item.TryGetProperty("description", out var gd) && gd.ValueKind == JsonValueKind.String && !string.IsNullOrWhiteSpace(gd.GetString()))
                                    genres.Add(gd.GetString()!);
                    }
                }
            }
            catch (JsonException) { /* no details */ }

            return new GameInfo(text, pct, count, meta, metaUrl, released, genres);
        }

        /// <summary>Only an https link, without Steam's tracking tag.</summary>
        private static string? CleanUrl(string? url)
        {
            if (!Uri.TryCreate(url, UriKind.Absolute, out var u) || u.Scheme != Uri.UriSchemeHttps) return null;
            return u.GetLeftPart(UriPartial.Path);
        }

        public static string DefaultCacheDir => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "game-info");

        private sealed record Cached(DateTime FetchedUtc, string? Reviews, string? Details);

        /// <summary>The game's info: from the disk when it is under a day old, else from Steam; the old copy when Steam cannot be reached.</summary>
        public static async Task<GameInfo> GetAsync(HttpClient http, uint appId, string? cacheDir = null, DateTime? nowUtc = null, CancellationToken ct = default)
        {
            if (appId == 0) return GameInfo.Empty;
            cacheDir ??= DefaultCacheDir;
            DateTime now = nowUtc ?? DateTime.UtcNow;
            string file = Path.Combine(cacheDir, appId + ".json");
            Cached? old = Read(file);
            if (old is not null && now - old.FetchedUtc < TimeSpan.FromHours(24)) return Parse(old.Reviews, old.Details, appId);

            string? reviews = await TryGet(http, ReviewsUrl(appId), ct).ConfigureAwait(false);
            string? details = await TryGet(http, DetailsUrl(appId), ct).ConfigureAwait(false);
            if (reviews is null && details is null)
                return old is null ? GameInfo.Empty : Parse(old.Reviews, old.Details, appId);   // offline: what was kept

            // a half answer keeps the other half from the old copy
            reviews ??= old?.Reviews; details ??= old?.Details;
            Write(file, new Cached(now, reviews, details));
            return Parse(reviews, details, appId);
        }

        private static async Task<string?> TryGet(HttpClient http, string url, CancellationToken ct)
        {
            try
            {
                using var resp = await http.GetAsync(url, ct).ConfigureAwait(false);
                return resp.IsSuccessStatusCode ? await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false) : null;
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException && !ct.IsCancellationRequested) { return null; }
        }

        private static Cached? Read(string file)
        {
            try { return File.Exists(file) ? JsonSerializer.Deserialize<Cached>(File.ReadAllText(file)) : null; }
            catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException) { return null; }
        }

        private static void Write(string file, Cached c)
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(file)!);
                File.WriteAllText(file, JsonSerializer.Serialize(c));
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { /* a cache that cannot be saved is not an error */ }
        }
    }
}
