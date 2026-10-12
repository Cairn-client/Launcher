// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.IO;
using System.Diagnostics;
using System.Linq;
using System.Threading.Tasks;
using Cairn.Launcher.Models;
using Cairn.Setup;

namespace Cairn.Launcher.Pages
{
    public partial class Home
    {
        // The game whose page is shown on the right of the window. Host and Join act on the game profile selected here.
        private string selectedGameId = GameProfileRegistry.Kcd2Id;

        // the feature card on a game's page: the category tab shown (empty = the first)
        private string featureTab = string.Empty;

        private static string StatusClass(FeatureStatus s) => s switch { FeatureStatus.Ready => "ready", FeatureStatus.InTest => "test", _ => "planned" };

        // The search box above the game list.
        private string gameFilter = "";

        private GameProfile? SelectedProfile => GameProfileRegistry.Get(selectedGameId);

        /// <summary>The setup check has run and every step is done, the optional ones too: what "Ready to play" means.</summary>
        /// <summary>The part of setup that concerns every game: Steam is running and signed in.</summary>
        private bool SystemSetupGood => setupSnapshot is not null && setupSteps.Where(s => s.Id == StepId.Steam).All(s => s.Status == StepStatus.Done);

        private bool SetupAllGood => setupSnapshot is not null && setupSteps.Where(s => s.Id != StepId.Ready).All(s => s.Status == StepStatus.Done);

        // ---- Host / Join, and stopping them ---------------------------------------------------------------------------------
        // While the game this launcher started is running, the Host button reads "Stop hosting" and the Join button "Leave game";
        // either one closes the game, the Modding Tools' own helper programs, and what Cairn started for the session.
        // The selected game's own settings window (the button beside Host and Join); the menu bar's Settings is showSettings.
        private bool showGameSettings;

        private void OpenGameSettings()
        {
            var taken = settingsStore.Refresh(settings);
            if (taken.Count > 0) Log.Information("settings: changed outside the launcher since it read them: {Keys}", string.Join(", ", taken));
            showGameSettings = true;
            if (SelectedProfile is { } og) _ = LoadOptionStatusesAsync(og);
        }

        private void SaveGameSettings()
        {
            WriteSettings("the game settings window");
            showGameSettings = false;
            if (SelectedProfile is { } sg && sg.Options.Count > 0) _ = ApplyGameOptionsAsync(sg);
            else _ = ApplyDlss5Async();   // DLSS 5 in or out of the game's folder, to match the tick (a game whose pack offers it as an option: its program does it)
        }

        /// <summary>Closed without SAVE: the unsaved edits go back, as for the general Settings.</summary>
        private void CancelGameSettings()
        {
            settingsStore.Revert(settings);
            showGameSettings = false;
        }

        private bool showStopConfirm;
        private string stopWhat = "";   // "host" or "join"

        private async Task HostClickAsync()
        {
            if (NeedsModGuard()) return;
            if (gameRunning && launchedAs == "host") { AskStop("host"); return; }
            if (await PreflightHasIssuesAsync("host"))
            {
                showPreflight = true;
                return;
            }
            await OpenHostOrCode();
        }

        private async Task JoinClickAsync()
        {
            if (NeedsModGuard()) return;
            if (gameRunning && launchedAs == "join") { AskStop("join"); return; }
            if (await PreflightHasIssuesAsync("join"))
            {
                showPreflight = true;
                return;
            }
            await OpenJoinSteam();
        }

        /// <summary>A supported game cannot be hosted or joined until its mod pack is downloaded (the buttons are off too; this is the second lock).</summary>
        private bool NeedsMod(GameProfile game) => game.IsSupported && !IsModInstalled(game);

        private bool NeedsModGuard()
        {
            if (SelectedProfile is not { } g || !NeedsMod(g) || gameRunning) return false;
            SetPackMessage("Install the mod before you host or join.", error: true);
            return true;
        }

        private void AskStop(string what) { stopWhat = what; showStopConfirm = true; }

        private void CancelStop() => showStopConfirm = false;

        private async Task ConfirmStopAsync()
        {
            showStopConfirm = false;
            Log.Information("session: the player stopped the {What} session", stopWhat);
            StopHostedRelay();      // the relay of a hosted session
            StopExistingAgent();    // this player's agent
            await Task.Run(CloseGameAndTools);
            gameRunning = false;
            launchedAs = "";
            launchedGameId = "";
            ResetLaunchState();     // the launch window, the connect gate, the game's menu hook
            await InvokeAsync(StateHasChanged);
        }

        /// <summary>
        /// The game has closed (the player quit it, or it crashed): the Modding Tools' helper programs it started are closed too. Left running,
        /// the trace server keeps Steam showing the Modding Tools app as running long after the game is gone.
        /// </summary>
        private static void CloseModdingTools() => CloseProcesses(ModdingToolsProcesses);

        private static readonly string[] ModdingToolsProcesses = ["trace_server", "trace_server.run", "trace_server_launcher"];

        /// <summary>Closes the game executable and its helper programs, and waits a moment for them to exit.</summary>
        private void CloseGameAndTools()
        {
            var names = new List<string>(ModdingToolsProcesses);
            string? gameExe = null;
            if (pendingGameProcess is { HasExited: false } pg)
            {
                try { gameExe = pg.ProcessName; } catch { }
            }
            if (string.IsNullOrEmpty(gameExe))
            {
                var profile = GameProfileRegistry.Get(launchedGameId) ?? SelectedProfile;
                if (profile != null && !string.IsNullOrWhiteSpace(profile.DefaultExeName))
                    gameExe = Path.GetFileNameWithoutExtension(profile.DefaultExeName);
            }
            if (!string.IsNullOrEmpty(gameExe) && !names.Contains(gameExe, StringComparer.OrdinalIgnoreCase))
                names.Insert(0, gameExe);
            else if (!names.Contains("KingdomCome", StringComparer.OrdinalIgnoreCase))
                names.Insert(0, "KingdomCome");

            CloseProcesses(names.ToArray());
        }

        private static void CloseProcesses(string[] names)
        {
            foreach (string name in names)
            {
                Process[] procs;
                try { procs = Process.GetProcessesByName(name); }
                catch (Exception ex) { Log.Warning(ex, "session: could not list {Name}", name); continue; }
                foreach (var p in procs)
                {
                    try
                    {
                        if (!p.HasExited) { p.Kill(entireProcessTree: true); p.WaitForExit(5000); }
                    }
                    catch (Exception ex) { Log.Warning(ex, "session: {Name} (pid {Pid}) did not close", name, p.Id); }
                    finally { p.Dispose(); }
                }
            }
        }

        /// <summary>Lets the player point Cairn at a game it did not find (not on Steam, or in another library).</summary>
        private void LocateGame(GameProfile profile)
        {
            string? picked = null;
            var t = new System.Threading.Thread(() =>
            {
                using var d = new System.Windows.Forms.OpenFileDialog
                {
                    Title = $"Locate {profile.DisplayName}",
                    Filter = $"{profile.DisplayName} ({profile.DefaultExeName})|{profile.DefaultExeName}|Programs (*.exe)|*.exe",
                };
                if (d.ShowDialog() == System.Windows.Forms.DialogResult.OK) picked = d.FileName;
            });
            t.SetApartmentState(System.Threading.ApartmentState.STA);
            t.Start();
            t.Join();
            if (picked is null) return;

            if (GamePathStore.ResolveInstallFolder(picked, profile.DefaultExeName) is not { } folder)
            {
                SetPackMessage($"That is not {profile.DefaultExeName}. Pick the game's own executable.", error: true);
                return;
            }
            if (!gamePaths.Set(profile.Id, folder)) SetPackMessage("Could not remember that folder (the Cairn data folder is not writable).", error: true);
            steamFoundCache.Remove(profile.Id);
            GameProfileRegistry.MarkInstalled(profile.Id, folder);
            Log.Information("games: {Game} located at {Folder}", profile.Id, folder);
        }

        /// <summary>Opens the game in the Steam client's library (the Steam app handles the steam:// link; nothing happens without Steam).</summary>
        private static void OpenSteamLibrary(GameProfile g) =>
            Cairn.Launcher.Components.Shared.UrlLauncher.Open($"steam://nav/games/details/{g.PageAppId}");

        private static void OpenSteamLibrary(uint appId) =>
            Cairn.Launcher.Components.Shared.UrlLauncher.Open($"steam://nav/games/details/{appId}");

        private static void OpenSteamStore(uint appId) =>
            Cairn.Launcher.Components.Shared.UrlLauncher.Open($"https://store.steampowered.com/app/{appId}");

        private static void OpenSteamStore(GameProfile g) =>
            Cairn.Launcher.Components.Shared.UrlLauncher.Open($"https://store.steampowered.com/app/{g.PageAppId}");

        private void SelectGame(string id)
        {
            if (GameProfileRegistry.Get(id) is not null) selectedGameId = id;
            if (GameProfileRegistry.Get(id) is { } g) _ = LoadGameInfoAsync(g);
        }

        // Steam is asked once per other game, not at every render.
        private readonly Dictionary<string, bool> steamFoundCache = new();

        // Folders the player pointed at for games outside Steam (a GOG or Epic copy, a manual install).
        private readonly GamePathStore gamePaths = new();

        /// <summary>Whether the profile's game is on this PC. KCD2's is the game path in Settings; the others are looked up in Steam.</summary>
        private bool IsGameFound(GameProfile profile)
        {
            if (profile.Id == GameProfileRegistry.Kcd2Id)
            {
                bool found = !string.IsNullOrWhiteSpace(settings.GamePath) && File.Exists(settings.GamePath);
                GameProfileRegistry.MarkInstalled(profile.Id, found ? settings.GamePath : null);
                return found;
            }
            if (steamFoundCache.TryGetValue(profile.Id, out bool cached)) return cached;
            if (gamePaths.Get(profile.Id) is { } custom && GamePathStore.ResolveInstallFolder(custom, profile.DefaultExeName) is { } folder)
            {
                GameProfileRegistry.MarkInstalled(profile.Id, folder);
                return steamFoundCache[profile.Id] = true;
            }
            bool installed = false;
            try
            {
                var steam = SteamInstall.Locate(RealSetupHost.Instance, null);
                if (steam.Manifest(profile.SteamAppId) is { FullyInstalled: true } m && Directory.Exists(m.InstallPath))
                {
                    GameProfileRegistry.MarkInstalled(profile.Id, m.InstallPath);
                    installed = true;
                }
            }
            catch { /* no Steam, or an unreadable library: not found */ }
            return steamFoundCache[profile.Id] = installed;
        }
    }
}
