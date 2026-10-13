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
        }
        // ---- the News page: the latest release of each mod the player has (or follows), kept for half an hour ----
        private IReadOnlyList<(GameProfile Game, IReadOnlyList<ReleaseNote> Notes)> modNews = Array.Empty<(GameProfile, IReadOnlyList<ReleaseNote>)>();
        private bool modNewsLoading;
        private DateTime modNewsAt = DateTime.MinValue;

        private async Task LoadModNewsAsync(bool force = false)
        {
            if (modNewsLoading || (!force && DateTime.UtcNow - modNewsAt < TimeSpan.FromMinutes(30))) return;
            modNewsLoading = true;
            await InvokeAsync(StateHasChanged);
            try
            {
                var games = GameProfileRegistry.All().Where(g => g.ModRepo.Length > 0 && (IsModDownloaded(g) || HomeFavorites.Contains(g.Id) || recent.Entries.Any(e => e.GameId == g.Id)))
                    .OrderBy(g => g.DisplayName, StringComparer.OrdinalIgnoreCase).Take(10).ToList();
                var asks = games.Select(async g => (Game: g, Notes: await Changelog.FetchRepoAsync(PackHttp, g.ModRepo, 2))).ToList();
                var done = await Task.WhenAll(asks);
                modNews = done.Where(d => d.Notes is { Count: > 0 }).Select(d => (d.Game, d.Notes!)).ToList();
                modNewsAt = DateTime.UtcNow;
            }
            catch (Exception ex) { Log.Debug(ex, "news: the mods'' releases could not be read"); }
            finally { modNewsLoading = false; await InvokeAsync(StateHasChanged); }
        }
        // ---- the Profile page ----
        private (string Id, string Name, string Avatar) steamSelf = ("", "", "");

        private void LoadSteamSelf()
        {
            var me = SteamIdentity.Read(RealSetupHost.Instance.SteamRootFromRegistry());
            if (me is not null) steamSelf = (me.Id, me.Name, me.Avatar);
        }

        private async Task SetProfileSwitchAsync(string which, bool on)
        {
            switch (which)
            {
                case "presence": settings.PresenceEnabled = on; break;
                case "notify": settings.NotifyFriends = on; break;
                case "update": settings.AutoUpdateMods = on; break;
            }
            WriteSettings("the profile page");
            await InvokeAsync(StateHasChanged);
        }
        // ---- the cards of Home that point at something to do ----
        private IReadOnlyList<HomeAttention> HomeAttentionItems()
        {
            var list = new List<HomeAttention>();
            if (updateInfo is { } up && !updateBusy)
                list.Add(new($"Cairn {up.Tag} is out.", "Update", RunUpdateAsync));
            foreach (var g in GameProfileRegistry.All().Where(g => g.ModRepo.Length > 0 && IsModDownloaded(g)))
            {
                string id = g.Id;
                var offered = CatalogPackOf(g)?.Version;
                var have = InstalledPackOf(g)?.Version;
                if (BlockedReasonOfInstalled(g) is { } why)
                    list.Add(new($"{g.DisplayName}: this mod version has a known problem. {why}", "Update", () => DownloadPackAsync(id)));
                else if (HomeLogic.ModStateOf(offered, have, true) == ModState.UpdateAvailable)
                    list.Add(new($"{g.DisplayName} mod {have} -> {offered}", "Update", () => DownloadPackAsync(id)));
                else if (IsGameFound(g) && !IsModInstalled(g))
                    list.Add(new($"{g.DisplayName} mod is downloaded but not in the game.", "Install", () => InstallModAsync(id)));
            }
            if (setupChecked && !SystemSetupGood)
                list.Add(new("Something is missing on this PC (Steam, or a program the games need).", "Open setup", () => { _ = OpenSetupChecklist(); return Task.CompletedTask; }));
            return list;
        }

        /// <summary>Games on this PC that Cairn has a mod for and the player has none of yet (three at most).</summary>
        private IReadOnlyList<GameProfile> HomeSuggestions() =>
            GameProfileRegistry.All().Where(g => g.IsSupported && g.ModRepo.Length > 0 && IsGameFound(g) && !IsModDownloaded(g) && packBusyId != g.Id)
                .OrderBy(g => g.DisplayName, StringComparer.OrdinalIgnoreCase).Take(3).ToList();

        private string HomeMostPlayed() =>
            recent.Entries.OrderByDescending(e => e.Minutes).ThenByDescending(e => e.Sessions).Select(e => GameProfileRegistry.Get(e.GameId)?.DisplayName).FirstOrDefault(n => n is not null) ?? "-";    }
}
