// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Threading;
using System.Threading.Tasks;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// Game mods are separate downloads: the installer carries only the launcher, and each game's mod is a pack published with
    /// the releases of the game's own mod repository (see Cairn.Setup/Packs.cs), updated on its own. The game page shows the pack's state; the sidebar can filter by it.
    /// </summary>
    public partial class Home
    {
        private static readonly HttpClient PackHttp = CreatePackHttp();
        private static HttpClient CreatePackHttp()
        {
            var http = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("Cairn-launcher");
            return http;
        }

        // The catalog of downloadable mods (each game's latest mod release); null until it has been fetched (or when it cannot be).
        private PackIndex? packIndex;
        private bool packIndexLoading;
        private bool packIndexTried;
        private DateTime? packIndexCheckedUtc;   // when the catalog was last read (for the footer)

        // The downloads: one queue, shown in the footer; the game's page and Home read what is running from it.
        private DownloadManager? downloadsInstance;
        private DownloadManager Downloads => downloadsInstance ??= new DownloadManager(RunPackDownloadAsync);
        private string? packBusyId => Downloads.Active?.GameId;
        private double packProgressValue => Downloads.Active?.Progress ?? 0;
        private string packMessage = "";
        private bool packMessageIsError;
        // Reading a pack's manifest per game per render is wasteful: asked once, until a download or removal changes it.
        private readonly Dictionary<string, InstalledPack?> installedPackCache = new(StringComparer.OrdinalIgnoreCase);

        private string PacksRoot => PackStore.DefaultRoot;

        private InstalledPack? InstalledPackOf(GameProfile g)
        {
            if (!installedPackCache.TryGetValue(g.Id, out var p))
                installedPackCache[g.Id] = p = PackStore.Read(PacksRoot, g.Id);
            return p;
        }

        /// <summary>The mod pack is on this PC (verified when it was downloaded).</summary>
        private bool IsModDownloaded(GameProfile g) => InstalledPackOf(g) is not null;

        /// <summary>
        /// The mod is really usable: downloaded, and (for a game with a mod folder) placed in it. Checked against the files in the
        /// game's Mods folder, so it is true for a development build too, which has no install manifest to consult.
        /// </summary>
        private bool IsModInstalled(GameProfile g)
        {
            if (!IsModDownloaded(g)) return false;
            if (IsExternalMod(g)) return GameFolderOf(g) is { } folder && ExternalInstall.IsPlaced(folder, PacksRoot, g.Id);
            if (!IsPackRunGame(g.Id)) return true;
            if (packSetupGameId != g.Id) return true;   // the pack's setup is asked for the game that is selected
            return packSetup?.Steps.FirstOrDefault(s => s.Id == "mod")?.Status == PackStepStatus.Done;   // the pack's own word: its mod is in place
        }

        /// <summary>Puts a downloaded mod into the game's Mods folder (the setup checklist does the same on its own once everything else is ready).</summary>
        private async Task InstallModAsync(string gameId)
        {
            if (GameProfileRegistry.Get(gameId) is not { } game) return;
            if (IsExternalMod(game)) { await InstallExternalModAsync(game); return; }
            if (!IsPackRunGame(gameId)) return;
            if (gameRunning) { SetPackMessage("Close the game first.", error: true); return; }
            if (PackSetupProgram() is null) { SetPackMessage(ModTooOldText, error: true); return; }
            await RunPackActionAsync("mod", "place-mod");   // the pack's program puts its mod in the Modding Tools
            await InvokeAsync(StateHasChanged);
        }

        private PackInfo? CatalogPackOf(GameProfile g) => packIndex?.ForGame(g.Id);

        // What the catalog offers for a pack that is not a game's own mod (what a mod depends on, content): asked when needed, until the catalog is read again.
        private readonly Dictionary<string, PackInfo?> offeredPacks = new(StringComparer.OrdinalIgnoreCase);

        // A newest mod version the catalog says not to offer: game id -> "version: why". The game's page says so instead of offering an update.
        private readonly Dictionary<string, string> blockedOffers = new(StringComparer.OrdinalIgnoreCase);

        private static PackInfo? ModOf(PackIndex? index) => index?.Packs.FirstOrDefault(p => PackKinds.IsGameMod(p.Kind));

        private async Task LoadPackIndexAsync()
        {
            if (packIndexLoading) return;
            packIndexLoading = true;
            try
            {
                // every game that has a mod repository has its own latest release; the catalog is what they say together
                var found = new List<PackInfo>();
                var blocked = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
                bool anyRepo = false;
                foreach (var g in GameProfileRegistry.All().Where(g => g.ModRepo.Length > 0))
                {
                    anyRepo = true;
                    string channel = ChannelOf(g.Id);
                    var stable = ModOf(await PackCatalog.FetchAsync(PackHttp, g.ModRepo));
                    var beta = channel == PackChannels.Beta ? ModOf(await PackChannels.FetchBetaAsync(PackHttp, g.ModRepo)) : null;
                    var chosen = PackChannels.Choose(channel, stable, beta);
                    if (chosen is not null && GameProfileRegistry.Catalog.BlockOf(chosen.Id, chosen.Version) is { } block)
                    {
                        blocked[g.Id] = $"{chosen.Version}: {block.Reason}";
                        // a blocked beta falls back to the stable release; a blocked stable release is simply not offered
                        chosen = ReferenceEquals(chosen, beta) && stable is not null && GameProfileRegistry.Catalog.BlockOf(stable.Id, stable.Version) is null ? stable : null;
                    }
                    if (chosen is not null) found.Add(chosen);
                }
                blockedOffers.Clear();
                foreach (var b in blocked) blockedOffers[b.Key] = b.Value;
                offeredPacks.Clear();
                packIndex = anyRepo && found.Count > 0 ? new PackIndex(PackIndex.CurrentSchema, found) : null;
                if (packIndex is not null) packIndexCheckedUtc = DateTime.UtcNow;
                Log.Information("packs: catalog {Result}", packIndex is null ? "not available" : $"{packIndex.Packs.Count} pack(s)");
            }
            finally
            {
                packIndexLoading = false;
                packIndexTried = true;
                await InvokeAsync(StateHasChanged);
            }
            foreach (var g in GameProfileRegistry.All().Where(g => g.ContentPacks.Count > 0 && IsModDownloaded(g))) _ = EnsureContentAsync(g);
        }

        /// <summary>What the catalog offers for a pack: a game's mod (its channel's release), or any other pack from the repository the games catalog names for it. Null when nothing.</summary>
        private async Task<PackInfo?> OfferedPackAsync(string packId)
        {
            if (GameProfileRegistry.Get(packId) is { ModRepo.Length: > 0 } g) return CatalogPackOf(g);
            if (offeredPacks.TryGetValue(packId, out var cached)) return cached;
            var repo = GameProfileRegistry.Catalog.RepoOfPack(packId);
            var info = repo is null ? null : (await PackCatalog.FetchAsync(PackHttp, repo))?.Packs.FirstOrDefault(p => string.Equals(p.Id, packId, StringComparison.OrdinalIgnoreCase));
            if (info is not null && GameProfileRegistry.Catalog.BlockOf(info.Id, info.Version) is not null) info = null;
            return offeredPacks[packId] = info;
        }

        // the catalog's answer for everything the pack depends on, the dependencies' own included (the planner works on what is known)
        private async Task PrefetchDependenciesAsync(PackInfo pack, Dictionary<string, PackInfo> offered, int depth)
        {
            if (depth > 4) return;
            foreach (var dep in pack.Dependencies)
            {
                if (offered.ContainsKey(dep.Id)) continue;
                if (await OfferedPackAsync(dep.Id) is not { } o) continue;
                offered[dep.Id] = o;
                await PrefetchDependenciesAsync(o, offered, depth + 1);
            }
        }

        /// <summary>Queues a pack's download (a game's mod, a first one or an update; or content) and returns when it is done. What it depends on is downloaded first.</summary>
        private async Task DownloadPackAsync(string packId)
        {
            var game = GameProfileRegistry.Get(packId);   // null for a pack that is not a game's mod
            string name = game?.DisplayName ?? packId;
            if (Downloads.ItemOf(packId) is { } queued) { await queued.Finished; return; }
            if (!packIndexTried || packIndex is null) await LoadPackIndexAsync();
            var info = await OfferedPackAsync(packId);
            var url = info is null ? null : PackCatalog.PackUrl(GameProfileRegistry.Catalog.RepoOfPack(packId), info);
            if (info is null || url is null)
            {
                SetPackMessage(blockedOffers.TryGetValue(packId, out var why)
                    ? $"The newest {name} mod ({why}) is on hold, so it is not offered."
                    : "No mod is available for this game yet.", error: true);
                return;
            }
            if (!PackCatalog.LauncherIsEnough(info.LauncherMin, Globals.CairnVersion))
            {
                SetPackMessage($"This mod needs Cairn {info.LauncherMin} or newer. Update the launcher first.", error: true);
                return;
            }

            var known = new Dictionary<string, PackInfo>(StringComparer.OrdinalIgnoreCase);
            await PrefetchDependenciesAsync(info, known, 0);
            var plan = PackPlanner.Plan(info, id => PackStore.Read(PacksRoot, id), id => known.GetValueOrDefault(id));
            if (!plan.Ok)
            {
                SetPackMessage($"Could not download {name}: {plan.Problem}.", error: true);
                return;
            }

            SetPackMessage("", error: false);
            foreach (var dep in plan.Order.Where(p => !string.Equals(p.Id, info.Id, StringComparison.OrdinalIgnoreCase)))
            {
                var depUrl = PackCatalog.PackUrl(GameProfileRegistry.Catalog.RepoOfPack(dep.Id), dep);
                bool ok = depUrl is not null
                    && await Downloads.Enqueue(new DownloadItem(dep.Id, dep.Id, PackStore.Read(PacksRoot, dep.Id)?.Version, dep, depUrl)).Finished;
                if (!ok) { SetPackMessage($"Could not download {name}: {dep.Id} did not download.", error: true); return; }
            }
            await Downloads.Enqueue(new DownloadItem(packId, name, PackStore.Read(PacksRoot, packId)?.Version, info, url)).Finished;
        }

        /// <summary>A game's content packs (its start saves): downloaded when the player has none yet or the catalog has a newer one.</summary>
        private async Task EnsureContentAsync(GameProfile game)
        {
            foreach (var id in game.ContentPacks)
            {
                var info = await OfferedPackAsync(id);
                if (info is null || info.Kind != PackKinds.Content) continue;
                if (PackStore.Read(PacksRoot, id) is { } have
                    && !(Version.TryParse(info.Version, out var offered) && Version.TryParse(have.Version, out var mine) && offered > mine)) continue;
                _ = DownloadPackAsync(id);
            }
        }

        /// <summary>The queue's turn for one pack: download it, then (for the setup mod) put it in the game, or (for content) put its files where the game looks. Runs off the UI thread.</summary>
        private async Task<(bool Ok, string Message)> RunPackDownloadAsync(DownloadItem item, IProgress<double> progress, CancellationToken ct)
        {
            string gameId = item.GameId, name = item.GameName;
            bool isSetupMod = gameId == ActiveGameId && item.Info.Kind == PackKinds.Mod;
            if (isSetupMod && !gameRunning) await InvokeAsync(ReleasePackSession);   // its program holds the mod's files open
            if (isSetupMod) await InvokeAsync(() => SetPackDownloadActivity(true, 0, null));
            await InvokeAsync(StateHasChanged);

            var sw = Stopwatch.StartNew();
            long lastTick = 0;
            var relay = new Progress<double>(p =>
            {
                progress.Report(p);
                if (!isSetupMod || (sw.ElapsedMilliseconds - lastTick < 150 && p < 1)) return;   // the checklist repaints a few times a second, not per chunk
                lastTick = sw.ElapsedMilliseconds;
                _ = InvokeAsync(() => SetPackDownloadActivity(true, p, null));
            });

            PackResult result;
            try
            {
                result = await PackInstaller.DownloadAsync(item.Info, item.Url, PacksRoot, PackHttp, relay, ct);
            }
            catch (OperationCanceledException)
            {
                Log.Information("packs: {Pack} {Version} cancelled", gameId, item.Info.Version);
                await InvokeAsync(() =>
                {
                    if (isSetupMod) SetPackDownloadActivity(false, null, null);
                    SetPackMessage($"{name} download cancelled.", error: false);
                    StateHasChanged();
                });
                throw;
            }
            Log.Information("packs: {Pack} {Version} {Outcome}: {Message}", gameId, item.Info.Version, result.Ok ? "downloaded" : "FAILED", result.Message);

            string kindWord = item.Info.Kind == PackKinds.Content ? "files" : "mod";
            await InvokeAsync(() =>
            {
                installedPackCache.Remove(gameId);
                ModLua.Reload();
                if (isSetupMod)
                    SetPackDownloadActivity(false, null, result.Ok ? null : $"The mod could not be downloaded ({result.Message}).");
                SetPackMessage(result.Ok ? $"{name} {kindWord} downloaded." : $"Could not download the {kindWord}: {result.Message}.", error: !result.Ok);
                StateHasChanged();
            });
            if (result.Ok && item.Info.Kind == PackKinds.Content && PackStore.ReadContract(PacksRoot, item.Info.Id) is { InstallTo: PackContract.StartSaveTarget })
            {
                // content the player did not have yet: copied where the game looks; what the player already has is never overwritten
                int copied = await Task.Run(() => ContentInstall.CopyMissing(Path.Combine(PackStore.FilesDir(PacksRoot, item.Info.Id), PackStore.ModFolder), StartSaveRoot));
                Log.Information("packs: {Pack} put {Count} file(s) into the start saves", gameId, copied);
            }
            if (result.Ok && item.Info.Kind == PackKinds.External)
            {
                // an external mod goes into the game's own folder as soon as it is downloaded (the Install button stays as the fallback)
                Downloads.MarkInstalling(item);
                await InvokeAsync(() => InstallModAsync(gameId));
            }
            if (result.Ok && isSetupMod)
            {
                // downloading installs it too: placed in the game right away (the Install button stays as the fallback if that fails)
                Downloads.MarkInstalling(item);
                await InvokeAsync(() => InstallModAsync(gameId));
                if (GameProfileRegistry.Get(gameId) is { } installed) _ = EnsureContentAsync(installed);
            }
            return (result.Ok, result.Message);
        }
        private async Task RemovePackAsync(string gameId)
        {
            if (packBusyId is not null || GameProfileRegistry.Get(gameId) is not { } game) return;
            if (gameRunning)
            {
                SetPackMessage("Close the game first.", error: true);
                return;
            }
            string? note = null;
            if (IsExternalMod(game))
            {
                if (GameFolderOf(game) is { } folder)
                {
                    var r = await Task.Run(() => ExternalInstall.Unplace(folder, PacksRoot, gameId));
                    if (!r.Ok) { SetPackMessage($"Could not remove the mod from the game ({r.Message}).", error: true); return; }
                    note = r.Message;
                }
            }
            else if (PackSetupProgram(gameId) is { } program)
            {
                // the pack's own program takes its mod out of the Modding Tools (an older pack without the action leaves it, and says so)
                var r = await PackSetup.DoAsync(program, "unplace-mod", GameExeOf(gameId), _ => { }, _ => { });
                if (!r.Ok && r.Message.Length > 0) { SetPackMessage($"Could not remove the mod from the game ({r.Message}).", error: true); return; }
                note = r.Message;
            }
            if (gameId == packSessionGameId) ReleasePackSession();
            await Task.Run(() => PackStore.Remove(PacksRoot, gameId));
            installedPackCache.Remove(gameId);
            ModLua.Reload();
            Log.Information("packs: {Game} removed ({Note})", gameId, note ?? "download only");
            SetPackMessage($"{game.DisplayName} mod removed.", error: false);
            if (gameId == ActiveGameId) await CheckSetupAgainAsync();
        }

        private void SetPackMessage(string message, bool error)
        {
            packMessage = message;
            packMessageIsError = error;
            ShowToast(message, error);
        }
    }
}
