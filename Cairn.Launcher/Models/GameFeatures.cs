// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;

namespace Cairn.Launcher.Models
{
    /// <summary>Where a feature stands. Everything a game's mod does, plans or cannot do is one list with one of these on each line.</summary>
    public enum FeatureStatus
    {
        /// <summary>Works, and has been played in real sessions.</summary>
        Ready,
        /// <summary>Written and checked by tests; still being tried in real sessions.</summary>
        InTest,
        /// <summary>Not built yet, intended.</summary>
        Planned,
    }

    /// <summary>One line of a game's feature card.</summary>
    public sealed record GameFeature(string Name, string Detail, FeatureStatus Status = FeatureStatus.Ready);

    /// <summary>A category (a tab on the card) with its lines. A new feature is one more line in a group; a new category is one more group.</summary>
    public sealed record GameFeatureGroup(string Title, IReadOnlyList<GameFeature> Items);

    /// <summary>What the mod of one game does, shown as a card on the game's page. A game with no mod has none, and no card.</summary>
    public static class GameFeatures
    {
        /// <summary>The id of Kingdom Come: Deliverance II (GameProfileRegistry.Kcd2Id).</summary>
        public const string Kcd2Id = "kcd2";

        public static IReadOnlyList<GameFeatureGroup> For(string gameId) =>
            string.Equals(gameId, Kcd2Id, StringComparison.OrdinalIgnoreCase) ? Kcd2 : Array.Empty<GameFeatureGroup>();

        public static int Count(IReadOnlyList<GameFeatureGroup> groups, FeatureStatus status) =>
            groups.Sum(g => g.Items.Count(f => f.Status == status));

        public static string Label(FeatureStatus s) => s switch
        {
            FeatureStatus.Ready => "Ready",
            FeatureStatus.InTest => "In test",
            _ => "Planned",
        };

        private static readonly GameFeatureGroup[] Kcd2 =
        {
            new("Sessions", new GameFeature[]
            {
                new("Host and join", "Join by Steam code, address or from the friends who are hosting. Both players need the same version."),
                new("Same world", "The host's world is sent to the joiner, who loads it: the same NPCs, horses, items, chests, doors and weather."),
                new("Start and Join on the game's menu", "Start Game, Join Game and a new adventure straight from the game's own main menu."),
                new("Stay together", "A leash warns and then pulls a player who strays; the host can bring a partner to them."),
                new("Sleep and wait together", "Time skips for everyone at once, and the host's clock rules."),
            }),
            new("Players", new GameFeature[]
            {
                new("Partner's figure", "Name above their head, health, walking, running, jumping, crouching and riding, with their clothes and weapons."),
                new("Everything they do", "Hands and items, doors, herbs, whistle, carrying, minigames and about 1,170 of the game's body animations."),
                new("On the map", "Every other player is drawn on your map with their name, and moves with them.", FeatureStatus.InTest),
                new("Dice", "Play dice with a partner."),
            }),
            new("Fights and law", new GameFeature[]
            {
                new("Fighting together", "Players hit NPCs, NPCs fight back, and the swings are shown on both screens.", FeatureStatus.InTest),
                new("NPCs can hurt a joiner", "Blows from NPCs, guards and dogs reach the joiner and can knock them down or kill them.", FeatureStatus.InTest),
                new("Friendly fire", "Players can hurt each other; the host decides if it is on."),
                new("Death without Game Over", "A death drops a grave and a respawn instead of ending the session."),
                new("Crime and guards", "A crime counts for both players or only the one who did it, and guards pursue the guilty one."),
            }),
            new("Story and talk", new GameFeature[]
            {
                new("Shared quests", "Quests, objectives and cutscenes follow the host; the joiner is held and catches up."),
                new("Subtitles", "What the host and NPCs say in conversations and cutscenes appears for the other players, in their own language, with the speaker's name.", FeatureStatus.InTest),
            }),
            new("Menu and tools", new GameFeature[]
            {
                new("In-game mod menu", "Insert opens a menu drawn by the launcher: name badge, ping line, friendly fire, crime, fast travel, whistle, free movement (F3), bring partner, I'm stuck."),
                new("Collect everyone's logs", "The host can ask every player for their logs and gets them in one folder; each player can switch this off.", FeatureStatus.InTest),
                new("DLSS 5 (RTX cards)", "A tick in the game's settings puts DLSS 5 into the game through DLSS 5 Autopilot and OptiScaler, and takes it out again; the game's menu then opens its overlay. Community software, never tried with Cairn in a real session.", FeatureStatus.InTest),
            }),
        };
    }
}
