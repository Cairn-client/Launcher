// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// 0.2.0 renamed the programs the launcher starts (the native plugin, the agent, the relay, the master server). A settings.json
    /// written by 0.1.x still names the old files, and the launcher then reports "a mod file is missing". The old default names are
    /// corrected in memory (never written unless the player changes that key); a path the player chose themselves is left alone.
    /// </summary>
    public static class RenamedPrograms
    {
        private static readonly (string Old, string New)[] Dll =
            { ("KCDMP.dll", "CairnNative.dll") };
        private static readonly (string Old, string New)[] Agent =
            { ("KcdMpClient.exe", "CairnClient.exe") };
        private static readonly (string Old, string New)[] Relay =
            { ("KcdMpServer.exe", "CairnServer.exe") };
        private static readonly (string Old, string New)[] MasterServer =
            { (@"MasterServer\KcdMpMasterServer.exe", @"MasterServer\CairnMasterServer.exe"), ("KcdMpMasterServer.exe", "CairnMasterServer.exe") };

        public static void Apply(AppSettings s)
        {
            s.DllPath = Fix(s.DllPath, Dll);
            s.AgentPath = Fix(s.AgentPath, Agent);
            s.RelayPath = Fix(s.RelayPath, Relay);
            s.MasterServerPath = Fix(s.MasterServerPath, MasterServer);
        }

        private static string Fix(string current, (string Old, string New)[] renames)
        {
            foreach (var (oldName, newName) in renames)
                if (string.Equals(current?.Trim(), oldName, StringComparison.OrdinalIgnoreCase)) return newName;
            return current ?? "";
        }
    }
}
