// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
namespace Cairn.Launcher.Models
{
    /// <summary>
    /// One game Cairn can start a shared session for. Everything that used to be a Kingdom Come: Deliverance II
    /// constant in the launcher (executable, Steam app id, mod payload folder, menu picture, default server)
    /// is a field here, so a second title is a new profile and not a new launcher.
    /// </summary>
    /// <summary>One Steam app a game needs: its name, its app id, and whether it has a store page (a tool or a mod kit may only have a library entry).</summary>
    public record SteamRequirement(string Name, uint AppId, bool HasStorePage = true);

    public class GameProfile
    {
        public string Id { get; set; } = string.Empty;
        public string DisplayName { get; set; } = string.Empty;
        /// <summary>The studio that made the game; the game list can be grouped by it.</summary>
        public string Developer { get; set; } = string.Empty;

        /// <summary>The series or license the game belongs to (Kerbal Space Program and its sequel, every Star Wars game); the game list can group by it, across studios.</summary>
        public string Franchise { get; set; } = string.Empty;
        public uint SteamAppId { get; set; }

        /// <summary>The app whose Steam store and library pages describe the game, when that differs from <see cref="SteamAppId"/> (Kingdom Come II runs from its Modding Tools app, which has no store page of its own). 0 = the same.</summary>
        public uint StoreAppId { get; set; }

        public uint PageAppId => StoreAppId != 0 ? StoreAppId : SteamAppId;
        public string DefaultExeName { get; set; } = string.Empty;
        /// <summary>Where the game's mod is published ("owner/name" on GitHub): its latest release carries the pack the launcher downloads.</summary>
        public string ModRepo { get; set; } = string.Empty;
        public string BackgroundImage { get; set; } = string.Empty;
        public string DefaultServerApi { get; set; } = string.Empty;
        public bool IsInstalled { get; set; }
        public string InstallPath { get; set; } = string.Empty;

        /// <summary>False for a game that is listed but has no multiplayer support yet: its page shows, Host and Join stay off.</summary>
        public bool IsSupported { get; set; } = true;

        /// <summary>One or two sentences shown on the game page.</summary>
        public string Description { get; set; } = string.Empty;

        /// <summary>The game's logo (a transparent PNG), shown in the game list and on its page.</summary>
        public string LogoImage { get; set; } = string.Empty;

        /// <summary>The game's accent colour (a #rrggbb), used on its slide on the Home page.</summary>
        public string Accent { get; set; } = string.Empty;

        /// <summary>A wide, high-resolution picture for the Home slideshow (empty means the background picture is used).</summary>
        public string HeroImage { get; set; } = string.Empty;

        /// <summary>A tall poster for shelves; empty means the background picture is cropped instead.</summary>
        public string PosterImage { get; set; } = string.Empty;

        /// <summary>What the player must own on Steam to play this game with Cairn (the game, its modding tool), shown on the game's page with links.</summary>
        public System.Collections.Generic.List<SteamRequirement> Requirements { get; set; } = new();

        /// <summary>Screenshots shown as thumbnails on the game's slide.</summary>
        public System.Collections.Generic.List<string> Screenshots { get; set; } = new();
    }
}
