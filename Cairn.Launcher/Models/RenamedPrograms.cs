// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// The programs the launcher starts (the native plugin, the agent, the relay) used to ship with the installer and be named in
    /// settings.json by their file names. They now come with each game's mod pack, and an empty path means "the pack's". A settings.json
    /// written by an earlier launcher still names the old default files; those names are corrected in memory (never written unless the
    /// player changes that key). A path the player chose themselves is left alone. The master server still ships with the launcher.
    /// </summary>
    public static class RenamedPrograms
    {
        private static readonly string[] Dll = { "KCDMP.dll", "CairnNative.dll" };
        private static readonly string[] Agent = { "KcdMpClient.exe", "CairnClient.exe" };
        private static readonly string[] Relay = { "KcdMpServer.exe", "CairnServer.exe" };
        private static readonly (string Old, string New)[] MasterServer =
            { (@"MasterServer\KcdMpMasterServer.exe", @"MasterServer\CairnMasterServer.exe"), ("KcdMpMasterServer.exe", "CairnMasterServer.exe") };

        public static void Apply(AppSettings s)
        {
            s.DllPath = ToPack(s.DllPath, Dll);
            s.AgentPath = ToPack(s.AgentPath, Agent);
            s.RelayPath = ToPack(s.RelayPath, Relay);
            s.MasterServerPath = Fix(s.MasterServerPath, MasterServer);
        }

        private static string ToPack(string current, string[] oldDefaults)
        {
            foreach (var name in oldDefaults)
                if (string.Equals(current?.Trim(), name, StringComparison.OrdinalIgnoreCase)) return "";
            return current ?? "";
        }

        private static string Fix(string current, (string Old, string New)[] renames)
        {
            foreach (var (oldName, newName) in renames)
                if (string.Equals(current?.Trim(), oldName, StringComparison.OrdinalIgnoreCase)) return newName;
            return current ?? "";
        }
    }
}