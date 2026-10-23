// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
using System.IO;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.RegularExpressions;
using Serilog;

namespace Cairn.Launcher.Components.Shared
{
    /// <summary>A newer release: its tag, its page, and (when GitHub lists it) the installer to download with its SHA-256.</summary>
    public sealed record UpdateInfo(string Tag, string PageUrl, string? DownloadUrl, string? Sha256, long Size, string Notes = "");

    /// <summary>
    /// Cairn: "a newer version is out", and one click to install it. Once at start the launcher asks GitHub (api.github.com, this
    /// repository's releases, no account, nothing sent but the request itself) for the newest release and compares its tag with its own
    /// version. The Update button downloads that release's installer (only from this repository's own release address), checks it against
    /// the SHA-256 GitHub lists for the file, and runs it silently over the installed copy: same folder, same shortcuts' choices, the
    /// old shortcuts replaced by the new ones. Any failure of the check itself (offline, rate limit) is silence.
    /// </summary>
    public static class UpdateCheck
    {
        private const string ReleasesUrl = "https://api.github.com/repos/Cairn-client/Launcher/releases?per_page=8";
        private const string DownloadPrefix = "https://github.com/Cairn-client/Launcher/releases/download/";

        /// <summary>Both versions as four numbers, so 0.10 and 0.10.0 are the same (System.Version leaves a missing part at -1).</summary>
        public static Version? Normalize(string? s)
        {
            if (string.IsNullOrWhiteSpace(s) || !Version.TryParse(s.Trim().TrimStart('v', 'V'), out var v)) return null;
            return new Version(v.Major, v.Minor, Math.Max(v.Build, 0), Math.Max(v.Revision, 0));
        }

        /// <summary>The newest release of the given list when it is newer than <paramref name="current"/>; otherwise null.</summary>
        public static UpdateInfo? Newer(string releasesJson, string current)
        {
            var cur = Normalize(current);
            if (cur is null) return null;
            UpdateInfo? best = null; Version? bestV = null;
            using var doc = JsonDocument.Parse(releasesJson);
            foreach (var r in doc.RootElement.EnumerateArray())
            {
                if (r.TryGetProperty("draft", out var d) && d.GetBoolean()) continue;
                string? tag = r.TryGetProperty("tag_name", out var t) ? t.GetString() : null;
                var v = Normalize(tag);
                if (v is null || tag is null) continue;
                if (bestV is not null && v <= bestV) continue;
                string page = r.TryGetProperty("html_url", out var u) ? u.GetString() ?? "" : "";
                string body = r.TryGetProperty("body", out var b) ? b.GetString() ?? "" : "";
                string? url = null, sha = null; long size = 0;
                if (r.TryGetProperty("assets", out var assets))
                    foreach (var a in assets.EnumerateArray())
                    {
                        string name = a.TryGetProperty("name", out var n) ? n.GetString() ?? "" : "";
                        if (!name.StartsWith("Cairn-Setup-", StringComparison.Ordinal) || !name.EndsWith(".exe", StringComparison.Ordinal)) continue;
                        url = a.TryGetProperty("browser_download_url", out var du) ? du.GetString() : null;
                        size = a.TryGetProperty("size", out var sz) ? sz.GetInt64() : 0;
                        string? dg = a.TryGetProperty("digest", out var dgv) ? dgv.GetString() : null;
                        if (dg is not null && dg.StartsWith("sha256:", StringComparison.OrdinalIgnoreCase)) sha = dg[7..].ToLowerInvariant();
                        break;
                    }
                sha ??= Regex.Match(body, "sha256 `([0-9a-fA-F]{64})`").Groups[1].Value is { Length: 64 } m ? m.ToLowerInvariant() : null;
                if (url is not null && !url.StartsWith(DownloadPrefix, StringComparison.OrdinalIgnoreCase)) url = null;   // only this repository's own files
                best = new UpdateInfo(tag, page, url, sha, size, ShortNotes(body)); bestV = v;
            }
            return best is not null && bestV > cur ? best : null;
        }

        /// <summary>The release notes for a message box: no checksum line, no markdown marks, at most ~700 characters.</summary>
        public static string ShortNotes(string body)
        {
            var keep = body.Replace("\r", "").Split('\n')
                .Where(l => !l.Contains("sha256", StringComparison.OrdinalIgnoreCase))
                .Select(l => l.Replace("**", "").Replace("`", "").TrimStart('#', ' '))
                .Where(l => l.Trim().Length > 0);
            string s = string.Join("\n", keep);
            return s.Length <= 700 ? s : s[..700].TrimEnd() + "...";
        }

        /// <summary>True when the last <see cref="CheckAsync"/> could not ask GitHub (offline, rate limit), so "nothing new" was not an answer.</summary>
        public static bool LastCheckFailed { get; private set; }

        // the newest release publishes this file (its release workflow); a plain download is not held to the API's 60 requests an hour for one address
        private const string ManifestUrl = "https://github.com/Cairn-client/Launcher/releases/latest/download/update.json";

        /// <summary>The newest release as the manifest says (same shape as the API's list); the API itself when the manifest is not there (an older release has none) or cannot be read.</summary>
        private static async Task<string> ManifestOrReleasesAsync(HttpClient http)
        {
            try
            {
                string manifest = await http.GetStringAsync(ManifestUrl);
                using var doc = JsonDocument.Parse(manifest);
                if (doc.RootElement.ValueKind == JsonValueKind.Array && doc.RootElement.GetArrayLength() > 0) return manifest;
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or JsonException) { Log.Debug("update check: no manifest ({Why}), asking the API", ex.GetType().Name); }
            return await http.GetStringAsync(ReleasesUrl);
        }

        public static async Task<UpdateInfo?> CheckAsync(string current)
        {
            try
            {
                LastCheckFailed = false;
                using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(6) };
                http.DefaultRequestHeaders.UserAgent.ParseAdd("Cairn-launcher");
                http.DefaultRequestHeaders.Accept.ParseAdd("application/vnd.github+json");
                string json = await ManifestOrReleasesAsync(http);
                var n = Newer(json, current);
                Log.Information("update check: {Result}", n is null ? "this is the newest" : n.Tag + " is out");
                return n;
            }
            catch (Exception ex)
            {
                Log.Information("update check: not done ({Why})", ex.GetType().Name);
                LastCheckFailed = true;
                return null;
            }
        }

        /// <summary>
        /// Downloads the installer to the temp folder and checks its SHA-256. Returns the path, or null with the reason (the file is
        /// deleted when it does not match). <paramref name="progress"/> gets 0..1.
        /// </summary>
        public static async Task<(string? Path, string Why)> DownloadAsync(UpdateInfo info, IProgress<double>? progress, CancellationToken ct)
        {
            if (info.DownloadUrl is null || info.Sha256 is null) return (null, "this release has no installer file with a checksum");
            string dir = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "Cairn-update");
            Directory.CreateDirectory(dir);
            string file = System.IO.Path.Combine(dir, "Cairn-Setup-" + info.Tag + ".exe");
            try
            {
                using var http = new HttpClient { Timeout = TimeSpan.FromMinutes(20) };
                http.DefaultRequestHeaders.UserAgent.ParseAdd("Cairn-launcher");
                using var resp = await http.GetAsync(info.DownloadUrl, HttpCompletionOption.ResponseHeadersRead, ct);
                resp.EnsureSuccessStatusCode();
                long total = resp.Content.Headers.ContentLength ?? info.Size;
                using (var src = await resp.Content.ReadAsStreamAsync(ct))
                using (var dst = File.Create(file))
                {
                    var buf = new byte[256 * 1024]; long got = 0; int n;
                    while ((n = await src.ReadAsync(buf, ct)) > 0)
                    {
                        await dst.WriteAsync(buf.AsMemory(0, n), ct);
                        got += n;
                        if (total > 0) progress?.Report((double)got / total);
                    }
                }
                string have;
                using (var fs = File.OpenRead(file)) have = Convert.ToHexString(await SHA256.HashDataAsync(fs, ct)).ToLowerInvariant();
                if (have != info.Sha256)
                {
                    try { File.Delete(file); } catch { }
                    return (null, "the downloaded file does not match its checksum");
                }
                return (file, "");
            }
            catch (Exception ex)
            {
                try { File.Delete(file); } catch { }
                return (null, ex is OperationCanceledException ? "cancelled" : "the download failed (" + ex.GetType().Name + ")");
            }
        }
    }
}