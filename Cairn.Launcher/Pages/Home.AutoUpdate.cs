// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// Mods kept up to date by themselves: when the launcher has read the catalog (at start, and every few hours while it is open) a mod that
    /// has a newer version is downloaded and put in the game, but never while a game runs or another download is going. Friends who play
    /// together need the same mod version, so this is on by default (Settings turns it off).
    /// </summary>
    public partial class Home
    {
        private static readonly TimeSpan AutoUpdatePeriod = TimeSpan.FromHours(4);
        private bool autoUpdating;

        private void StartAutoUpdateLoop()
        {
            _ = Task.Run(async () =>
            {
                while (true)
                {
                    await Task.Delay(AutoUpdatePeriod);
                    try { await InvokeAsync(RefreshCatalogAndPacksAsync); }
                    catch (Exception ex) { Log.Debug(ex, "updates: the periodic check failed"); }
                }
            });
        }

        private async Task AutoUpdateModsAsync()
        {
            if (!settings.AutoUpdateMods || autoUpdating) return;
            autoUpdating = true;
            try
            {
                foreach (var g in GameProfileRegistry.All().Where(g => g.ModRepo.Length > 0).ToList())
                {
                    if (gameRunning || Downloads.Active is not null) break;   // nothing is replaced under a running game, nor over another download
                    if (CatalogPackOf(g) is not { } offered || InstalledPackOf(g) is not { } have) continue;
                    if (HomeLogic.ModStateOf(offered.Version, have.Version, true) != ModState.UpdateAvailable) continue;
                    if (settings.HeldBackMods.TryGetValue(g.Id, out var held) && held == offered.Version) continue;   // the player went back from this one
                    if (!PackCatalog.LauncherIsEnough(offered.LauncherMin, Globals.CairnVersion)) continue;   // a launcher update comes first
                    string from = have.Version;
                    Log.Information("updates: {Game} {From} -> {To}, downloading in the background", g.Id, from, offered.Version);
                    await DownloadPackAsync(g.Id);
                    installedPackCache.Remove(g.Id);
                    if (InstalledPackOf(g) is { } now && now.Version == offered.Version)
                        ShowToast($"{g.DisplayName} mod updated to {offered.Version}.", error: false, action: PreviousPackOf(g) is null ? null : "Undo", onAction: () => RollbackPackAsync(g.Id), milliseconds: 20_000);
                }
            }
            catch (Exception ex) { Log.Warning(ex, "updates: a background update failed"); }
            finally { autoUpdating = false; }
        }
    }
}
