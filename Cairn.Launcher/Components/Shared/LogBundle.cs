// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.IO;
using System.IO.Compression;
using System.Linq;

namespace Cairn.Launcher.Components.Shared
{
    /// <summary>
    /// "Save the logs" and "Report a bug": the launcher's own logs and settings, and whatever the game's pack put in a folder for it (its program's
    /// <c>logs collect</c>: the game's logs, the plugin's, the agent's, the relay's; the pack knows where its game keeps them). One zip, where the player says.
    /// </summary>
    public static class LogBundle
    {
        /// <param name="packLogsDir">The folder the game's pack filled, or null when there is none (its files go into the zip as they are, folders kept).</param>
        public static string Collect(string? packLogsDir, string? zipPath = null)
        {
            // the player picks where the zip goes (Settings > Save the logs); the Desktop is only the default
            zipPath ??= Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory), $"Cairn-logs-{DateTime.Now:yyyyMMdd-HHmmss}.zip");

            using var zip = ZipFile.Open(zipPath, ZipArchiveMode.Create);

            // which build this is, in one small file (the first thing to look at when two players' logs disagree)
            try
            {
                var info = zip.CreateEntry("cairn-info.txt", CompressionLevel.Optimal);
                using var iw = new StreamWriter(info.Open());
                iw.WriteLine($"Cairn version : {Globals.CairnVersion}");
                iw.WriteLine($"Handshake     : {Globals.Version}   (players with the same Major.Minor join each other)");
                iw.WriteLine($"Saved at      : {DateTime.Now:yyyy-MM-dd HH:mm:ss}");
                iw.WriteLine($"Windows       : {Environment.OSVersion.VersionString}");
                iw.WriteLine("Note          : the game keeps writing its log while it runs; for the whole session, save the logs after closing the game.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }

            // everything is read with full sharing: the game, the agent and Serilog hold their files open while they run
            void AddIfPresent(string? path, string entryName)
            {
                try
                {
                    if (string.IsNullOrWhiteSpace(path) || !File.Exists(path)) return;
                    using var src = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
                    var entry = zip.CreateEntry(entryName, CompressionLevel.Optimal);
                    using var dst = entry.Open();
                    src.CopyTo(dst);
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { /* one unreadable file must not sink the bundle */ }
            }

            // the game's pack's files, as its program laid them out
            if (!string.IsNullOrWhiteSpace(packLogsDir) && Directory.Exists(packLogsDir))
            {
                try
                {
                    string root = Path.GetFullPath(packLogsDir);
                    foreach (var f in Directory.EnumerateFiles(root, "*", SearchOption.AllDirectories).Take(200))
                        AddIfPresent(f, Path.GetRelativePath(root, f).Replace('\\', '/'));
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
            }

            // Serilog's rolling file names are app<yyyyMMdd>.log; take the two newest so the bundle covers a session that straddled midnight
            try
            {
                foreach (var f in new DirectoryInfo(Globals.AppFolder).GetFiles("app*.log").OrderByDescending(f => f.LastWriteTimeUtc).Take(2))
                    AddIfPresent(f.FullName, f.Name);
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }

            // the launcher's real settings file
            AddIfPresent(Path.GetFullPath("settings.json"), "settings.json");
            AddIfPresent(Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "settings.json"), "settings.launcher-dir.json");
            AddIfPresent(Globals.ConfigFilePath, "config.json");

            return zipPath;
        }
    }
}
