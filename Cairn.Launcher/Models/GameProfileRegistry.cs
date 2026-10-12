// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// The games Cairn knows about, by <see cref="GameProfile.Id"/>. Built-in profiles are registered on first use;
    /// a game found on this PC is recorded with <see cref="MarkInstalled"/>. Thread-safe.
    /// </summary>
    public static class GameProfileRegistry
    {
        public const string Kcd2Id = "kcd2";
        public const string Kcd1Id = "kcd1";
        public const string Fo4Id  = "fo4";

        private static readonly object Gate = new();
        private static readonly Dictionary<string, GameProfile> Profiles = new(StringComparer.OrdinalIgnoreCase);

        static GameProfileRegistry()
        {
            Register(new GameProfile
            {
                Id = Kcd2Id,
                DisplayName = "Kingdom Come: Deliverance II",
                Developer = "Warhorse Studios",
                Franchise = "Kingdom Come",
                SteamAppId = 2429020,                 // the Modding Tools build, which is what both players run (AppSettings.SteamAppId)
                StoreAppId = 1771300,                 // the retail game: the Modding Tools app has no store page
                Requirements =
                [
                    new SteamRequirement("Kingdom Come: Deliverance II", 1771300),
                    new SteamRequirement("Kingdom Come: Deliverance II Modding Tools", 2429020, HasStorePage: false),
                ],
                DefaultExeName = "KingdomCome.exe",
                PayloadFolder = "cairn",              // the mod folder placed into the game's Mods directory
                BackgroundImage = "img/games/kcd2.jpg",   // the page artwork, shipped with the launcher (wwwroot/img/games)
                LogoImage = "img/games/kcd2-logo.png",
                Description = "Explore Bohemia side by side. The host's world is shared with a friend: the same NPCs, weather, doors and fights.",
                DefaultServerApi = new AppSettings().MasterServerUrl,
            });

            // Listed so it can be found and selected; the multiplayer plugin does not target this game yet.
            Register(new GameProfile
            {
                Id = Kcd1Id,
                DisplayName = "Kingdom Come: Deliverance",
                Developer = "Warhorse Studios",
                Franchise = "Kingdom Come",
                SteamAppId = 379430,                  // the retail game
                DefaultExeName = "KingdomCome.exe",
                BackgroundImage = "img/games/kcd1.jpg",
                LogoImage = "img/games/kcd1-logo.png",
                Description = "Rattay, Sasau and the Bohemian countryside. Co-op for the first game is on the roadmap.",
                DefaultServerApi = new AppSettings().MasterServerUrl,
                IsSupported = false,
            });

            Register(new GameProfile
            {
                Id = Fo4Id,
                DisplayName = "Fallout 4",
                Developer = "Bethesda Game Studios",
                Franchise = "Fallout",
                SteamAppId = 377160,                  // the retail game
                DefaultExeName = "Fallout4.exe",
                BackgroundImage = "img/games/fo4-bg.jpg",
                LogoImage = "img/games/fo4-logo.png",
                Description = "As the sole survivor of Vault 111, explore the Commonwealth wasteland. Co-op support is in research and development.",
                DefaultServerApi = new AppSettings().MasterServerUrl,
                IsSupported = false,
            });

            // Listed so they can be found and selected; no multiplayer plugin targets these games yet.
            RegisterUpcoming("ai", "Alien: Isolation", "Creative Assembly", "Alien", 214490, "AlienIsolation.exe",
                "Survive the Sevastopol station with a friend at your side. Co-op support is in research and development.");
            RegisterUpcoming("hla", "Half-Life: Alyx", "Valve", "Half-Life", 546560, "hlvr.exe",
                "Fight the Combine in City 17 together. Co-op support is in research and development.");
            RegisterUpcoming("ksp", "Kerbal Space Program", "Squad", "Kerbal Space Program", 220200, "KSP_x64.exe",
                "Build rockets and run a space program as a crew. Co-op support is in research and development.");
            RegisterUpcoming("ksp2", "Kerbal Space Program 2", "Intercept Games", "Kerbal Space Program", 954850, "KSP2_x64.exe",
                "Interstellar rockets and colonies, built together. Co-op support is in research and development.");
            RegisterUpcoming("jfo", "Star Wars Jedi: Fallen Order", "Respawn Entertainment", "Star Wars", 1172380, "SwGame-Win64-Shipping.exe",
                "Cal Kestis's run from the Empire, shared with a friend. Co-op support is in research and development.");
            RegisterUpcoming("jsv", "Star Wars Jedi: Survivor", "Respawn Entertainment", "Star Wars", 1774580, "JediSurvivor.exe",
                "Cal Kestis's next chapter, side by side. Co-op support is in research and development.");
            RegisterUpcoming("swo", "Star Wars Outlaws", "Massive Entertainment", "Star Wars", 2842040, "Outlaws.exe",
                "Work the galaxy's underworld as a pair of scoundrels. Co-op support is in research and development.");
            RegisterUpcoming("sub", "Subnautica", "Unknown Worlds Entertainment", "Subnautica", 264710, "Subnautica.exe",
                "Dive into the alien ocean of planet 4546B together. Co-op support is in research and development.");
            RegisterUpcoming("subbz", "Subnautica: Below Zero", "Unknown Worlds Entertainment", "Subnautica", 848450, "SubnauticaZero.exe",
                "Explore the frozen seas of 4546B with a partner. Co-op support is in research and development.");
            RegisterUpcoming("fl", "007 First Light", "IO Interactive", "James Bond", 3768760, "007FirstLight.exe",
                "Bond's origin story, shared with a friend. Co-op support is in research and development.");
            RegisterUpcoming("cp77", "Cyberpunk 2077", "CD Projekt Red", "Cyberpunk", 1091500, "Cyberpunk2077.exe",
                "Take on Night City with a friend. Co-op support is in research and development.");
            Enrich();
        }

        // The Home page's slideshow and shelves: each game's accent colour, its tall poster (First Light has none on Steam, so its
        // background is cropped instead) and the screenshots shipped in wwwroot/img/games.
        private static readonly Dictionary<string, string> Accents = new(StringComparer.OrdinalIgnoreCase)
        {
            ["kcd2"] = "#E8C872", ["kcd1"] = "#C9A24A", ["fo4"] = "#6FD08C", ["ai"] = "#7FD14A", ["hla"] = "#F28C28", ["ksp"] = "#4FA3E0",
            ["ksp2"] = "#5BC0BE", ["jfo"] = "#E8683A", ["jsv"] = "#4DB3F5", ["swo"] = "#E8B84A", ["sub"] = "#2AB7CA", ["subbz"] = "#8FD8F0",
            ["fl"] = "#D9B25F", ["cp77"] = "#F3E02A",
        };
        private static readonly HashSet<string> WithoutPoster = new(StringComparer.OrdinalIgnoreCase) { "fl" };

        private static void Enrich()
        {
            foreach (var p in Profiles.Values)
            {
                if (Accents.TryGetValue(p.Id, out var accent)) p.Accent = accent;
                p.HeroImage = $"img/games/{p.Id}-hero.jpg";
                if (!WithoutPoster.Contains(p.Id)) p.PosterImage = $"img/games/{p.Id}-poster.jpg";
                p.Screenshots = new List<string> { $"img/games/shots/{p.Id}-1.jpg", $"img/games/shots/{p.Id}-2.jpg" };
            }
        }

        /// <summary>A game that is listed with its artwork but has no multiplayer support yet (<c>img/games/{id}-bg.jpg</c>, <c>{id}-logo.png</c>).</summary>
        private static void RegisterUpcoming(string id, string displayName, string developer, string franchise, uint steamAppId, string exeName, string description, bool hasLogo = true)
        {
            Register(new GameProfile
            {
                Id = id,
                DisplayName = displayName,
                Developer = developer,
                Franchise = franchise,
                SteamAppId = steamAppId,              // the retail game
                DefaultExeName = exeName,
                BackgroundImage = $"img/games/{id}-bg.jpg",
                LogoImage = hasLogo ? $"img/games/{id}-logo.png" : string.Empty,
                Description = description,
                DefaultServerApi = new AppSettings().MasterServerUrl,
                IsSupported = false,
            });
        }

        /// <summary>Adds a profile, or replaces the one with the same id.</summary>
        public static void Register(GameProfile profile)
        {
            if (profile is null) throw new ArgumentNullException(nameof(profile));
            if (string.IsNullOrWhiteSpace(profile.Id)) throw new ArgumentException("A game profile needs an Id.", nameof(profile));
            lock (Gate) Profiles[profile.Id] = profile;
        }

        public static GameProfile? Get(string id)
        {
            lock (Gate) return Profiles.TryGetValue(id, out var p) ? p : null;
        }

        public static IReadOnlyList<GameProfile> All()
        {
            lock (Gate) return Profiles.Values.OrderBy(p => p.DisplayName, StringComparer.OrdinalIgnoreCase).ToList();
        }

        /// <summary>Records where a game was found (an empty path clears it). False when the id is not registered.</summary>
        public static bool MarkInstalled(string id, string? installPath)
        {
            lock (Gate)
            {
                if (!Profiles.TryGetValue(id, out var p)) return false;
                p.InstallPath = installPath ?? string.Empty;
                p.IsInstalled = !string.IsNullOrWhiteSpace(p.InstallPath);
                return true;
            }
        }
    }
}
