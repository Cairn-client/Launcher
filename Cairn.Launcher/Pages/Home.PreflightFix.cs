// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Launcher.Models;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>The buttons of the ready check: a line that is not fine can fix itself, then the check runs again.</summary>
    public partial class Home
    {
        private async Task ApplyPreflightFixAsync(string fix)
        {
            if (SelectedProfile is not { } g) return;
            Log.Information("preflight: the player pressed {Fix} for {Game}", fix, g.Id);
            switch (fix)
            {
                case PreflightFix.StartSteam:
                    Cairn.Launcher.Components.Shared.UrlLauncher.Open("steam://open/main");
                    ShowToast("Starting Steam...");
                    await Task.Delay(5000);
                    break;
                case PreflightFix.InstallMod:
                case PreflightFix.UpdateMod:
                    await DownloadPackAsync(g.Id);   // a download puts the mod in the game too
                    if (!IsModInstalled(g)) await InstallModAsync(g.Id);
                    break;
                case PreflightFix.LocateGame:
                    LocateGame(g);
                    break;
                case PreflightFix.UpdateLauncher:
                    await HomeUpdateClickAsync();
                    return;
            }
            await OpenPreflightAsync();
        }
    }
}
