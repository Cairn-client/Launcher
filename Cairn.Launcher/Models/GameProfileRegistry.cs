// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Cairn.Setup;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// The games Cairn knows about, by <see cref="GameProfile.Id"/>. The list is the catalog (Cairn.Setup/Catalog.cs), read from the network
    /// and kept on disk; what the launcher ships with is only the seed, the one game it must be able to show before it has ever reached the
    /// network (Seed/*.json, embedded). A game found on this PC is recorded with <see cref="MarkInstalled"/>. Thread-safe.
    /// </summary>
    public static class GameProfileRegistry
    {
        private static readonly object Gate = new();
        private static readonly Dictionary<string, GameProfile> Profiles = new(StringComparer.OrdinalIgnoreCase);
        private static GameList list = GameList.Empty;

        static GameProfileRegistry()
        {
            if (Seed() is { } seed) Apply(seed, notify: false);
        }

        /// <summary>The catalog's shared part: requirement definitions, blocked mod versions, notices, the repositories of packs that are not a game's mod.</summary>
        public static GameList Catalog { get { lock (Gate) return list; } }

        /// <summary>The list of games changed (a catalog was read); raised on whatever thread applied it.</summary>
        public static event Action? Changed;

        /// <summary>The seed: what the launcher knows without the network (an embedded copy of the KCD2 game.json and a one-game list).</summary>
        internal static CatalogSnapshot? Seed()
        {
            var asm = typeof(GameProfileRegistry).Assembly;
            string? Read(string name)
            {
                using var s = asm.GetManifestResourceStream(name);
                if (s is null) return null;
                using var r = new StreamReader(s);
                return r.ReadToEnd();
            }
            var l = Read("Cairn.Launcher.Seed.games.seed.json") is { } lt ? GameList.Parse(lt) : null;
            if (l is null) return null;
            var games = new List<CatalogGame>();
            foreach (var e in l.Games)
                if (Read($"Cairn.Launcher.Seed.{e.Id}.game.json") is { } gt && GameManifest.Parse(gt) is { } m)
                    games.Add(new CatalogGame(e, m, "img/games/"));   // the seed's pictures ship with the launcher
            return new CatalogSnapshot(l, games, FromCache: true);
        }

        private static CatalogGame? SeedGame(GameEntry entry) =>
            Seed()?.Games.FirstOrDefault(g => string.Equals(g.Entry.Id, entry.Id, StringComparison.OrdinalIgnoreCase)) is { } s
                ? new CatalogGame(entry, s.Manifest, s.BaseUrl) : null;

        /// <summary>Makes the catalog the list of games. A game that stays keeps where it was found on this PC; a game that is no longer listed goes.</summary>
        public static void ApplyCatalog(CatalogSnapshot snapshot) => Apply(snapshot, notify: true);

        private static void Apply(CatalogSnapshot snapshot, bool notify)
        {
            lock (Gate)
            {
                var before = Profiles.ToDictionary(p => p.Key, p => p.Value, StringComparer.OrdinalIgnoreCase);
                Profiles.Clear();
                foreach (var g in snapshot.Games)
                {
                    var p = FromCatalog(g, snapshot.List);
                    if (before.TryGetValue(p.Id, out var old)) { p.InstallPath = old.InstallPath; p.IsInstalled = old.IsInstalled; }
                    Profiles[p.Id] = p;
                }
                // a listed game whose game.json could not be had (no release yet, no copy on disk) is not dropped: it stays what it was, or what the seed knows
                foreach (var entry in snapshot.List.Games.Where(e => !Profiles.ContainsKey(e.Id)))
                {
                    if (before.TryGetValue(entry.Id, out var kept)) Profiles[entry.Id] = kept;
                    else if (SeedGame(entry) is { } fromSeed) Profiles[entry.Id] = FromCatalog(fromSeed, snapshot.List);
                }
                list = snapshot.List;
            }
            if (notify) Changed?.Invoke();
        }

        internal static GameProfile FromCatalog(CatalogGame g, GameList catalog)
        {
            var m = g.Manifest;
            return new GameProfile
            {
                Id = m.Id,
                DisplayName = m.Name,
                Developer = m.Developer,
                Franchise = m.Franchise,
                SteamAppId = m.SteamAppId,
                StoreAppId = m.StoreAppId,
                DefaultExeName = m.Exe,
                ModRepo = g.Entry.Repo,
                ExeIsSetting = g.Entry.ExeSetting,
                BackgroundImage = g.Picture(m.Images.Background),
                LogoImage = g.Picture(m.Images.Logo),
                HeroImage = g.Picture(m.Images.Hero),
                PosterImage = g.Picture(m.Images.Poster),
                Screenshots = m.Images.Screenshots.Select(g.Picture).ToList(),
                Description = m.Description,
                Accent = m.Accent,
                IsSupported = m.Supported,
                DefaultServerApi = new AppSettings().MasterServerUrl,
                Requirements = m.ResolveRequirements(catalog)
                    .Where(c => c.Kind == Component.SteamApp)
                    .Select(c => new SteamRequirement(c.Name, c.AppId, c.HasStorePage)).ToList(),
                Features = m.Features.Select(f => new GameFeatureGroup(f.Title,
                    f.Items.Select(i => new GameFeature(i.Name, i.Detail, i.State switch
                    {
                        FeatureState.InTest => FeatureStatus.InTest,
                        FeatureState.Planned => FeatureStatus.Planned,
                        _ => FeatureStatus.Ready,
                    })).ToList())).ToList(),
                ContentPacks = m.Content.ToList(),
                HostSteps = (m.HostSteps ?? Array.Empty<string>()).ToList(),
                JoinSteps = (m.JoinSteps ?? Array.Empty<string>()).ToList(),
                OwnSession = m.OwnSession,
                Setup = (m.Setup ?? Array.Empty<SetupRule>()).ToList(),
                Options = (m.Options ?? Array.Empty<GameOption>()).ToList(),
            };
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
