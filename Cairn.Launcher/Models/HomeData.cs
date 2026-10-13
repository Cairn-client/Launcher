// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;

namespace Cairn.Launcher.Models
{
    /// <summary>One game's play history on this PC.</summary>
    public sealed class RecentSession
    {
        public string GameId { get; set; } = "";
        /// <summary>"host" or "join": how the last session was started.</summary>
        public string Role { get; set; } = "";
        public DateTime LastUtc { get; set; }
        public int Sessions { get; set; }
        /// <summary>Minutes the game was running in the sessions started here, all together.</summary>
        public int Minutes { get; set; }
        /// <summary>The friend whose game the last session joined (empty when this player hosted, or joined by a code).</summary>
        public string With { get; set; } = "";
    }

    /// <summary>
    /// What the Home page's Continue card shows: which games were started from this launcher, how, and when.
    /// <c>%LOCALAPPDATA%\Cairn\recent.json</c>; missing or unreadable means no history.
    /// </summary>
    public sealed class RecentSessions
    {
        private readonly string _file;
        public List<RecentSession> Entries { get; private set; } = new();

        public static string DefaultFile => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "recent.json");

        public RecentSessions(string? file = null)
        {
            _file = file ?? DefaultFile;
            try
            {
                if (File.Exists(_file))
                    Entries = JsonSerializer.Deserialize<List<RecentSession>>(File.ReadAllText(_file)) ?? new();
            }
            catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException) { Entries = new(); }
        }

        /// <summary>The most recently played game, or null when nothing was played yet.</summary>
        public RecentSession? Last => Entries.OrderByDescending(e => e.LastUtc).FirstOrDefault();

        /// <summary>Notes that a session of <paramref name="gameId"/> was started now (best effort: history is not worth interrupting anyone for).</summary>
        public void Record(string gameId, string role, DateTime? nowUtc = null, string with = "")
        {
            if (string.IsNullOrWhiteSpace(gameId)) return;
            var e = Entries.FirstOrDefault(x => string.Equals(x.GameId, gameId, StringComparison.OrdinalIgnoreCase));
            if (e is null) Entries.Add(e = new RecentSession { GameId = gameId });
            e.Role = role;
            e.LastUtc = nowUtc ?? DateTime.UtcNow;
            e.Sessions++;
            e.With = role == "join" ? with : "";
            Save();
        }

        /// <summary>The game of a session closed after this many minutes: added to what the game has been played so far.</summary>
        public void AddPlaytime(string gameId, int minutes)
        {
            var e = Entries.FirstOrDefault(x => string.Equals(x.GameId, gameId, StringComparison.OrdinalIgnoreCase));
            if (e is null || minutes < 1) return;
            e.Minutes += minutes;
            Save();
        }

        private void Save()
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(_file)!);
                File.WriteAllText(_file, JsonSerializer.Serialize(Entries, new JsonSerializerOptions { WriteIndented = true }));
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
        }
    }

    /// <summary>One thing on Home that wants the player: what is wrong or new, and the button that deals with it.</summary>
    public sealed record HomeAttention(string Text, string Label, Func<Task> Do);

    /// <summary>One line of the Home page's Mods card.</summary>
    public sealed record HomeModRow(GameProfile Game, ModState State, string? Version);

    /// <summary>Where a game's mod stands, for the Home page's Mods card.</summary>
    public enum ModState
    {
        /// <summary>This release has no mod for the game.</summary>
        NotAvailable,
        NotDownloaded,
        /// <summary>Downloaded, but not in the game's Mods folder.</summary>
        NotInstalled,
        /// <summary>Installed, and this release offers a different version.</summary>
        UpdateAvailable,
        Installed,
    }

    public static class HomeLogic
    {
        /// <summary>The catalog's version is newer than the downloaded one. An older one (the stable channel after a beta was downloaded) is not an update.</summary>
        public static bool IsNewerPack(string catalogVersion, string downloadedVersion) =>
            Cairn.Wire.ReleaseVersionCompare.Compare(downloadedVersion, catalogVersion) switch
            {
                Cairn.Wire.ReleaseVersionComparison.LocalIsOlder => true,
                Cairn.Wire.ReleaseVersionComparison.Ambiguous => !string.Equals(catalogVersion, downloadedVersion, StringComparison.Ordinal),
                _ => false,
            };

        public static ModState ModStateOf(string? catalogVersion, string? downloadedVersion, bool installed)
        {
            if (downloadedVersion is null) return catalogVersion is null ? ModState.NotAvailable : ModState.NotDownloaded;
            if (catalogVersion is not null && IsNewerPack(catalogVersion, downloadedVersion)) return ModState.UpdateAvailable;
            return installed ? ModState.Installed : ModState.NotInstalled;
        }

        /// <summary>"45 min", "2 h 5 min", "13 h": the time a game has been played.</summary>
        public static string Played(int minutes) =>
            minutes < 60 ? $"{minutes} min" : minutes % 60 == 0 || minutes >= 600 ? $"{minutes / 60} h" : $"{minutes / 60} h {minutes % 60} min";

        /// <summary>"just now", "5 minutes ago", "yesterday", "3 days ago", "2 weeks ago", "4 months ago".</summary>
        public static string Ago(DateTime thenUtc, DateTime nowUtc)
        {
            var d = nowUtc - thenUtc;
            if (d < TimeSpan.FromMinutes(1)) return "just now";
            if (d < TimeSpan.FromHours(1)) return Plural((int)d.TotalMinutes, "minute");
            if (d < TimeSpan.FromDays(1)) return Plural((int)d.TotalHours, "hour");
            if (d < TimeSpan.FromDays(2)) return "yesterday";
            if (d < TimeSpan.FromDays(14)) return $"{(int)d.TotalDays} days ago";
            if (d < TimeSpan.FromDays(60)) return Plural((int)(d.TotalDays / 7), "week");
            return Plural((int)(d.TotalDays / 30), "month");
        }

        /// <summary>The names in <paramref name="now"/> that were not in <paramref name="before"/> (null before = the first look: nothing is new).</summary>
        public static IReadOnlyList<string> NewlyHosting(IEnumerable<string>? before, IEnumerable<string> now)
        {
            if (before is null) return Array.Empty<string>();
            var known = new HashSet<string>(before, StringComparer.OrdinalIgnoreCase);
            return now.Where(n => !known.Contains(n)).Distinct(StringComparer.OrdinalIgnoreCase).ToList();
        }

        private static string Plural(int n, string unit) => $"{n} {unit}{(n == 1 ? "" : "s")} ago";

        /// <summary>The newest release when it is newer than <paramref name="current"/>; null when the running version is the newest (or either is not a version).</summary>
        public static ReleaseNote? NewerRelease(string current, IReadOnlyList<ReleaseNote>? releases)
        {
            if (releases is null || !TryVersion(current, out var cur)) return null;
            ReleaseNote? best = null; Version? bestV = null;
            foreach (var r in releases)
            {
                if (r.Prerelease || !TryVersion(r.Tag, out var v) || v <= cur) continue;
                if (bestV is null || v > bestV) { best = r; bestV = v; }
            }
            return best;
        }

        private static bool TryVersion(string s, out Version v)
        {
            s = (s ?? "").Trim().TrimStart('v', 'V');
            int cut = s.IndexOfAny(new[] { '+', '-' });
            if (cut > 0) s = s[..cut];
            return Version.TryParse(s, out v!);
        }
    }
}
