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
    public sealed record PreflightItem(string Title, CheckStatus Status, string Detail, string Fix = "");

    /// <summary>What a line that is not fine can do about itself: the key of the action, and the words on its button.</summary>
    public static class PreflightFix
    {
        public const string PlaceMod = "place-mod";
        public const string InstallMod = "install-mod", UpdateMod = "update-mod", StartSteam = "start-steam", LocateGame = "locate-game", UpdateLauncher = "update-launcher";

        public static string Label(string fix) => fix switch
        {
            InstallMod => "DOWNLOAD THE MOD",
            PlaceMod => "PUT THE MOD IN THE GAME",
            UpdateMod => "UPDATE THE MOD",
            StartSteam => "START STEAM",
            LocateGame => "LOCATE THE GAME",
            UpdateLauncher => "UPDATE CAIRN",
            _ => "",
        };

        /// <summary>The fix for a rule of a game's own setup list, by its kind.</summary>
        public static string ForRule(string kind) => kind switch
        {
            "steam-running" => StartSteam,
            "game-found" => LocateGame,
            "mod-installed" => InstallMod,
            _ => "",
        };
    }

    /// <summary>What the launcher knows about this PC and this game when the ready check is asked for.</summary>
    public sealed record PreflightInput(
        bool Supported, bool GameFound, bool ModInstalled, bool SetupOk, bool SteamRunning,
        bool PortFree, bool GameRunning, string? NewerLauncher, string? ModBlocked = null);

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
                : new("The game", CheckStatus.Fail, "Not found. Set its path in Settings, or use Locate game on its page.", PreflightFix.LocateGame));

            list.Add(!i.ModInstalled
                ? new("The mod", CheckStatus.Fail, "Not installed. Download it from the game's page.", PreflightFix.InstallMod)
                : i.ModBlocked is not null
                    ? new("The mod", CheckStatus.Warn, $"This version has a known problem: {i.ModBlocked} Update it from the game's page when a new one is out.", PreflightFix.UpdateMod)
                    : new("The mod", CheckStatus.Ok, "Downloaded and installed."));

            list.Add(i.SetupOk
                ? new("Setup", CheckStatus.Ok, "Everything the checklist looks at is in place.")
                : new("Setup", CheckStatus.Warn, "The setup checklist has something left to do. Open it from the game's page."));

            list.Add(i.SteamRunning
                ? new("Steam", CheckStatus.Ok, "Running.")
                : new("Steam", CheckStatus.Warn, "Not running. Friends join through Steam, so start it first.", PreflightFix.StartSteam));

            list.Add(i.PortFree
                ? new("Host port", CheckStatus.Ok, "Free on this PC.")
                : new("Host port", CheckStatus.Warn, "Already in use. Hosting may fail; change the port in Settings, or close what uses it."));

            list.Add(i.GameRunning
                ? new("The game", CheckStatus.Warn, "It is already running. Close it before starting a new session.")
                : new("Nothing running", CheckStatus.Ok, "The game is closed, so a new session can start."));

            list.Add(i.NewerLauncher is null
                ? new("Version", CheckStatus.Ok, "This is the newest Cairn.")
                : new("Version", CheckStatus.Warn, $"Cairn {i.NewerLauncher} is out. Both players need the same version to play together.", PreflightFix.UpdateLauncher));

            return list;
        }

        /// <summary>No line fails (warnings are advice, not blockers).</summary>
        public static bool CanPlay(IReadOnlyList<PreflightItem> items) => items.All(x => x.Status != CheckStatus.Fail);
    }
}
