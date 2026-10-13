// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// External mods (<see cref="PackKinds.External"/>): a game's mod that lives in the game's own folder and brings its own way to
    /// start a session. Cairn downloads and updates it, places it in the game's folder, and starts the game (and the mod's companion
    /// program, when it has one); the session itself is the mod's. Host and Join (the Cairn session for Kingdom Come II) are not shown.
    /// </summary>
    public partial class Home
    {
        /// <summary>The game's mod is an external one: its downloaded pack says so, or, before any download, the catalog's entry for it.</summary>
        private bool IsExternalMod(GameProfile g) =>
            g.OwnSession ||
            (InstalledPackOf(g)?.Contract?.Kind ?? CatalogPackOf(g)?.Kind) == PackKinds.External;

        /// <summary>The game's own folder on this PC (where the exe is), when it was found.</summary>
        private string? GameFolderOf(GameProfile g)
        {
            if (!IsGameFound(g)) return null;
            return !string.IsNullOrWhiteSpace(g.InstallPath) && Directory.Exists(g.InstallPath) ? g.InstallPath : null;
        }

        private async Task InstallExternalModAsync(GameProfile game)
        {
            if (gameRunning && (string.IsNullOrEmpty(launchedGameId) || launchedGameId == game.Id)) { SetPackMessage("Close the game first.", error: true); return; }
            if (GameFolderOf(game) is not { } folder)
            {
                SetPackMessage($"{game.DisplayName} was not found on this PC. Make sure it is installed, then locate it.", error: true);
                return;
            }
            var r = await Task.Run(() => ExternalInstall.Place(folder, PacksRoot, game.Id));
            Log.Information("packs: {Game} placed in the game's folder: {Ok} {Message}", game.Id, r.Ok, r.Message);
            SetPackMessage(r.Ok ? "" : $"Could not install the mod: {r.Message}.", error: !r.Ok);
            await InvokeAsync(StateHasChanged);
        }

        /// <summary>Play: starts the mod's companion program (when it has one and it is not running), then the game; the mod takes it from there.</summary>
        private async Task PlayExternalAsync()
        {
            if (SelectedProfile is not { } game || !IsExternalMod(game)) return;
            if (!IsModInstalled(game)) { SetPackMessage("Install the mod before you play.", error: true); return; }
            if (gameRunning) { SetPackMessage($"{game.DisplayName} is already running.", error: false); return; }
            if (GameFolderOf(game) is not { } folder) { SetPackMessage($"{game.DisplayName} was not found on this PC. Make sure it is installed, then locate it.", error: true); return; }

            // a mod with a setup list of its own (Fallout 4: the script extender its launch program comes from) is checked first: what is missing is shown and done there
            if (PackSetupProgram(game.Id) is not null && !await EnsureSetupReadyAsync()) return;

            var companion = ExternalInstall.CompanionPath(PacksRoot, game.Id);
            bool companionWasOpen = companion is not null && IsRunning(companion);
            if (companion is not null) EnsureExternalBridge();   // the companion is the mod's bridge: it connects to this pipe
            if (companion is not null && !IsRunning(companion))
            {
                try
                {
                    var psi = new ProcessStartInfo(companion) { WorkingDirectory = Path.GetDirectoryName(companion)!, UseShellExecute = false };
                    psi.Environment[BridgeProtocol.PipeEnvironmentVariable] = externalBridge!.PipeName;
                    Process.Start(psi)?.Dispose();
                    Log.Information("games: {Game} companion started: {File}", game.Id, Path.GetFileName(companion));
                }
                catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException)
                {
                    Log.Warning(ex, "games: {Game} companion did not start", game.Id);
                    SetPackMessage($"The mod's own program did not start ({ex.Message}).", error: true);
                    return;
                }
            }

            // a mod whose own program starts the game (Noita's proxy does: it hosts or joins, then starts Noita) is only opened here
            if (companion is not null && PackStore.ReadContract(PacksRoot, game.Id) is { CompanionStartsGame: true })
            {
                SetPackMessage(companionWasOpen ? $"The mod's own window is already open: start {game.DisplayName} from it." : "", error: false);
                _ = ShowExternalMenuWhenUpAsync(game, folder);
                await InvokeAsync(StateHasChanged);
                return;
            }

            try
            {
                if (PackStore.ReadContract(PacksRoot, game.Id) is { Launch.Length: > 0 })
                {
                    // the mod starts the game with a program of its own in the game's folder (a mod loader's launcher)
                    if (ExternalInstall.LaunchPath(folder, PacksRoot, game.Id) is not { } launch) { SetPackMessage("The mod's launch program is not in the game's folder yet. Open Check setup (or press Play again) so the launcher can finish the setup.", error: true); return; }
                    Process.Start(new ProcessStartInfo(launch) { WorkingDirectory = Path.GetDirectoryName(launch)!, UseShellExecute = false })?.Dispose();
                }
                else if (game.SteamAppId != 0) UrlLauncher.Open($"steam://rungameid/{game.SteamAppId}");
                else
                {
                    var exe = Path.Combine(folder, game.DefaultExeName);
                    Process.Start(new ProcessStartInfo(exe) { WorkingDirectory = folder, UseShellExecute = false })?.Dispose();
                }
                Log.Information("games: {Game} started", game.Id);
                SetPackMessage("", error: false);
                if (companion is not null) _ = ShowExternalMenuWhenUpAsync(game, folder);   // the launcher's menu over the game, once the game's window is there
            }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException)
            {
                Log.Warning(ex, "games: {Game} did not start", game.Id);
                SetPackMessage($"{game.DisplayName} did not start ({ex.Message}).", error: true);
            }
            await InvokeAsync(StateHasChanged);
        }

        // The bridge the launcher listens on for external games whose mod has a companion; one per launcher run, given to the companion when it starts.
        private BridgeServer? externalBridge;

        // The game's saves, shown in its settings window (read from its bridge while it runs, from its program otherwise); and the one whose Delete was pressed once (a second press confirms).
        private IReadOnlyList<BridgeSaveSlot> externalSaves = Array.Empty<BridgeSaveSlot>();
        private string? deleteSaveAsked;
        private bool showAllSaves;
        private bool savesKnown;      // the list on screen is the game's own (read from its program or its bridge), not a guess
        private bool savesLoading;

        /// <summary>The bridge of this game is connected (the game runs and its mod has a bridge).</summary>
        private bool BridgeConnectedFor(GameProfile g) => externalBridge is { Connected: true } b && string.Equals(b.Game, g.Id, StringComparison.OrdinalIgnoreCase);

        private void EnsureExternalBridge()
        {
            if (externalBridge is not null) return;
            var bridge = externalBridge = new BridgeServer();
            bridge.ConnectionChanged += game =>
            {
                if (game.Length > 0) bridge.Send(BridgeProtocol.AskSaves());
                else { externalSaves = Array.Empty<BridgeSaveSlot>(); deleteSaveAsked = null; }
                _ = InvokeAsync(StateHasChanged);
            };
            bridge.Received += msg =>
            {
                if (msg is BridgeToast toast) { _ = InvokeAsync(() => ShowToast(toast.Text, toast.Error)); return; }
                if (msg is not BridgeSaves saves) return;
                externalSaves = saves.Slots;
                savesKnown = true;
                savesLoading = false;
                deleteSaveAsked = null;
                _ = InvokeAsync(StateHasChanged);
            };
        }

        /// <summary>The game's saves for its settings window: from its bridge while it runs, else from its pack's program (which answers with the game closed).</summary>
        private async Task LoadSavesAsync(GameProfile g)
        {
            savesKnown = false;
            savesLoading = true;
            deleteSaveAsked = null;
            showAllSaves = false;
            externalSaves = Array.Empty<BridgeSaveSlot>();
            await InvokeAsync(StateHasChanged);
            try
            {
                if (BridgeConnectedFor(g)) { externalBridge!.Send(BridgeProtocol.AskSaves()); return; }   // the bridge answers: Received sets the list
                if (ExternalInstall.OptionsProgramPath(PacksRoot, g.Id) is { } program && await SavesRunner.ListAsync(program) is { } slots)
                {
                    externalSaves = slots;
                    savesKnown = true;
                }
            }
            finally
            {
                if (!BridgeConnectedFor(g) || savesKnown) savesLoading = false;
                await InvokeAsync(StateHasChanged);
            }
        }

        /// <summary>Delete on a save: the first press asks, the second does it (through the bridge while the game runs, else through the pack's program).</summary>
        private async Task DeleteExternalSaveAsync(string id)
        {
            if (deleteSaveAsked != id) { deleteSaveAsked = id; return; }
            deleteSaveAsked = null;
            if (SelectedProfile is not { } g) return;
            if (BridgeConnectedFor(g))
            {
                if (BridgeProtocol.DeleteSave(id) is { } msg && externalBridge!.Send(msg)) Log.Information("games: asked the bridge to delete save {Id}", id);
                else ShowToast("The game's program did not answer. Is the game running?", true);   // the bridge answers with a toast and the new list
                return;
            }
            if (ExternalInstall.OptionsProgramPath(PacksRoot, g.Id) is not { } program) { ShowToast("The mod's program is not there.", true); return; }
            var (ok, message, slots) = await SavesRunner.DeleteAsync(program, id);
            Log.Information("games: {Game} save {Id} deleted: {Ok} {Message}", g.Id, id, ok, message);
            if (slots is not null) externalSaves = slots;
            ShowToast(message, !ok);
            await InvokeAsync(StateHasChanged);
        }

        private void ToggleShowAllSaves() => showAllSaves = !showAllSaves;
        /// <summary>Waits for the game's process (the game starts through Steam, so there is no process to hold), then draws the mod's menu over it from the bridge.</summary>
        private async Task ShowExternalMenuWhenUpAsync(GameProfile game, string folder)
        {
            if (externalBridge is not { } bridge) return;
            string name = Path.GetFileNameWithoutExtension(game.DefaultExeName);
            Process? proc = null;
            for (int i = 0; i < 240 && proc is null; i++)
            {
                await Task.Delay(1000);
                try { proc = Process.GetProcessesByName(name).FirstOrDefault(p => !p.HasExited); }
                catch (InvalidOperationException) { }
            }
            if (proc is null) { Log.Information("games: {Game} did not show up, so no menu", game.Id); return; }
            if (!bridge.Connected) { Log.Information("games: {Game} has no bridge connected, so no launcher menu", game.Id); return; }   // a companion that is not a bridge draws nothing
            try
            {
                inGameMenu?.Stop();
                string key = Cairn.Launcher.Services.InGameMenuDriver.ReadMenuKey(SettingsFileName);
                var driver = new Cairn.Launcher.Services.InGameMenuDriver(proc, key, bridge);
                inGameMenu = driver;
                _ = Task.Run(driver.RunAsync);
                Log.Information("games: {Game} menu started on its bridge", game.Id);
            }
            catch (Exception ex) { Log.Warning(ex, "games: the menu of {Game} could not start", game.Id); }
        }

        private static bool IsRunning(string exePath)
        {
            string name = Path.GetFileNameWithoutExtension(exePath);
            try
            {
                var found = Process.GetProcessesByName(name);
                bool any = found.Length > 0;
                foreach (var p in found) p.Dispose();
                return any;
            }
            catch (InvalidOperationException) { return false; }
        }

        /// <summary>The how-to lines of a game whose mod runs the session itself, when the catalog gives none of its own.</summary>
        private static readonly string[] ExternalSteps =
        {
            "Download the mod above; Cairn puts it in the game's folder.",
            "Press Play. Cairn starts the game, and the mod's own program if it has one.",
            "Host or join from the mod's own menu in the game.",
        };
    }
}
