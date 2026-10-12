// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading.Tasks;
using Microsoft.JSInterop;
using Cairn.Launcher.Components.Views;
using Cairn.Launcher.Models;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>The small conveniences around the pages: the right-click menu of a game, toasts, the quick search (Ctrl+K), mod updates from Home.</summary>
    public partial class Home
    {
        // ---- the right-click menu of a game in the Library list ----------------------------------------------------------------

        private string? menuGameId;
        private double menuX, menuY;

        private void OpenGameMenu(string gameId, Microsoft.AspNetCore.Components.Web.MouseEventArgs e)
        {
            menuGameId = gameId;
            // keep the menu inside the window (it is about 230 x 250)
            menuX = Math.Max(4, e.ClientX);
            menuY = Math.Max(4, e.ClientY);
        }

        private void CloseGameMenu() => menuGameId = null;

        private void MenuAction(Action action) { menuGameId = null; action(); }

        private async Task MenuAction(Func<Task> action) { menuGameId = null; await action(); }

        private void OpenInstallFolder(GameProfile g)
        {
            try
            {
                if (!string.IsNullOrWhiteSpace(g.InstallPath) && System.IO.Directory.Exists(g.InstallPath))
                    Process.Start(new ProcessStartInfo("explorer.exe", $"\"{g.InstallPath}\"") { UseShellExecute = true });
            }
            catch (Exception ex) { Log.Debug(ex, "could not open the install folder of {Game}", g.Id); }
        }

        // ---- toasts -----------------------------------------------------------------------------------------------------------

        private sealed record Toast(int Id, string Text, bool Error, string? Action = null, Func<Task>? OnAction = null);

        private readonly List<Toast> toasts = new();

        /// <summary>The toast's button: its action runs and the toast goes.</summary>
        private async Task ToastActionAsync(Toast t)
        {
            toasts.Remove(t);
            if (t.OnAction is not null) await t.OnAction();
        }
        private int toastSeq;

        /// <summary>A notice in the corner that goes away by itself.</summary>
        private void ShowToast(string text, bool error = false, string? action = null, Func<Task>? onAction = null, int milliseconds = 0)
        {
            if (string.IsNullOrWhiteSpace(text)) return;
            var t = new Toast(++toastSeq, text, error, action, onAction);
            toasts.Add(t);
            while (toasts.Count > 4) toasts.RemoveAt(0);
            _ = Task.Run(async () =>
            {
                await Task.Delay(milliseconds > 0 ? milliseconds : error ? 6000 : 3500);
                await InvokeAsync(() => { toasts.Remove(t); StateHasChanged(); });
            });
            _ = InvokeAsync(StateHasChanged);
        }

        // ---- favourites, play history ------------------------------------------------------------------------------------------

        private void ToggleFavoriteWithToast(GameProfile g)
        {
            bool was = IsFavorite(g.Id);
            ToggleFavorite(g.Id);
            ShowToast(was ? $"{g.DisplayName} removed from your favourites." : $"{g.DisplayName} added to your favourites.");
        }

        private RecentSession? PlayedOf(string gameId) =>
            recent.Entries.FirstOrDefault(e => string.Equals(e.GameId, gameId, StringComparison.OrdinalIgnoreCase));

        // ---- search (the box in the top bar; Ctrl+K puts the cursor in it) ----------------------------------------------------

        private SearchBar? searchBar;

        [JSInvokable]
        public void ToggleQuickSearchJS()
        {
            if (searchBar is not null) _ = InvokeAsync(searchBar.FocusAsync);
        }

        private void PickFromSearch(string gameId)
        {
            OpenGameFromHome(gameId);
            StateHasChanged();
        }
        // ---- session history: play again ---------------------------------------------------------------------------------------------

        /// <summary>The two sessions before the latest one, newest first (each game once).</summary>
        private IReadOnlyList<(GameProfile Game, RecentSession Session)> HomeHistory() =>
            recent.Entries.OrderByDescending(e => e.LastUtc).Skip(1).Take(2)
                .Select(e => (Game: GameProfileRegistry.Get(e.GameId), Session: e))
                .Where(x => x.Game is not null).Select(x => (x.Game!, x.Session)).ToList();

        // ---- the shared in-game menu --------------------------------------------------------------------------------------------

        private Cairn.Launcher.Services.InGameMenuDriver? inGameMenu;

        /// <summary>The game is up: its menu is drawn by the launcher over the game (the same for every game), once the mod says it has one.</summary>
        private void StartInGameMenu(Process game, DateTime gameStartLocal)
        {
            Cairn.Launcher.Services.InGameMenuDriver.Trace("StartInGameMenu called");
            try
            {
                inGameMenu?.Stop();
                string root = LogBundleGameRoot;
                string key = Cairn.Launcher.Services.InGameMenuDriver.ReadMenuKey(SettingsFileName);
                var driver = new Cairn.Launcher.Services.InGameMenuDriver(game, gameStartLocal, root, key, dlss5: Dlss5Active);
                inGameMenu = driver;
                _ = Task.Run(driver.RunAsync);
            }
            catch (Exception ex) { Log.Warning(ex, "in-game menu: could not start"); }
        }

        // ---- the ready check -------------------------------------------------------------------------------------------------------------

        private bool showPreflight;
        private PortOwner? portOwner;        // who holds the host port, when it is taken
        private IReadOnlyList<PreflightItem> preflightItems = Array.Empty<PreflightItem>();

        private static bool PortIsFree(int port)
        {
            try
            {
                var l = new System.Net.Sockets.TcpListener(System.Net.IPAddress.Any, port);
                l.Start(); l.Stop();
                return true;
            }
            catch (System.Net.Sockets.SocketException) { return false; }
        }

        /// <summary>Closes the program that holds the host port (asked twice in the window), then checks again.</summary>
        private async Task ClosePortOwnerAsync()
        {
            if (portOwner is not { } o) return;
            if (PortOwnerFinder.IsProtected(o.Pid)) { ShowToast("That program cannot be closed from here.", error: true); return; }
            try
            {
                using var p = Process.GetProcessById(o.Pid);
                p.Kill();
                await Task.Run(() => p.WaitForExit(3000));
                Log.Information("preflight: closed {Name} ({Pid}) that held port {Port}", o.Name, o.Pid, settings.HostPort);
                ShowToast($"{o.Name} was closed.");
            }
            catch (Exception ex) when (ex is ArgumentException or InvalidOperationException or System.ComponentModel.Win32Exception)
            {
                Log.Information(ex, "preflight: could not close {Name} ({Pid})", o.Name, o.Pid);
                ShowToast($"{o.Name} could not be closed (it may need administrator rights).", error: true);
            }
            await OpenPreflightAsync();
        }

        private async Task OpenPreflightAsync()
        {
            await UpdatePreflightStatusAsync(showModal: true);
        }

        private CheckStatus readyCheckStatus = CheckStatus.Ok;

        private async Task UpdatePreflightStatusAsync(bool showModal = false)
        {
            if (SelectedProfile is not { } g) return;
            int port = settings.HostPort;
            bool steam = false, portFree = true;
            await Task.Run(() =>
            {
                steam = Process.GetProcessesByName("steam").Length > 0;
                portFree = (gameRunning && launchedAs == "host") || PortIsFree(port);   // our own relay holds it while we host
            });
            portOwner = portFree ? null : await Task.Run(() => PortOwnerFinder.Find(port));
            var upd = HomeUpdate();
            preflightItems = Preflight.Build(new PreflightInput(
                g.IsSupported, IsGameFound(g), IsModInstalled(g), SetupAllGood, steam, portFree, gameRunning, upd.Tag, BlockedReasonOfInstalled(g)));
            
            if (preflightItems.Any(x => x.Status == CheckStatus.Fail))
                readyCheckStatus = CheckStatus.Fail;
            else if (preflightItems.Any(x => x.Status == CheckStatus.Warn))
                readyCheckStatus = CheckStatus.Warn;
            else
                readyCheckStatus = CheckStatus.Ok;

            if (showModal) showPreflight = true;
            await InvokeAsync(StateHasChanged);
        }

        private async Task<bool> PreflightHasIssuesAsync(string mode)
        {
            await UpdatePreflightStatusAsync(showModal: false);
            // If anything is red (Fail), or if port is blocked when hosting, or steam isn't running
            bool issue = preflightItems.Any(x => x.Status == CheckStatus.Fail)
                || (mode == "host" && preflightItems.Any(x => x.Title == "Host port" && x.Status != CheckStatus.Ok))
                || preflightItems.Any(x => x.Title == "Steam" && x.Status != CheckStatus.Ok);
            return issue;
        }

        // ---- what Steam says about a game: reviews, Metacritic, release date, genres ----------------------------------------------------

        private readonly Dictionary<uint, GameInfo> gameInfos = new();
        private readonly HashSet<uint> gameInfoLoading = new();

        private GameInfo? InfoOf(GameProfile g) => gameInfos.TryGetValue(g.PageAppId, out var i) ? i : null;

        private async Task LoadGameInfoAsync(GameProfile g)
        {
            uint id = g.PageAppId;
            if (id == 0 || gameInfos.ContainsKey(id) || !gameInfoLoading.Add(id)) return;
            try
            {
                gameInfos[id] = await GameInfoSource.GetAsync(PackHttp, id);
            }
            catch (Exception ex) { Log.Debug(ex, "game info of {Game} could not be read", g.Id); gameInfos[id] = GameInfo.Empty; }
            finally { gameInfoLoading.Remove(id); }
            await InvokeAsync(StateHasChanged);
        }

        private string ReviewText(GameInfo? i) =>
            i is null ? "Loading..." : i.ReviewText is null ? "-" : $"{i.ReviewText}" + (i.PositivePercent is int p ? $" ({p}% of {i.ReviewCount:N0})" : "");

        private static string ReviewClass(GameInfo? i) => i?.PositivePercent switch { >= 70 => "score good", >= 40 => "score mixed", < 40 => "score bad", _ => "" };

        private static string ScoreClass(int s) => s >= 75 ? "good" : s >= 50 ? "mixed" : "bad";

        // ---- mod updates from Home ------------------------------------------------------------------------------------------------

        private IReadOnlyList<HomeModRow> ModUpdates() => HomeModRows().Where(r => r.State == ModState.UpdateAvailable).ToList();

        /// <summary>Queues every mod that has a newer version (a download also installs it); the queue runs them one after the other.</summary>
        private Task UpdateAllModsAsync()
        {
            foreach (var row in ModUpdates()) _ = DownloadPackAsync(row.Game.Id);
            return Task.CompletedTask;
        }

        // the footer's buttons: fire and forget, the queue shows what happens
        private void FooterUpdate(string gameId) => _ = DownloadPackAsync(gameId);
        private Task FooterCheckAsync() => RefreshCatalogAndPacksAsync();
    }
}
