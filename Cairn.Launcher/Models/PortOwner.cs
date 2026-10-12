// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Diagnostics;

namespace Cairn.Launcher.Models
{
    /// <summary>The program that is listening on a TCP port.</summary>
    public sealed record PortOwner(int Pid, string Name);

    /// <summary>Who holds the host port, for the ready check's "close it" button.</summary>
    public static class PortOwnerFinder
    {
        /// <summary>The process id listening on <paramref name="port"/> in the output of <c>netstat -ano -p tcp</c>; null when nothing listens.</summary>
        public static int? ParseListeningPid(string netstat, int port)
        {
            foreach (var raw in (netstat ?? "").Split('\n'))
            {
                var p = raw.Trim().Split(' ', StringSplitOptions.RemoveEmptyEntries);
                if (p.Length < 5 || !p[0].Equals("TCP", StringComparison.OrdinalIgnoreCase)) continue;
                if (!p[1].EndsWith(":" + port, StringComparison.Ordinal)) continue;
                if (!p[3].Equals("LISTENING", StringComparison.OrdinalIgnoreCase)) continue;
                if (int.TryParse(p[4], out int pid)) return pid;
            }
            return null;
        }

        /// <summary>Windows itself and this launcher are never offered for closing.</summary>
        public static bool IsProtected(int pid) => pid <= 4 || pid == Environment.ProcessId;

        public static PortOwner? Find(int port)
        {
            try
            {
                var psi = new ProcessStartInfo("netstat.exe", "-ano -p tcp") { RedirectStandardOutput = true, UseShellExecute = false, CreateNoWindow = true };
                using var proc = Process.Start(psi);
                if (proc is null) return null;
                string output = proc.StandardOutput.ReadToEnd();
                proc.WaitForExit(5000);
                if (ParseListeningPid(output, port) is not int pid) return null;
                string name;
                try { name = Process.GetProcessById(pid).ProcessName; } catch (ArgumentException) { return null; }
                return new PortOwner(pid, name);
            }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException) { return null; }
        }
    }
}
