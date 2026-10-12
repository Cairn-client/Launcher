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

        /// <summary>The search text matches a title, an id, a developer or a franchise, ignoring case.</summary>
        public static bool Matches(GameProfile g, string query) =>
            query.Length == 0
            || g.DisplayName.Contains(query, StringComparison.OrdinalIgnoreCase)
            || g.Id.Contains(query, StringComparison.OrdinalIgnoreCase)
            || g.Developer.Contains(query, StringComparison.OrdinalIgnoreCase)
            || g.Franchise.Contains(query, StringComparison.OrdinalIgnoreCase);

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
