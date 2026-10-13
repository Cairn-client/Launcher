// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using System.IO;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>The data of the pages that are views over what the launcher already knows: Activity, and the cards of Home that summarise it.</summary>
    public partial class Home
    {
        /// <summary>Every game that was played from here, the latest first.</summary>
        private IReadOnlyList<(GameProfile Game, RecentSession Session)> ActivityRows() =>
            recent.Entries.OrderByDescending(e => e.LastUtc)
                .Select(e => (Game: GameProfileRegistry.Get(e.GameId), Session: e))
                .Where(x => x.Game is not null).Select(x => (x.Game!, x.Session)).ToList();

        // ---- the latest release of each mod the player has (or follows), kept for half an hour: the "From the mods" section of Home ----
        private IReadOnlyList<(GameProfile Game, IReadOnlyList<ReleaseNote> Notes)> modNews = Array.Empty<(GameProfile, IReadOnlyList<ReleaseNote>)>();
        private bool modNewsLoading;
        private DateTime modNewsAt = DateTime.MinValue;

        private async Task LoadModNewsAsync(bool force = false)
        {
            if (modNewsLoading || (!force && DateTime.UtcNow - modNewsAt < TimeSpan.FromMinutes(30))) return;
            modNewsLoading = true;
            await InvokeAsync(StateHasChanged);
            try
            {
                var games = GameProfileRegistry.All().Where(g => g.ModRepo.Length > 0 && (IsModDownloaded(g) || HomeFavorites.Contains(g.Id) || recent.Entries.Any(e => e.GameId == g.Id)))
                    .OrderBy(g => g.DisplayName, StringComparer.OrdinalIgnoreCase).Take(10).ToList();
                var asks = games.Select(async g => (Game: g, Notes: await Changelog.FetchRepoAsync(PackHttp, g.ModRepo, 2))).ToList();
                var done = await Task.WhenAll(asks);
                modNews = done.Where(d => d.Notes is { Count: > 0 }).Select(d => (d.Game, d.Notes!)).ToList();
                modNewsAt = DateTime.UtcNow;
            }
            catch (Exception ex) { Log.Debug(ex, "news: the mods'' releases could not be read"); }
            finally { modNewsLoading = false; await InvokeAsync(StateHasChanged); }
        }
        // ---- the Profile page ----
        private (string Id, string Name, string Avatar) steamSelf = ("", "", "");

        private SteamProfileStatus.ProfileInfo? steamSelfInfo;
        private DateTime steamSelfInfoAt = DateTime.MinValue;

        /// <summary>What the player's own Steam profile page says (names, since when, most played games), read at most every ten minutes.</summary>
        private async Task LoadSteamSelfInfoAsync()
        {
            if (!settings.FriendStatus || steamSelf.Id.Length == 0 || DateTime.UtcNow - steamSelfInfoAt < TimeSpan.FromMinutes(10)) return;
            steamSelfInfoAt = DateTime.UtcNow;
            steamSelfInfo = await SteamProfileStatus.FetchInfoAsync(steamProfiles, steamSelf.Id, CancellationToken.None);
            await InvokeAsync(StateHasChanged);
        }

        private void LoadSteamSelf()
        {
            var me = SteamIdentity.Read(RealSetupHost.Instance.SteamRootFromRegistry());
            if (me is not null) steamSelf = (me.Id, me.Name, me.Avatar);
        }

        // ---- the cards and sections of Home that point at something ----

        /// <summary>Games on this PC that Cairn has a mod for and the player has none of yet (three at most).</summary>
        private IReadOnlyList<GameProfile> HomeSuggestions() =>
            GameProfileRegistry.All().Where(g => g.IsSupported && g.ModRepo.Length > 0 && IsGameFound(g) && !IsModDownloaded(g) && packBusyId != g.Id)
                .OrderBy(g => g.DisplayName, StringComparer.OrdinalIgnoreCase).Take(3).ToList();

        private IReadOnlyList<GameProfile> HomeRecentGames() =>
            recent.Entries.OrderByDescending(e => e.LastUtc).Select(e => GameProfileRegistry.Get(e.GameId)).Where(g => g is not null && g.BackgroundImage.Length > 0).Select(g => g!).Take(8).ToList();

        private string HomeMostPlayed() =>
            recent.Entries.OrderByDescending(e => e.Minutes).ThenByDescending(e => e.Sessions).Select(e => GameProfileRegistry.Get(e.GameId)?.DisplayName).FirstOrDefault(n => n is not null) ?? "-";
    }
}
