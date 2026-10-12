// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace Cairn.Launcher.Components.Shared
{
    /// <summary>
    /// The Modding Tools build of the game starts its own debug console (Tools\TraceServer\trace_server.run.exe) with every launch.
    /// Nothing in Cairn needs it, so while the game runs its window is hidden. The process is left alone (the game talks to it);
    /// only the window, and its taskbar button, go.
    /// </summary>
    public static class TraceServerWindow
    {
        private const int SW_HIDE = 0;
        private const string ProcessPrefix = "trace_server";

        private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

        [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc proc, IntPtr lParam);
        [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hWnd);
        [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
        [DllImport("user32.dll")] private static extern bool ShowWindow(IntPtr hWnd, int cmd);

        /// <summary>Hides the trace server's window whenever it shows, until <paramref name="game"/> exits. Never throws.</summary>
        public static void HideWhileRunning(Process game)
        {
            _ = Task.Run(async () =>
            {
                try
                {
                    int hidden = 0;
                    while (!game.HasExited)
                    {
                        hidden += HideNow();
                        await Task.Delay(hidden == 0 ? 250 : 2000);   // quick while waiting for it to appear, then a slow watch for a restart
                    }
                }
                catch (Exception ex) { Log.Debug(ex, "trace server window: watch ended"); }
            });
        }

        /// <summary>Hides every visible window of a trace_server process; returns how many were hidden.</summary>
        public static int HideNow()
        {
            int count = 0;
            EnumWindows((hWnd, _) =>
            {
                try
                {
                    if (!IsWindowVisible(hWnd)) return true;
                    GetWindowThreadProcessId(hWnd, out uint pid);
                    using var p = Process.GetProcessById((int)pid);
                    if (p.ProcessName.StartsWith(ProcessPrefix, StringComparison.OrdinalIgnoreCase) && ShowWindow(hWnd, SW_HIDE))
                        count++;
                }
                catch { /* the process went away between the enumeration and the lookup */ }
                return true;
            }, IntPtr.Zero);
            if (count > 0) Log.Information("trace server: its window was hidden");
            return count;
        }
    }
}
