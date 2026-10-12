// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
// Portions from the original project, marczukmichal/kcd2-multiplayer; its author keeps their copyright (AUTHORS.md).
global using Serilog;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using System.Threading.Tasks;

namespace Cairn.Launcher.Components.Shared
{
    public static class Globals
    {
        public static string AppData { get; } = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        public static string AppFolder { get; } = Path.Combine(AppData, "Cairn");

        public static bool IsMasterOnline { get; set; } = false;

        /// <summary>
        /// was a hardcoded "0.1.0", stale against the real shipped
        /// version (VERSION said 0.9.5 at the time this was found). Now read
        /// from the assembly's InformationalVersion, which the csproj sets
        /// from VERSION at build time -- see Cairn.Launcher.csproj's
        /// VersionFileContent property. One source, not two to keep in sync.
        /// </summary>
        public static string Version { get; } =
            Assembly.GetExecutingAssembly()
                .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion
            ?? "0.0.0";

        /// <summary>
        /// Cairn: the fork's own release number, stamped from VERSION at build time (Cairn.Launcher.csproj), so the version line
        /// follows each release by itself. <see cref="Version"/> stays upstream's protocol string (the relay needs it unchanged).
        /// </summary>
        public static string CairnVersion { get; } =
            Assembly.GetExecutingAssembly()
                .GetCustomAttributes<AssemblyMetadataAttribute>()
                .FirstOrDefault(a => a.Key == "Cairn.Version")?.Value
            ?? "dev";
        public static string ConfigFilePath { get; } = Path.Combine(AppFolder, "config.json");

        public static StyleProfile CurrentStyleProfile { get; set; } = new StyleProfile();
        public static short MaxStyleProfilesHistory { get; } = 5;
        public static string StylesHistoryFilePath { get; } = Path.Combine(AppFolder, "style_profiles_history.json");
        public static string StylesProfilesDirectoryPath { get; } = Path.Combine(AppFolder, "StyleProfiles");

        public static event Action? OnStyleChanged;

        public static ModsProfile CurrentModsProfile { get; set; } = new ModsProfile();
        public static string ModsProfilesFilePath { get; } = Path.Combine(AppFolder, "mods_profiles.json");
        public static string ModsProfilesDirectoryPath { get; } = Path.Combine(AppFolder, "ModsProfiles");


        static Globals()
        {
            // Ensure the application folder exists. A launcher that predates the rename kept its files in
            // %APPDATA%\CAIRN_Launcher; they are copied (never moved) into the new folder once.
            if (!Directory.Exists(AppFolder))
            {
                Directory.CreateDirectory(AppFolder);
                try
                {
                    string legacy = Path.Combine(AppData, "CAIRN_Launcher");
                    if (Directory.Exists(legacy))
                    {
                        foreach (string dir in Directory.EnumerateDirectories(legacy, "*", SearchOption.AllDirectories))
                            Directory.CreateDirectory(Path.Combine(AppFolder, Path.GetRelativePath(legacy, dir)));
                        foreach (string file in Directory.EnumerateFiles(legacy, "*", SearchOption.AllDirectories))
                            File.Copy(file, Path.Combine(AppFolder, Path.GetRelativePath(legacy, file)), overwrite: false);
                    }
                }
                catch { /* a failed migration leaves a fresh, empty folder */ }
            }
        }
        public static void NotifyStyleChanged()
        {
            OnStyleChanged?.Invoke();
        }
    }
}
