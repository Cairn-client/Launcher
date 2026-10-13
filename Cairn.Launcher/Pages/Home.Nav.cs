// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using Cairn.Setup;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Threading.Tasks;
using Cairn.Launcher.Models;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// The window's pages. The top bar is the navbar: Home and Library on the left; Settings, Versions and Help on the right.
    /// Each one is a page in the main area (Library is the game list and the selected game's page); none is a popup.
    /// </summary>
    public partial class Home
    {
        private AppView currentView = AppView.Home;
        private AppView viewBeforeSettings = AppView.Home;

        /// <summary>Whether the Settings page is the one showing. Setting it opens the page, or goes back to where the player was.</summary>
        private bool showSettings
        {
            get => currentView == AppView.Settings;
            set
            {
                if (value)
                {
                    if (currentView != AppView.Settings) viewBeforeSettings = currentView;
                    currentView = AppView.Settings;
                }
                else if (currentView == AppView.Settings)
                {
                    currentView = viewBeforeSettings;
                }
            }
        }

        /// <summary>Goes to a page. Leaving Settings without saving puts the unsaved edits back, as closing the old window did.</summary>
        private void NavigateTo(AppView view)
        {
            if (view == currentView)
            {
                if (view == AppView.Library && SelectedProfile is { } alreadyShown) _ = LoadGameInfoAsync(alreadyShown);
                return;
            }
            if (currentView == AppView.Settings) CancelSettings();
            if (view == AppView.Settings) OpenSettings();
            else currentView = view;
            if (view is AppView.Versions or AppView.Home) _ = RefreshChangelogIfStaleAsync();   // a release made while the launcher was open shows up
            if (view == AppView.Home) _ = RefreshHomeFriendsIfStaleAsync();
            if (view == AppView.Home) _ = LoadModNewsAsync();
            if (view == AppView.Profile) { LoadSteamSelf(); _ = LoadSteamSelfInfoAsync(); _ = RefreshHomeFriendsIfStaleAsync(); }
            if (view == AppView.Library && SelectedProfile is { } shown) _ = LoadGameInfoAsync(shown);
        }

        // ---- Versions: the changelog ----------------------------------------------------------------------------------------

        private IReadOnlyList<ReleaseNote>? changelog;
        private bool changelogLoading;
        private System.DateTime changelogAt = System.DateTime.MinValue;
        private static readonly System.TimeSpan ChangelogMaxAge = System.TimeSpan.FromMinutes(3);

        private async Task LoadChangelogAsync()
        {
            if (changelogLoading) return;
            changelogLoading = true;
            try
            {
                var fetched = await Changelog.FetchAsync(PackHttp, default, Changelog.DefaultCacheFile);
                if (fetched is not null) { changelog = fetched; changelogAt = System.DateTime.UtcNow; }   // a failed refresh keeps what was shown
                else changelog ??= null;
                Log.Information("changelog: {Result}", fetched is null ? "not available" : $"{fetched.Count} release(s)");
            }
            finally
            {
                changelogLoading = false;
                await InvokeAsync(StateHasChanged);
            }
        }

        private Task ReloadChangelogAsync() => LoadChangelogAsync();

        private Task RefreshChangelogIfStaleAsync() =>
            System.DateTime.UtcNow - changelogAt > ChangelogMaxAge ? LoadChangelogAsync() : Task.CompletedTask;

        /// <summary>The Versions page's button: the update check, and a fresh look at the releases too.</summary>
        private async Task CheckUpdatesAndChangelogAsync()
        {
            _ = LoadChangelogAsync();
            await CheckUpdateNowAsync();
        }

        // ---- Home: the library at a glance ------------------------------------------------------------------------------------

        private int HomeGamesOnPc => GameProfileRegistry.All().Count(IsGameFound);
        private int HomeModsInstalled => GameProfileRegistry.All().Count(IsModInstalled);

        /// <summary>Home's game cards: show the game in the Library.</summary>
        private void OpenGameFromHome(string gameId)
        {
            SelectGame(gameId);
            NavigateTo(AppView.Library);
            StateHasChanged();
        }
        // ---- Home: continue, friends, mods, favourites, update banner, the tour --------------------------------------------------

        private readonly RecentSessions recent = new();

        /// <summary>A session of the selected game was just started from here (Host or Join): remembered for the Continue card.</summary>
        private void RecordSession(string role)
        {
            recent.Record(selectedGameId, role, with: joiningWith);
            joiningWith = "";
            sessionStartedUtc = DateTime.UtcNow;
        }

        // who the session being started joins (a friend picked from the list, or the sender of an invitation), and when the game started
        private string joiningWith = "";
        private DateTime? sessionStartedUtc;

        private RecentSession? HomeLast => recent.Last;

        private bool IsFavorite(string gameId) => listPrefs.Favorites.Contains(gameId, StringComparer.OrdinalIgnoreCase);

        private void ToggleFavorite(string gameId)
        {
            if (!listPrefs.Favorites.RemoveAll(f => string.Equals(f, gameId, StringComparison.OrdinalIgnoreCase)).Equals(1))
                listPrefs.Favorites.Add(gameId);
            listPrefs.Save();
        }

        private System.Collections.Generic.HashSet<string> HomeFavorites => new(listPrefs.Favorites, StringComparer.OrdinalIgnoreCase);

        // The friends who are hosting, asked of Steam quietly while Home is showing: it never opens a window, and a failure is just "unavailable".
        private List<SteamFriendData>? homeFriends;
        private string homeFriendsState = "idle";   // idle | checking | ok | unavailable
        private DateTime homeFriendsAt = DateTime.MinValue;

        private async Task RefreshHomeFriendsIfStaleAsync()
        {
            _ = RefreshSteamFriendStatusAsync();
            if (homeFriendsState == "checking" || DateTime.UtcNow - homeFriendsAt < TimeSpan.FromSeconds(50)) return;
            if (showJoinSteam || steamBusy || gameRunning || !setupChecked || !SetupAllGood) return;
            string? program = PackSessionProgram(FriendsGameId);
            if (program is null) { homeFriendsState = "unavailable"; return; }
            homeFriendsState = "checking";
            await InvokeAsync(StateHasChanged);
            try
            {
                var r = await PackSession.FriendsOnceAsync(program, settings.SteamAppId, GameExeOf(FriendsGameId) ?? "");
                if (r is { State: "ok" })
                {
                    var friends = r.Friends.Select(f => new SteamFriendData { Name = f.Name, Code = f.Code, Release = f.Release }).ToList();
                    var fresh = HomeLogic.NewlyHosting(homeFriends?.Select(f => f.Name), friends.Select(f => f.Name));
                    homeFriends = friends; homeFriendsState = "ok";
                    if (fresh.Count > 0 && settings.TrayNotices) TrayService.Notify("A friend is hosting", string.Join(", ", fresh) + (fresh.Count == 1 ? " is" : " are") + " hosting a game. Open Cairn to join.");
                }
                else homeFriendsState = "unavailable";
            }
            catch (Exception ex)
            {
                Log.Debug(ex, "home: Steam friends could not be asked");
                homeFriendsState = "unavailable";
            }
            homeFriendsAt = DateTime.UtcNow;
            await InvokeAsync(StateHasChanged);
        }

        /// <summary>Starts a loop that looks again once a minute while Home is the page showing.</summary>
        private void StartHomeFriendsLoop()
        {
            _ = Task.Run(async () =>
            {
                while (true)
                {
                    await Task.Delay(TimeSpan.FromSeconds(30));
                    if (currentView == AppView.Home) await RefreshHomeFriendsIfStaleAsync();
                }
            });
        }

        /// <summary>
        /// Opens the Join window with this friend's code already in it. Like the game page's Join button it needs the game's mod: without it
        /// the game would start as the plain game and never join (the Home card used to skip that check), so a mod not yet downloaded or
        /// placed is brought in first, and the join stops with a plain message if that cannot be done.
        /// </summary>
        private async Task JoinFriendFromHomeAsync(SteamFriendData friend)
        {
            joiningWith = friend.Name;
            if (FriendsGameId.Length > 0) SelectGame(FriendsGameId);
            if (gameRunning) { ShowToast("The game is already running. Close it first, or use Leave game on its page.", true); return; }
            if (SelectedProfile is { } g && NeedsMod(g))
            {
                if (!IsModDownloaded(g)) await DownloadPackAsync(g.Id);
                if (!IsModInstalled(g)) await InstallModAsync(g.Id);
                if (NeedsModGuard()) return;   // still no mod: told in plain words
            }
            await OpenJoinSteam();
            if (showJoinSteam) { steamCodeInput = friend.Code; await InvokeAsync(StateHasChanged); }
        }

        private IReadOnlyList<HomeModRow> HomeModRows() =>
            GameProfileRegistry.All().Where(g => g.IsSupported)
                .Select(g =>
                {
                    var state = HomeLogic.ModStateOf(CatalogPackOf(g)?.Version, InstalledPackOf(g)?.Version, IsModInstalled(g));
                    // an update is shown by the version it offers, anything else by the one on this PC
                    return new HomeModRow(g, state, state == ModState.UpdateAvailable ? CatalogPackOf(g)?.Version : InstalledPackOf(g)?.Version ?? CatalogPackOf(g)?.Version);
                })
                .Where(r => r.State != ModState.NotAvailable).ToList();

        /// <summary>The release to announce on Home: the update check's answer, or else a newer release the changelog shows.</summary>
        private (string? Tag, IReadOnlyList<string> Notes) HomeUpdate()
        {
            if (updateInfo is { } u) return (u.Tag, UpdateCheckNotes(u));
            var n = HomeLogic.NewerRelease(Cairn.Launcher.Components.Shared.Globals.CairnVersion, changelog);
            return n is null ? (null, System.Array.Empty<string>()) : (n.Tag, n.Items);
        }

        private static IReadOnlyList<string> UpdateCheckNotes(Cairn.Launcher.Components.Shared.UpdateInfo u) =>
            Changelog.Items((u.Notes ?? "").Replace("\n", "\n- ").Insert(0, "- "));

        private Task HomeUpdateClickAsync() => updateInfo is not null ? Task.Run(UpdateClick) : CheckUpdateNowAsync();

        // ---- the first-run tour ---------------------------------------------------------------------------------------------

        private bool showTour;

        private void FinishTour()
        {
            showTour = false;
            if (!listPrefs.TourSeen) { listPrefs.TourSeen = true; listPrefs.Save(); }
        }

        private void ReplayTour() { showTour = true; NavigateTo(AppView.Home); }

    }
}
