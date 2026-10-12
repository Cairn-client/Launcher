// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// Settings > Saves > "Delete all saves": every save the game has made and every character and chest the mod kept, so a new
    /// adventure starts clean. The start saves (what New adventure begins from) stay.
    /// </summary>
    public static class SaveCleaner
    {
        public sealed record Result(int Saves, int ModFolders, string Error);

        /// <summary>The folders and files the cleaning would remove, as a plain list.</summary>
        public static IReadOnlyList<string> Targets(string savesRoot, IEnumerable<string> dataRoots)
        {
            var list = new List<string>();
            if (Directory.Exists(savesRoot))
                foreach (var line in Directory.EnumerateDirectories(savesRoot, "playline*"))
                    list.AddRange(Directory.EnumerateFiles(line, "*.whs"));
            foreach (var root in dataRoots.Where(Directory.Exists))
            {
                foreach (var d in new[] { "henry", "chests", "hosted-henry" })
                {
                    string p = Path.Combine(root, d);
                    if (Directory.Exists(p)) list.Add(p);
                }
                foreach (var f in Directory.EnumerateFiles(root, "*-staged.json")) list.Add(f);
            }
            return list;
        }

        public static string DefaultSavesRoot =>
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Saved Games", "kingdomcome2", "saves");

        /// <summary>Where the agent keeps its characters and chests: the earlier Cairn mod's folder and the Kingdom Come: Together default.</summary>
        public static IReadOnlyList<string> DefaultDataRoots
        {
            get
            {
                string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                return new[] { Path.Combine(local, "Cairn"), Path.Combine(local, "KCDMP") };
            }
        }

        public static Result Clean(string savesRoot, IEnumerable<string> dataRoots)
        {
            int saves = 0, folders = 0;
            string error = "";
            foreach (var path in Targets(savesRoot, dataRoots))
            {
                try
                {
                    if (Directory.Exists(path)) { Directory.Delete(path, true); folders++; }
                    else
                    {
                        File.Delete(path);
                        if (path.EndsWith(".whs", StringComparison.OrdinalIgnoreCase)) saves++; else folders++;
                    }
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                {
                    error = $"{Path.GetFileName(path)}: {ex.Message}";
                }
            }
            return new Result(saves, folders, error);
        }
    }
}