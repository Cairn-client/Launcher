// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Threading.Tasks;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// Game mods are separate downloads: the installer carries only the launcher, and each game's mod is a pack published with
    /// the release (see Cairn.Setup/Packs.cs). The game page shows the pack's state; the sidebar can filter by it.
    /// </summary>
    public partial class Home
    {
        private static readonly HttpClient PackHttp = CreatePackHttp();
        private static HttpClient CreatePackHttp()
        {
            var http = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("Cairn-launcher");
            return http;
        }

        // The release's catalog of downloadable mods; null until it has been fetched (or when it cannot be).
        private PackIndex? packIndex;
        private bool packIndexLoading;
        private bool packIndexTried;

        // A download in flight: which game, how far, and the last outcome to tell the player.
        private string? packBusyId;
        private double packProgressValue;
        private string packMessage = "";
        private bool packMessageIsError;

        // Reading a pack's manifest per game per render is wasteful: asked once, until a download or removal changes it.
        private readonly Dictionary<string, InstalledPack?> installedPackCache = new(StringComparer.OrdinalIgnoreCase);

        private string PacksRoot => PackStore.DefaultRoot;

        private InstalledPack? InstalledPackOf(GameProfile g)
        {
            if (!installedPackCache.TryGetValue(g.Id, out var p))
                installedPackCache[g.Id] = p = PackStore.Read(PacksRoot, g.Id);
            return p;
        }

        /// <summary>The mod pack is on this PC (verified when it was downloaded).</summary>
        private bool IsModDownloaded(GameProfile g) => InstalledPackOf(g) is not null;

        /// <summary>
        /// The mod is really usable: downloaded, and (for a game with a mod folder) placed in it. Checked against the files in the
        /// game's Mods folder, so it is true for a development build too, which has no install manifest to consult.
        /// </summary>
        private bool IsModInstalled(GameProfile g)
        {
            if (!IsModDownloaded(g)) return false;
            if (g.Id != ModInstall.PackId) return true;
            return !string.IsNullOrWhiteSpace(settings.GamePath) && File.Exists(settings.GamePath)
                   && GameLocator.InstallRootOf(settings.GamePath) is { } mt && ModInstall.IsPlaced(mt, PacksRoot);
        }

        /// <summary>Puts a downloaded mod into the game's Mods folder (the setup checklist does the same on its own once everything else is ready).</summary>
        private async Task InstallModAsync(string gameId)
        {
            if (GameProfileRegistry.Get(gameId) is not { } game || gameId != ModInstall.PackId) return;
            if (gameRunning) { SetPackMessage("Close the game first.", error: true); return; }
            if (string.IsNullOrWhiteSpace(settings.GamePath) || GameLocator.InstallRootOf(settings.GamePath) is not { } mt)
            {
                SetPackMessage("Set the game's path in Settings first.", error: true);
                return;
            }
            var r = await Task.Run(() => ModInstall.Place(mt, PacksRoot));
            Log.Information("packs: {Game} placed: {Ok} {Message}", gameId, r.Ok, r.Message);
            SetPackMessage(r.Ok ? $"{game.DisplayName} mod installed." : $"Could not install the mod: {r.Message}.", error: !r.Ok);
            await InvokeAsync(StateHasChanged);
            await CheckSetupAgainAsync();
        }

        private PackInfo? CatalogPackOf(GameProfile g) => packIndex?.ForGame(g.Id);

        private async Task LoadPackIndexAsync()
        {
            if (packIndexLoading) return;
            packIndexLoading = true;
            try
            {
                packIndex = await PackCatalog.FetchAsync(PackHttp, Globals.CairnVersion);
                Log.Information("packs: catalog {Result}", packIndex is null ? "not available" : $"{packIndex.Packs.Count} pack(s)");
            }
            finally
            {
                packIndexLoading = false;
                packIndexTried = true;
                await InvokeAsync(StateHasChanged);
            }
        }

        private async Task DownloadPackAsync(string gameId)
        {
            if (packBusyId is not null) return;
            if (GameProfileRegistry.Get(gameId) is not { } game) return;
            if (!packIndexTried || packIndex is null) await LoadPackIndexAsync();
            var info = CatalogPackOf(game);
            var url = info is null ? null : PackCatalog.PackUrl(Globals.CairnVersion, info);
            if (info is null || url is null)
            {
                SetPackMessage("No mod is available for this game in this version of Cairn.", error: true);
                return;
            }

            packBusyId = gameId;
            packProgressValue = 0;
            SetPackMessage("", error: false);
            bool isSetupMod = gameId == ModInstall.PackId;
            if (isSetupMod) SetActivity(setupActivity with { DownloadingPack = true, PackProgress = 0, PackFailure = null });
            await InvokeAsync(StateHasChanged);

            var sw = Stopwatch.StartNew();
            long lastTick = 0;
            var progress = new Progress<double>(p =>
            {
                packProgressValue = p;
                if (sw.ElapsedMilliseconds - lastTick < 150 && p < 1) return;   // repaint a few times a second, not per chunk
                lastTick = sw.ElapsedMilliseconds;
                if (isSetupMod) SetActivity(setupActivity with { PackProgress = p });
                _ = InvokeAsync(StateHasChanged);
            });

            var result = await PackInstaller.DownloadAsync(info, url, PacksRoot, PackHttp, progress);
            Log.Information("packs: {Game} {Version} {Outcome}: {Message}", gameId, info.Version, result.Ok ? "downloaded" : "FAILED", result.Message);

            installedPackCache.Remove(gameId);
            packBusyId = null;
            if (isSetupMod)
                SetActivity(setupActivity with { DownloadingPack = false, PackProgress = null, PackFailure = result.Ok ? null : $"The mod could not be downloaded ({result.Message})." });
            SetPackMessage(result.Ok ? $"{game.DisplayName} mod downloaded." : $"Could not download the mod: {result.Message}.", error: !result.Ok);
            await InvokeAsync(StateHasChanged);
            if (result.Ok && isSetupMod) await InstallModAsync(gameId);   // downloading installs it too: placed in the game right away (the Install button stays as the fallback if that fails)
        }

        private async Task RemovePackAsync(string gameId)
        {
            if (packBusyId is not null || GameProfileRegistry.Get(gameId) is not { } game) return;
            if (gameRunning)
            {
                SetPackMessage("Close the game first.", error: true);
                return;
            }
            string? note = null;
            if (gameId == ModInstall.PackId && !string.IsNullOrWhiteSpace(settings.GamePath) && File.Exists(settings.GamePath)
                && GameLocator.InstallRootOf(settings.GamePath) is { } mt)
            {
                var r = await Task.Run(() => ModInstall.Unplace(mt, PacksRoot));
                if (!r.Ok) { SetPackMessage($"Could not remove the mod from the game ({r.Message}).", error: true); return; }
                note = r.Message;
            }
            await Task.Run(() => PackStore.Remove(PacksRoot, gameId));
            installedPackCache.Remove(gameId);
            Log.Information("packs: {Game} removed ({Note})", gameId, note ?? "download only");
            SetPackMessage($"{game.DisplayName} mod removed.", error: false);
            if (gameId == ModInstall.PackId) await CheckSetupAgainAsync();
        }

        private void SetPackMessage(string message, bool error)
        {
            packMessage = message;
            packMessageIsError = error;
            ShowToast(message, error);
        }
    }
}
