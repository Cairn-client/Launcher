// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// What the launcher writes to disk, per second: the disk figure of a download (the network figure is the download's own speed,
    /// kept by <see cref="DownloadManager"/>). Read from the process's I/O counters, so no performance counters and no admin rights.
    /// Each call to <see cref="SampleDiskWrite"/> reports the change since the previous one.
    /// </summary>
    public sealed class SystemMetrics
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct IoCounters { public ulong ReadOps, WriteOps, OtherOps, ReadBytes, WriteBytes, OtherBytes; }

        [DllImport("kernel32.dll")] private static extern bool GetProcessIoCounters(IntPtr process, out IoCounters counters);

        private readonly Process self = Process.GetCurrentProcess();
        private long lastTicks;
        private ulong lastWrite;

        /// <summary>Bytes per second written since the last call (0 on the first call or when it cannot be read).</summary>
        public double SampleDiskWrite()
        {
            long now = Stopwatch.GetTimestamp();
            double dt = lastTicks == 0 ? 0 : (now - lastTicks) / (double)Stopwatch.Frequency;
            lastTicks = now;
            try
            {
                if (!GetProcessIoCounters(self.Handle, out var io)) return 0;
                double rate = dt > 0 && io.WriteBytes >= lastWrite ? (io.WriteBytes - lastWrite) / dt : 0;
                lastWrite = io.WriteBytes;
                return rate;
            }
            catch { return 0; }
        }

        /// <summary>"12.3 MB/s", "840 KB/s", "0 B/s".</summary>
        public static string Rate(double bytesPerSecond) => Size(bytesPerSecond) + "/s";

        /// <summary>"1.2 GB", "46.0 MB", "980 KB", "12 B".</summary>
        public static string Size(double bytes) =>
            bytes >= 1L << 30 ? $"{bytes / (1L << 30):0.0} GB"
            : bytes >= 1L << 20 ? $"{bytes / (1L << 20):0.0} MB"
            : bytes >= 1L << 10 ? $"{bytes / (1L << 10):0} KB"
            : $"{bytes:0} B";
    }
}
