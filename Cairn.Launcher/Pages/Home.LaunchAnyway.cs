// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Threading.Tasks;
using Cairn.Launcher.Models;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// A finding about the game's mod files is never a dead end: the message offers LAUNCH ANYWAY, the same launch without the file check.
    /// The launcher never changes Windows' security settings and never adds an exclusion: the player allows a file themselves, guided by the message.
    /// </summary>
    public partial class Home
    {
        private bool launchAnywayAvailable;

        /// <summary>The message's LAUNCH ANYWAY: the same launch, without the mod-file check (logged).</summary>
        private async Task LaunchAnywayAsync()
        {
            showMessage = false;
            launchAnywayAvailable = false;
            if (lastLaunchServer is not { } server) return;
            Log.Warning("MP-LAUNCH launch anyway: the player started the game past the mod-file check");
            skipModFileCheckOnce = true;
            await LaunchGame(server);
        }

        private ServerInfo? lastLaunchServer;
        private bool skipModFileCheckOnce;
    }
}
