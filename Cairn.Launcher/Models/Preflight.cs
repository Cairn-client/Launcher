// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;

namespace Cairn.Launcher.Models
{
    public enum CheckStatus { Ok, Warn, Fail }

    /// <summary>One line of the ready check: what was looked at, how it stands, and what to do about it.</summary>
    public sealed record PreflightItem(string Title, CheckStatus Status, string Detail);

    /// <summary>What the launcher knows about this PC and this game when the ready check is asked for.</summary>
    public sealed record PreflightInput(
        bool Supported, bool GameFound, bool ModInstalled, bool SetupOk, bool SteamRunning,
        bool PortFree, bool GameRunning, string? NewerLauncher);

    /// <summary>The "ready check" before hosting or joining: everything that commonly goes wrong, in plain words, as a pure function.</summary>
    public static class Preflight
    {
        public static IReadOnlyList<PreflightItem> Build(PreflightInput i)
        {
            var list = new List<PreflightItem>();
            if (!i.Supported)
            {
                list.Add(new("Multiplayer", CheckStatus.Fail, "Multiplayer for this game is still in development."));
                return list;
            }

            list.Add(i.GameFound
                ? new("The game", CheckStatus.Ok, "Found on this PC.")
                : new("The game", CheckStatus.Fail, "Not found. Set its path in Settings, or use Locate game on its page."));

            list.Add(i.ModInstalled
                ? new("The mod", CheckStatus.Ok, "Downloaded and installed.")
                : new("The mod", CheckStatus.Fail, "Not installed. Download it from the game's page."));

            list.Add(i.SetupOk
                ? new("Setup", CheckStatus.Ok, "Everything the checklist looks at is in place.")
                : new("Setup", CheckStatus.Warn, "The setup checklist has something left to do. Open it from the game's page."));

            list.Add(i.SteamRunning
                ? new("Steam", CheckStatus.Ok, "Running.")
                : new("Steam", CheckStatus.Warn, "Not running. Friends join through Steam, so start it first."));

            list.Add(i.PortFree
                ? new("Host port", CheckStatus.Ok, "Free on this PC.")
                : new("Host port", CheckStatus.Warn, "Already in use. Hosting may fail; change the port in Settings, or close what uses it."));

            list.Add(i.GameRunning
                ? new("The game", CheckStatus.Warn, "It is already running. Close it before starting a new session.")
                : new("Nothing running", CheckStatus.Ok, "The game is closed, so a new session can start."));

            list.Add(i.NewerLauncher is null
                ? new("Version", CheckStatus.Ok, "This is the newest Cairn.")
                : new("Version", CheckStatus.Warn, $"Cairn {i.NewerLauncher} is out. Both players need the same version to play together."));

            return list;
        }

        /// <summary>No line fails (warnings are advice, not blockers).</summary>
        public static bool CanPlay(IReadOnlyList<PreflightItem> items) => items.All(x => x.Status != CheckStatus.Fail);
    }
}
