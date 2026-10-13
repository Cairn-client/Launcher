// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.AspNetCore.Components;
using Cairn.Launcher.Models;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// The game list on the left, laid out like folders: each group (a letter, a studio or a franchise) opens and closes, all of
    /// them can be opened or closed at once, the order can be flipped, and what the player chose is remembered.
    /// </summary>
    public partial class Home
    {
        private readonly GameListPrefs listPrefs = GameListPrefs.Load();
        private HashSet<string>? collapsedFolders;

        private GameFilter gameFilterMode => listPrefs.Filter;
        private GameGrouping gameGrouping => listPrefs.Grouping;
        private bool listDescending => listPrefs.Descending;

        private HashSet<string> Collapsed => collapsedFolders ??= new HashSet<string>(listPrefs.Collapsed, StringComparer.OrdinalIgnoreCase);

        // ---- the library: the games the player added from the Store -------------------------------------------------------------

        /// <summary>
        /// The library of a player who had Cairn before the Store is worked out once: the games that are on this PC, that have a mod downloaded, that are
        /// favourites or that were played. A new player starts with nothing.
        /// </summary>
        private List<string> LibraryIds()
        {
            if (listPrefs.Library is { } have) return have;
            var start = GameProfileRegistry.All()
                .Where(g => IsModDownloaded(g) || IsFavorite(g.Id) || PlayedOf(g.Id) is not null || (g.IsSupported && IsGameFound(g)))
                .Select(g => g.Id).ToList();
            listPrefs.Library = start;
            listPrefs.Save();
            return start;
        }

        private bool InLibrary(string gameId) => LibraryIds().Contains(gameId, StringComparer.OrdinalIgnoreCase);

        private IReadOnlyList<GameProfile> LibraryGames() => GameProfileRegistry.All().Where(g => InLibrary(g.Id)).ToList();

        private void AddToLibrary(string gameId)
        {
            if (InLibrary(gameId)) return;
            listPrefs.Library!.Add(gameId);
            listPrefs.Save();
        }

        private void RemoveFromLibrary(string gameId)
        {
            LibraryIds().RemoveAll(x => string.Equals(x, gameId, StringComparison.OrdinalIgnoreCase));
            listPrefs.Save();
            if (string.Equals(selectedGameId, gameId, StringComparison.OrdinalIgnoreCase) && LibraryGames().FirstOrDefault() is { } next) SelectGame(next.Id);
        }

        private IReadOnlyList<GameGroup> GameGroups() =>
            GameListView.Build(LibraryGames(), gameFilter, gameFilterMode, gameGrouping, IsGameFound, IsModInstalled, listDescending, g => IsFavorite(g.Id),
                g => PlayedOf(g.Id)?.LastUtc, listPrefs.RecentFirst);

        /// <summary>While the player is searching, every folder with a match is shown open: a closed folder must not hide what was asked for.</summary>
        private bool Searching => gameFilter.Trim().Length > 0;

        private bool IsFolderOpen(GameGroup group) =>
            Searching || !Collapsed.Contains(GameListView.FolderKey(gameGrouping, group.Header));

        private void ToggleFolder(GameGroup group)
        {
            string key = GameListView.FolderKey(gameGrouping, group.Header);
            if (!Collapsed.Remove(key)) Collapsed.Add(key);
            SaveListPrefs();
        }

        private void OpenAllFolders()
        {
            foreach (var g in GameGroups()) Collapsed.Remove(GameListView.FolderKey(gameGrouping, g.Header));
            SaveListPrefs();
        }

        private void CloseAllFolders()
        {
            foreach (var g in GameGroups()) Collapsed.Add(GameListView.FolderKey(gameGrouping, g.Header));
            SaveListPrefs();
        }

        /// <summary>A filter is a tick box: each one narrows the list, and none ticked shows every game.</summary>
        private void ToggleGameFilter(GameFilter f) { listPrefs.Filter ^= f; SaveListPrefs(); }

        private void SetGameGrouping(GameGrouping g) { listPrefs.Grouping = g; SaveListPrefs(); }

        private void OnGroupingChanged(ChangeEventArgs e)
        {
            if (Enum.TryParse<GameGrouping>(e.Value?.ToString(), out var g)) SetGameGrouping(g);
        }

        private void ToggleRecentFirst() { listPrefs.RecentFirst = !listPrefs.RecentFirst; SaveListPrefs(); }

        private void FlipListOrder() { listPrefs.Descending = !listPrefs.Descending; SaveListPrefs(); }

        private void SaveListPrefs()
        {
            listPrefs.Collapsed = Collapsed.OrderBy(x => x, StringComparer.OrdinalIgnoreCase).ToList();
            listPrefs.Save();
        }

        private string EmptyListMessage() =>
            Searching ? $"No game matches \"{gameFilter.Trim()}\"."
            : gameFilterMode != GameFilter.All ? "No game fits the ticked filters. Untick one to see more."
            : "Your library is empty. Games you add show up here.";

    }
}
