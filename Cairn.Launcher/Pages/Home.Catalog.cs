// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// The game list is data, not code: the catalog (the Cairn-client/games repository, and each game's own repository) says which games there are
    /// and what the launcher shows for each; the launcher keeps the last copy for when it is offline. This part reads it, and shows what it
    /// announces: notices (known issues, news), mod versions it blocks, and the release channel a player follows.
    /// </summary>
    public partial class Home
    {
        private bool catalogLoading;

        /// <summary>Reads the catalog and makes it the list of games. A catalog that cannot be had changes nothing (the list is the last copy, or the seed).</summary>
        private async Task LoadCatalogAsync()
        {
            if (catalogLoading) return;
            catalogLoading = true;
            try
            {
                // CAIRN_GAMES_URL: a local copy of the games repository (a folder served over http), for working on the catalog
                var client = new CatalogClient(PackHttp, CatalogClient.DefaultCacheDir, Environment.GetEnvironmentVariable("CAIRN_GAMES_URL"));
                var snapshot = await client.LoadAsync();
                if (snapshot is null) { Log.Information("catalog: not available, the games are what the launcher knew"); return; }
                var missing = snapshot.List.Games.Where(e => snapshot.Games.All(g => g.Entry.Id != e.Id)).Select(e => e.Id).ToList();
                Log.Information("catalog: {Games} game(s){Cache}, {Notices} notice(s), {Blocked} blocked version(s){Missing}", snapshot.Games.Count,
                    snapshot.FromCache ? " (some from the last copy)" : "", snapshot.List.Notices.Count, snapshot.List.Blocked.Count,
                    missing.Count > 0 ? $", could not read: {string.Join(", ", missing)}" : "");
                GameProfileRegistry.ApplyCatalog(snapshot);
            }
            catch (Exception ex) { Log.Warning(ex, "catalog: could not be read"); }
            finally { catalogLoading = false; }
        }

        /// <summary>At start, and from "Check for updates": the games first (the mod repositories come from them), then what each mod offers.</summary>
        private async Task RefreshCatalogAndPacksAsync()
        {
            await LoadCatalogAsync();
            await LoadPackIndexAsync();
        }

        // the list changed: what was learned about the old one (where games are, what mods are on this PC) is asked again
        private void OnGamesChanged()
        {
            steamFoundCache.Clear();
            installedPackCache.Clear();
            if (SelectedProfile is null && GameProfileRegistry.All().FirstOrDefault() is { } first) selectedGameId = first.Id;
            _ = InvokeAsync(StateHasChanged);
        }

        // ---- notices -----------------------------------------------------------------------------------------------------------------

        /// <summary>The notices that apply now: for this launcher version, and for games this player has (a notice with no game is for everyone).</summary>
        private IReadOnlyList<Notice> CurrentNotices() =>
            GameProfileRegistry.Catalog.Notices
                .Where(n => n.IsCurrent(Globals.CairnVersion, DateTime.UtcNow, id => GameProfileRegistry.Get(id) is { } g && IsGameFound(g)))
                .ToList();

        /// <summary>What Home shows: the current notices the player has not closed.</summary>
        private IReadOnlyList<Notice> HomeNotices() => CurrentNotices().Where(n => !listPrefs.DismissedNotices.Contains(n.Id)).ToList();

        private void DismissNotice(string id)
        {
            if (!listPrefs.DismissedNotices.Contains(id)) listPrefs.DismissedNotices.Add(id);
            SaveListPrefs();
        }

        // ---- blocked mod versions ----------------------------------------------------------------------------------------------------

        /// <summary>Why the mod version on this PC must not be used (the catalog blocks it), or null.</summary>
        private string? BlockedReasonOfInstalled(GameProfile g) =>
            InstalledPackOf(g) is { } p && GameProfileRegistry.Catalog.BlockOf(p.Id, p.Version) is { } b ? b.Reason : null;

        // ---- release channel ---------------------------------------------------------------------------------------------------------

        private string ChannelOf(string gameId) =>
            settings.ModChannels.TryGetValue(gameId, out var c) && PackChannels.IsKnown(c) ? c : PackChannels.Stable;

        /// <summary>Switches a game's mod between stable releases and beta (pre-releases too), and looks again for what is on offer.</summary>
        private async Task ToggleChannelAsync(string gameId)
        {
            settings.ModChannels[gameId] = ChannelOf(gameId) == PackChannels.Beta ? PackChannels.Stable : PackChannels.Beta;
            WriteSettings("the release channel");
            ShowToast(ChannelOf(gameId) == PackChannels.Beta
                ? "You will be offered beta versions of this mod too. Both players should use the same one."
                : "You will be offered stable versions of this mod only.");
            await LoadPackIndexAsync();
        }
    }
}
