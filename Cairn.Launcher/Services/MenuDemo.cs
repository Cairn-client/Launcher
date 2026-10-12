// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Threading;
using System.Windows;
using Cairn.Launcher.Models;

namespace Cairn.Launcher.Services
{
    /// <summary>
    /// <c>Cairn.Launcher.exe --menu-demo</c>: the in-game menu over the main screen for a few seconds, with made-up items and the
    /// selection moving, to look at the overlay without a game. Nothing else runs.
    /// </summary>
    internal static class MenuDemo
    {
        /// <summary><c>--menu-probe &lt;game exe&gt;</c>: the driver against this very process, with a log in the temp folder, to see what it does.</summary>
        public static void Probe(string gameExe)
        {
            string logFile = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "cairn-menu-probe.log");
            Serilog.Log.Logger = new Serilog.LoggerConfiguration().MinimumLevel.Debug().WriteTo.File(logFile).CreateLogger();
            string root = Pages.Home.GameRootOf(gameExe);
            var driver = new InGameMenuDriver(System.Diagnostics.Process.GetCurrentProcess(), DateTime.Now.AddHours(-1), root, "insert");
            var task = System.Threading.Tasks.Task.Run(driver.RunAsync);
            Thread.Sleep(8000);
            driver.Stop();
            task.Wait(3000);
            Serilog.Log.CloseAndFlush();
        }

        public static void Run()
        {
            const string line = "[CAIRN-EVT] v1 1 menu_state Kingdom Come: Together~Mod menu~~|"
                + "name_badges~Display~onoff~1~0~On~Partner's name badge~Shows your partner's name (and health) above their figure."
                + "|ping_line~Display~onoff~1~0~On~Ping and clock line~Shows the ping and clock line in the top left corner."
                + "|clean_screen~Display~onoff~0~0~Off~Clean screen~Hides everything the mod draws, for screenshots and recording."
                + "|friendly_fire~Gameplay~onoff~1~1~On~Friendly fire~Players can hurt each other."
                + "|crime_mode~Gameplay~choice~-~0~Shared~Crime~Shared: a crime by either of you counts for both."
                + "|fast_travel~Gameplay~onoff~0~0~Off~Fast travel~Allows fast travel in a co-op session."
                + "|bring_partner~Gameplay~action~-~0~~Bring my partner to me~Host only: teleports your partner right beside you."
                + "|menu_key~Keys~key~-~0~Insert~Menu key~The key that opens this menu."
                + "|unstuck~Help~action~-~0~~I'm stuck~Frees you from a stuck state: a bed, a picker that did not close, a knockdown."
                + "|version~Help~info~-~0~0.3.3~Version~";
            var state = InGameMenuProtocol.Parse(line)!;
            var nav = new MenuNav();
            nav.Update(state);
            using var overlay = new OverlayWindow();
            var screen = System.Windows.Forms.Screen.PrimaryScreen!.Bounds;
            var rect = new Int32Rect(screen.X, screen.Y, screen.Width, screen.Height);
            var end = DateTime.UtcNow.AddSeconds(25);
            int tick = 0;
            while (DateTime.UtcNow < end)
            {
                overlay.Render(true, nav.State, nav.SelectedIndex, "", rect);
                Thread.Sleep(900);
                if (tick == 2) overlay.Snapshot(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "cairn-menu-demo.png"));
                if (++tick % 2 == 0) nav.Step(1);
            }
            overlay.Render(false, MenuState.Empty, -1, "", rect);
            Thread.Sleep(300);
        }
    }
}
