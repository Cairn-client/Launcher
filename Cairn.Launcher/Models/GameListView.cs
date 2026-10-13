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
    /// <summary>Which games the list shows: no flag shows every game, each flag narrows the list further.</summary>
    [Flags]
    public enum GameFilter
    {
        /// <summary>Every game Cairn knows.</summary>
        All = 0,
        /// <summary>Only games found on this PC (Steam, or a path the player gave).</summary>
        OnThisPc = 1,
        /// <summary>Only games whose mod has been downloaded.</summary>
        ModInstalled = 2,
        /// <summary>Only the games the player pinned with the heart.</summary>
        Favorites = 4,
    }

    /// <summary>How the list is organised into folders.</summary>
    public enum GameGrouping
    {
        /// <summary>A folder per first letter of the title.</summary>
        Title,
        /// <summary>A folder per studio.</summary>
        Developer,
        /// <summary>A folder per series or license, so a game and its sequel sit together whoever made them.</summary>
        Franchise,
    }

    public sealed record GameGroup(string Header, IReadOnlyList<GameProfile> Games);

    /// <summary>The sidebar's game list: the search box, the filter, the grouping and the order, as a pure function so it is testable.</summary>
    public static class GameListView
    {
        public static string UnknownGroup => "Other";

        public static IReadOnlyList<GameGroup> Build(
            IEnumerable<GameProfile> games, string? query, GameFilter filter, GameGrouping grouping,
            Func<GameProfile, bool> onThisPc, Func<GameProfile, bool> modInstalled, bool descending = false, Func<GameProfile, bool>? isFavorite = null,
            Func<GameProfile, DateTime?>? lastPlayed = null, bool recentFirst = false)
        {
            string q = (query ?? "").Trim();
            IEnumerable<GameProfile> shown = games.Where(g => Matches(g, q));
            if (filter.HasFlag(GameFilter.OnThisPc)) shown = shown.Where(onThisPc);
            if (filter.HasFlag(GameFilter.ModInstalled)) shown = shown.Where(modInstalled);
            if (filter.HasFlag(GameFilter.Favorites)) shown = shown.Where(g => isFavorite?.Invoke(g) ?? false);

            var cmp = StringComparer.OrdinalIgnoreCase;
            var ordered = (descending ? shown.OrderByDescending(g => g.DisplayName, cmp) : shown.OrderBy(g => g.DisplayName, cmp)).ToList();
            // "recently played" reorders the games inside each folder (the folders keep their order); never-played games follow by title
            if (recentFirst && lastPlayed is not null)
                ordered = ordered.OrderByDescending(g => lastPlayed(g) ?? DateTime.MinValue).ThenBy(g => g.DisplayName, cmp).ToList();

            Func<GameProfile, string> keyOf = grouping switch
            {
                GameGrouping.Developer => g => NameOr(g.Developer),
                GameGrouping.Franchise => g => NameOr(g.Franchise),
                _ => g => LetterOf(g.DisplayName),
            };

            // "Other" and "#" always come last, whichever way the folders are ordered
            var groups = ordered.GroupBy(keyOf, cmp).ToList();
            IEnumerable<IGrouping<string, GameProfile>> sorted = descending
                ? groups.OrderBy(x => IsCatchAll(x.Key) ? 1 : 0).ThenByDescending(x => x.Key, cmp)
                : groups.OrderBy(x => IsCatchAll(x.Key) ? 1 : 0).ThenBy(x => x.Key, cmp);
            return sorted.Select(x => new GameGroup(x.Key, x.ToList())).ToList();
        }

        /// <summary>The search text matches a game: see <see cref="Score"/>.</summary>
        public static bool Matches(GameProfile g, string query) => query.Trim().Length == 0 || Score(g, query) > 0;

        /// <summary>
        /// How well a game answers a search, 0 when it does not. Like a store's library search: accents, case and signs such as the trademark
        /// mark or a colon do not matter; the words may come in any order and a word may be only the start of one ("space prog" finds
        /// Kerbal Space Program); a short code finds a title by its initials ("sr2" finds Slime Rancher 2) or its id ("kcd2"); and the letters
        /// may be spread through the title ("kerbspace"). A title match ranks above a studio or franchise match, a word start above the
        /// middle of a word.
        /// </summary>
        public static int Score(GameProfile g, string query)
        {
            var words = Words(query);
            if (words.Count == 0) return 0;
            string title = Normalize(g.DisplayName), id = Normalize(g.Id), dev = Normalize(g.Developer), fran = Normalize(g.Franchise);
            var titleWords = title.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            string initials = string.Concat(titleWords.Select(w => w[0]));
            string compact = title.Replace(" ", "");
            int total = 0;
            foreach (string w in words)
            {
                int best = 0;
                if (titleWords.Any(t => t.StartsWith(w, StringComparison.Ordinal))) best = 40;
                else if (title.Contains(w, StringComparison.Ordinal)) best = 25;
                else if (id.Contains(w, StringComparison.Ordinal)) best = 22;
                else if (initials.StartsWith(w, StringComparison.Ordinal) && w.Length >= 2) best = 20;
                else if (Words(dev).Concat(Words(fran)).Any(t => t.StartsWith(w, StringComparison.Ordinal))) best = 12;
                else if (dev.Contains(w, StringComparison.Ordinal) || fran.Contains(w, StringComparison.Ordinal)) best = 8;
                else if (w.Length >= 3 && IsSubsequence(w, compact)) best = 4;
                if (best == 0) return 0;   // every word has to find something
                total += best;
            }
            if (title.StartsWith(string.Join(' ', words), StringComparison.Ordinal)) total += 30;   // the title begins with what was typed
            return total;
        }

        private static List<string> Words(string s) => Normalize(s).Split(' ', StringSplitOptions.RemoveEmptyEntries).ToList();

        /// <summary>Lower case, no accents, and every sign that is not a letter or a digit becomes a space ("Prey (2017)" and "PREYâ„¢" meet).</summary>
        public static string Normalize(string s)
        {
            if (string.IsNullOrEmpty(s)) return "";
            var decomposed = s.Normalize(System.Text.NormalizationForm.FormD);
            var sb = new System.Text.StringBuilder(decomposed.Length);
            foreach (char c in decomposed)
            {
                if (System.Globalization.CharUnicodeInfo.GetUnicodeCategory(c) == System.Globalization.UnicodeCategory.NonSpacingMark) continue;
                sb.Append(char.IsLetterOrDigit(c) ? char.ToLowerInvariant(c) : ' ');
            }
            return string.Join(' ', sb.ToString().Split(' ', StringSplitOptions.RemoveEmptyEntries));
        }

        private static bool IsSubsequence(string needle, string hay)
        {
            int i = 0;
            foreach (char c in hay) if (i < needle.Length && c == needle[i]) i++;
            return i == needle.Length;
        }

        private static string NameOr(string s) => string.IsNullOrWhiteSpace(s) ? UnknownGroup : s.Trim();

        private static bool IsCatchAll(string header) => header == UnknownGroup || header == "#";

        /// <summary>The heading letter for a title; titles that start with a digit or symbol share "#".</summary>
        public static string LetterOf(string title)
        {
            string t = title.TrimStart();
            return t.Length > 0 && char.IsLetter(t[0]) ? char.ToUpperInvariant(t[0]).ToString() : "#";
        }

        /// <summary>The folder's identity for remembering whether it is open: the grouping and its name, so "Valve" under developers and under titles never clash.</summary>
        public static string FolderKey(GameGrouping grouping, string header) => grouping + ":" + header;
    }

    /// <summary>
    /// What the player chose in the list and keeps between runs: the filter, the grouping, the order and which folders are closed.
    /// <c>%LOCALAPPDATA%\Cairn\list-prefs.json</c>; unreadable or missing means the defaults.
    /// </summary>
    public sealed class GameListPrefs
    {
        public GameFilter Filter { get; set; } = GameFilter.All;
        public GameGrouping Grouping { get; set; } = GameGrouping.Title;
        public bool Descending { get; set; }

        /// <summary>Inside each folder, the games played most recently come first.</summary>
        public bool RecentFirst { get; set; }
        public List<string> Collapsed { get; set; } = new();

        /// <summary>Games the player pinned (the heart on a poster): they get their own shelf on Home.</summary>
        public List<string> Favorites { get; set; } = new();

        /// <summary>The first-run tour has been seen (or skipped).</summary>
        public bool TourSeen { get; set; }

        /// <summary>The ids of the notices the player closed on Home; they stay in Help.</summary>
        public List<string> DismissedNotices { get; set; } = new();

        public static string DefaultFile => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "list-prefs.json");

        public static GameListPrefs Load(string? file = null)
        {
            try
            {
                file ??= DefaultFile;
                if (File.Exists(file))
                    return JsonSerializer.Deserialize<GameListPrefs>(File.ReadAllText(file)) ?? new GameListPrefs();
            }
            catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException) { /* unreadable: the defaults */ }
            return new GameListPrefs();
        }

        /// <summary>Best effort: a list setting that cannot be saved is not worth interrupting anyone for.</summary>
        public bool Save(string? file = null)
        {
            try
            {
                file ??= DefaultFile;
                Directory.CreateDirectory(Path.GetDirectoryName(file)!);
                File.WriteAllText(file, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
                return true;
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return false; }
        }
    }
}
