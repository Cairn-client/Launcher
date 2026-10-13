// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using System.IO;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

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

        // ---- the Saves page: every game's saves, asked of each game's own program ----
        private IReadOnlyList<(GameProfile Game, IReadOnlyList<BridgeSaveSlot> Slots)> allSaves = Array.Empty<(GameProfile, IReadOnlyList<BridgeSaveSlot>)>();
        private bool allSavesLoading;
        private string? allSavesAsked;

        private async Task LoadAllSavesAsync()
        {
            if (allSavesLoading) return;
            allSavesLoading = true;
            allSavesAsked = null;
            await InvokeAsync(StateHasChanged);
            try
            {
                var asks = GameProfileRegistry.All()
                    .Select(g => (Game: g, Program: ExternalInstall.OptionsProgramPath(PacksRoot, g.Id)))
                    .Where(x => x.Program is not null && File.Exists(x.Program))
                    .Select(async x => (x.Game, Slots: await SavesRunner.ListAsync(x.Program!, timeout: TimeSpan.FromSeconds(15))))
                    .ToList();
                var done = await Task.WhenAll(asks);
                allSaves = done.Where(d => d.Slots is { Count: > 0 }).OrderBy(d => d.Game.DisplayName, StringComparer.OrdinalIgnoreCase)
                    .Select(d => (d.Game, d.Slots!)).ToList();
            }
            catch (Exception ex) { Log.Debug(ex, "saves: could not be listed"); }
            finally { allSavesLoading = false; await InvokeAsync(StateHasChanged); }
        }

        private async Task DeleteFromSavesPageAsync((string GameId, string SaveId) save)
        {
            string key = save.GameId + ":" + save.SaveId;
            if (allSavesAsked != key) { allSavesAsked = key; return; }   // the first press asks, the second deletes
            allSavesAsked = null;
            if (ExternalInstall.OptionsProgramPath(PacksRoot, save.GameId) is not { } program) { ShowToast("The mod's program is not there.", true); return; }
            var (ok, message, _) = await SavesRunner.DeleteAsync(program, save.SaveId);
            Log.Information("saves: {Game} {Save} deleted: {Ok} {Message}", save.GameId, save.SaveId, ok, message);
            ShowToast(message, !ok);
            allSavesLoading = false;
            await LoadAllSavesAsync();
        }    }
}
