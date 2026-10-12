// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
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
                SetPackMessage($"{game.DisplayName} was not found on this PC. Locate it first.", error: true);
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
            if (GameFolderOf(game) is not { } folder) { SetPackMessage($"{game.DisplayName} was not found on this PC.", error: true); return; }

            if (ExternalInstall.CompanionPath(PacksRoot, game.Id) is { } companion && !IsRunning(companion))
            {
                try
                {
                    Process.Start(new ProcessStartInfo(companion) { WorkingDirectory = Path.GetDirectoryName(companion)!, UseShellExecute = false })?.Dispose();
                    Log.Information("games: {Game} companion started: {File}", game.Id, Path.GetFileName(companion));
                }
                catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException)
                {
                    Log.Warning(ex, "games: {Game} companion did not start", game.Id);
                    SetPackMessage($"The mod's own program did not start ({ex.Message}).", error: true);
                    return;
                }
            }

            try
            {
                if (game.SteamAppId != 0) UrlLauncher.Open($"steam://rungameid/{game.SteamAppId}");
                else
                {
                    var exe = Path.Combine(folder, game.DefaultExeName);
                    Process.Start(new ProcessStartInfo(exe) { WorkingDirectory = folder, UseShellExecute = false })?.Dispose();
                }
                Log.Information("games: {Game} started", game.Id);
                SetPackMessage("", error: false);
            }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException)
            {
                Log.Warning(ex, "games: {Game} did not start", game.Id);
                SetPackMessage($"{game.DisplayName} did not start ({ex.Message}).", error: true);
            }
            await InvokeAsync(StateHasChanged);
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
