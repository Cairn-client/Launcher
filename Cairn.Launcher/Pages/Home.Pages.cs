// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Launcher.Models;

namespace Cairn.Launcher.Pages
{
    /// <summary>The data of the pages that are views over what the launcher already knows: Activity, and the cards of Home that summarise it.</summary>
    public partial class Home
    {
        /// <summary>Every game that was played from here, the latest first.</summary>
        private IReadOnlyList<(GameProfile Game, RecentSession Session)> ActivityRows() =>
            recent.Entries.OrderByDescending(e => e.LastUtc)
                .Select(e => (Game: GameProfileRegistry.Get(e.GameId), Session: e))
                .Where(x => x.Game is not null).Select(x => (x.Game!, x.Session)).ToList();
    }
}
