// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using Cairn.Setup;

namespace Cairn.Launcher.Models
{
    /// <summary>The one thing the launcher still says about the game's mod: which release players must share (the downloaded pack's MAJOR.MINOR).</summary>
    public static class ModLua
    {
        public static void Reload() { }

        /// <summary>The mod's release as players compare it: the downloaded pack's MAJOR.MINOR, shown in the host window.</summary>
        public static string Handshake(string gameId)
        {
            var v = PackStore.Read(PackStore.DefaultRoot, gameId)?.Version;
            return System.Version.TryParse(v, out var p) ? $"{p.Major}.{p.Minor}.0" : Components.Shared.Globals.Version;
        }
    }
}
