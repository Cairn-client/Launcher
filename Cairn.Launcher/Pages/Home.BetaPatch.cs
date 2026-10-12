// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// the first public-beta patch, launcher half.
    /// <list type="bullet">
    /// <item>No injector exe: the launcher loads CairnNative.dll into the game itself (GameInjector), with the
    ///   injector's checks (the game it started, the shipped DLL by sha256, x64) failing closed.</item>
    /// <item>The file check says what it found (missing, different, unreadable, empty) and where it looked
    ///   (the install folder, the staged copy, the Modding Tools' Mods folder), with the full path, and is
    ///   never a dead end: a mod-file finding offers LAUNCH ANYWAY.</item>
    /// <item>The Visual C++ 2013 runtime the Modding Tools' trace server needs (an optional checklist step).</item>
    /// </list>
    /// The launcher never changes Windows' security settings and never adds an exclusion: the player allows
    /// a file himself, guided by the message.
    /// </summary>
    public partial class Home
    {
        // ------------------------------------------------------------ the DLL load (2.1)

        // ------------------------------------------------------------ the file check (2.2)

        /// <summary>
        /// The Modding Tools' Mods\<mod folder> for the configured game. from the install root the
        /// setup checklist uses (the folder above Bin), not GameRootOf's steam_appid.txt walk -- that walk
        /// falls back to the exe's own Bin\... folder where no steam_appid.txt exists (a fresh Modding Tools
        /// install), and the field's launcher then reported a present cairn.pak as gone and refused to launch.
        /// </summary>
        internal static string ModDirFor(string gamePath) =>
            ModInstall.TargetDir(GameLocator.InstallRootOf(gamePath) ?? GameRootOf(gamePath));

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

        // ------------------------------------------------------------ Cairn: NPC lip sync

        // ------------------------------------------------------------ the Visual C++ 2013 runtime (2.3)

    }
}
