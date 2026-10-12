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

    /// <summary>What the mod of one game does, shown as a card on the game's page. It is the game's own list (its game.json); a game with none has no card.</summary>
    public static class GameFeatures
    {
        public static IReadOnlyList<GameFeatureGroup> For(string gameId) =>
            (IReadOnlyList<GameFeatureGroup>?)GameProfileRegistry.Get(gameId)?.Features ?? Array.Empty<GameFeatureGroup>();

        public static int Count(IReadOnlyList<GameFeatureGroup> groups, FeatureStatus status) =>
            groups.Sum(g => g.Items.Count(f => f.Status == status));

        public static string Label(FeatureStatus s) => s switch
        {
            FeatureStatus.Ready => "Ready",
            FeatureStatus.InTest => "In test",
            _ => "Planned",
        };

    }
}
