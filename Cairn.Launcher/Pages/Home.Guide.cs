// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using System.Diagnostics;
using Cairn.Launcher.Components.Views;
using Cairn.Launcher.Models;
using Cairn.Setup;

namespace Cairn.Launcher.Pages
{
    /// <summary>What the welcome guide does with the player: it looks at Steam, lists the games found, picks one and downloads its mod.</summary>
    public partial class Home
    {
        private static bool IsSteamRunning()
        {
            try { return Process.GetProcessesByName("steam").Length > 0; }
            catch (InvalidOperationException) { return false; }
        }

        private void StartSteamFromGuide() => Cairn.Launcher.Components.Shared.UrlLauncher.Open("steam://open/main");

        private IReadOnlyList<TourModal.GuideGame> GuideGames() =>
            GameProfileRegistry.All().Where(g => g.IsSupported).Select(g => new TourModal.GuideGame(g.Id, g.DisplayName, IsGameFound(g))).ToList();

        private void PickFromGuide(string id) => SelectGame(id);

        private async Task DownloadFromGuide()
        {
            if (SelectedProfile is { } g) await DownloadPackAsync(g.Id);
        }
    }
}
