// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Launcher.Models;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// Updates are done by hand: nothing is downloaded or installed by itself, and no window or message opens for one. While the launcher is open
    /// it only looks, every few hours, so that an orange dot shows next to Versions (and next to a mod's version) when something newer is out.
    /// </summary>
    public partial class Home
    {
        private static readonly TimeSpan UpdateLookPeriod = TimeSpan.FromHours(4);

        private void StartAutoUpdateLoop()
        {
            _ = Task.Run(async () =>
            {
                while (true)
                {
                    await Task.Delay(UpdateLookPeriod);
                    try
                    {
                        await InvokeAsync(RefreshCatalogAndPacksAsync);
                        await CheckForUpdateAsync();
                    }
                    catch (Exception ex) { Log.Debug(ex, "updates: the periodic look failed"); }
                }
            });
        }
    }
}
