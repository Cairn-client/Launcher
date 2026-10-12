// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Cairn.Launcher.Models;
using Cairn.Launcher.Services;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// DLSS 5 in a game, as a setting of its own page: ticked and saved, the launcher puts it in the game's folder (DLSS 5 Autopilot does the work, see
    /// Services/Dlss5Service.cs); unticked and saved, it takes it out. While it is in, the game's menu has a row that opens its overlay.
    /// </summary>
    public partial class Home
    {
        private GpuClass? dlss5GpuCache;
        private string dlss5GpuName = "";
        private bool dlss5Busy;

        /// <summary>The best RTX card of this PC (read once).</summary>
        private GpuClass Dlss5Gpu
        {
            get
            {
                if (dlss5GpuCache is { } c) return c;
                var names = Dlss5Service.GpuNames();
                dlss5GpuCache = Dlss5.Best(names);
                dlss5GpuName = names.FirstOrDefault(n => Dlss5.ClassifyGpu(n) == dlss5GpuCache) ?? names.FirstOrDefault() ?? "";
                return dlss5GpuCache.Value;
            }
        }

        private string Dlss5GpuText => Dlss5Gpu == GpuClass.None
            ? Dlss5.Describe(GpuClass.None) + (dlss5GpuName.Length > 0 ? $" This PC has: {dlss5GpuName}." : "")
            : $"{dlss5GpuName}. {Dlss5.Describe(Dlss5Gpu)}";

        /// <summary>DLSS 5 is in this game's folder, put there by this launcher: the game's menu gets its row.</summary>
        private bool Dlss5Active => settings.Dlss5Enabled && Dlss5Service.IsInstalled(GameProfileRegistry.Kcd2Id);

        /// <summary>After the game's settings were saved: make the game's folder match the tick.</summary>
        private async Task ApplyDlss5Async()
        {
            if (dlss5Busy) return;
            string id = GameProfileRegistry.Kcd2Id;
            bool have = Dlss5Service.IsInstalled(id);
            bool want = settings.Dlss5Enabled && Dlss5.Supported(Dlss5Gpu);
            if (want == have) { if (settings.Dlss5Enabled != want) { settings.Dlss5Enabled = want; WriteSettings("DLSS 5 needs an RTX card"); } return; }

            // the game's ROOT folder, not the exe's: the tool looks for the game's DLSS file under the folder it is given, and the Modding Tools keep it
            // in BinWin64Shared, beside the exe's own folder (given the exe's folder it sees no DLSS and refuses the OptiScaler route)
            string? dir = string.IsNullOrWhiteSpace(settings.GamePath) ? null : Cairn.Setup.GameLocator.InstallRootOf(settings.GamePath) ?? Path.GetDirectoryName(settings.GamePath);
            string back = have ? "on" : "off";
            if (gameRunning || dir is null || !Directory.Exists(dir))
            {
                settings.Dlss5Enabled = have;   // nothing was done: the tick goes back to what the folder holds
                WriteSettings("DLSS 5 could not be changed");
                UiService.ShowError(gameRunning ? "Close the game first: DLSS 5 is put in or taken out while it is not running." : "Set the game's path in the game's settings first.");
                await InvokeAsync(StateHasChanged);
                return;
            }

            dlss5Busy = true;
            await InvokeAsync(StateHasChanged);
            ShowToast(want ? "Putting DLSS 5 into the game..." : "Taking DLSS 5 out of the game...");
            string last = "";
            Dlss5Run run;
            try
            {
                void Say(string s) { last = s; Log.Information("dlss5: {Line}", s); }
                run = want
                    ? await Dlss5Service.InstallAsync(id, dir, Say, CancellationToken.None)
                    : await Dlss5Service.RemoveAsync(id, dir, Say, CancellationToken.None);

                if (run.Ok)
                {
                    ShowToast(want ? "DLSS 5 is in the game. Open its overlay from the game's menu." : "DLSS 5 is out of the game.");
                    if (inGameMenu is { } m) m.Dlss5Active = Dlss5Active;
                }
                else
                {
                    settings.Dlss5Enabled = have;
                    WriteSettings($"DLSS 5 stayed {back}");
                    string tail = string.Join("\n", run.Output.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).TakeLast(6));
                    UiService.ShowError((want ? "DLSS 5 could not be put into the game." : "DLSS 5 could not be taken out.") + (tail.Length > 0 ? "\n\n" + tail : last.Length > 0 ? "\n\n" + last : ""));
                }
            }
            finally
            {
                dlss5Busy = false;
                await InvokeAsync(StateHasChanged);
            }
        }
    }
}
