// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// New Game without the prologue -- Start Game / Join Game on the main menu of a game this launcher
    /// started.
    ///
    /// Before the game starts: the first-run pages are checked (read only, never written); a host's bundled start save
    /// is staged into a free playline with a new playthrough seed (the engine lists saves once, at its start, and the
    /// host has no plugin at the menu to rescan); the player's own worlds are listed by the agent's helper (the continuity
    /// / beta patch classifier).
    ///
    /// While the game sits at its main menu: the menu is armed and redrawn through the game's own console (four times a
    /// second; no Lua timer runs at the main menu), the menu's choices are read from kcd.log, and each is carried out
    /// with the calls that already exist -- a load from the menu (join flow, after the menu's video is stopped: beta hardening),
    /// the joiner's CONNECT once its host is ready (the beta hardening gate) and the first-join answer (continuity's /join-choice).
    /// When the game exits, a start save that was never played is taken away again.
    /// </summary>
    public partial class Home
    {
        private volatile bool startGameTakeover;                 // this launch's menu is ours (cleared when the launch is reset)
        private string startGameJoinChoice = "";                 // "fresh" / "bring" once the joiner pressed Join
        private string startGameJoinState = "idle";              // idle | waiting | joining (what the joiner's menu shows)
        private StartGameSavesData? startGameSaves;
        private string startGameNewWhy = "";
        private readonly ConcurrentQueue<MenuTakeoverRule.MenuChoice> startGameChoices = new();
        private string? startGameStartRoot;                        // the start saves: one folder per playstyle
        private List<string> startGameStyles = new();              // the playstyles whose start save is installed

        private const int StartGameTickMs = 250;
        private const int StartGameStopVideoSettleMs = 1500;   // the agent's own settle after wh_ui_StopMovie (GameBridge.BetaJoin)

        public sealed class StartGameWorld { public int Pl { get; set; } public string Name { get; set; } = ""; public string Label { get; set; } = ""; }
        public sealed class StartGameStagedData { public int Pl { get; set; } public string Name { get; set; } = ""; public string Path { get; set; } = ""; public string SeedTag { get; set; } = ""; }
        public sealed class StartGameSavesData
        {
            public bool Ok { get; set; }
            public List<StartGameWorld> Usable { get; set; } = new();
            public string Hidden { get; set; } = "";
            public int HiddenRegular { get; set; }
            public int HiddenCopies { get; set; }
            public List<int> Free { get; set; } = new();
            public StartGameStagedData? Staged { get; set; }
        }
        public sealed class StartGameStageData { public bool Ok { get; set; } public int Pl { get; set; } public string Name { get; set; } = ""; public string SeedTag { get; set; } = ""; public string Why { get; set; } = ""; }
        public sealed class StartGameUnstageData { public bool Ok { get; set; } public int Removed { get; set; } public int Kept { get; set; } public List<string> Lines { get; set; } = new(); }

        /// <summary>
        /// Before the game starts. False = the launch stops here (the first-run pages are not accepted: the game would show
        /// them first, under a menu this launcher redraws).
        /// </summary>
        private async Task<bool> StartGameBeforeLaunchAsync(bool hosting)
        {
            var consent = await Task.Run(() => ConsentFlags.Read(ConsentFlags.UserFolder()));
            Log.Information("MP-StartGame first-run pages: {State} ({Detail})", consent.State, consent.Detail);
            if (consent.State is ConsentFlags.State.Missing or ConsentFlags.State.Stale)
            {
                UiService.ShowError(ConsentFlags.AcceptMessage);
                return false;
            }
            startGameJoinChoice = "";
            startGameJoinState = "idle";
            startGameSaves = null;
            startGameNewWhy = "";
            while (startGameChoices.TryDequeue(out _)) { }
            if (!settings.MenuTakeover) return true;

            string agent = AgentExe();
            startGameStartRoot = Environment.GetEnvironmentVariable("CAIRN_START_SAVE") is { Length: > 0 } envRoot ? envRoot : ResolveAgainstLauncher(settings.StartSavePath);
            startGameStyles = MenuTakeoverRule.Styles.Where(st => Directory.Exists(Path.Combine(startGameStartRoot, st))
                && Directory.GetFiles(Path.Combine(startGameStartRoot, st), "*.whs").Length == 1).ToList();
            Log.Information("MP-StartGame start saves installed: {Styles}", startGameStyles.Count == 0 ? "none" : string.Join(", ", startGameStyles));
            // a start save an earlier run staged and never played (the launcher closed with its game still open)
            if (await AgentHelper.RunAsync<StartGameUnstageData>(agent, "--start_game unstage", "StartGame", TimeSpan.FromSeconds(20)) is { } u && u.Lines.Count > 0)
                Log.Information("MP-StartGame earlier start saves: {Lines}", string.Join("; ", u.Lines));
            if (hosting)
            {
                var st = await AgentHelper.RunAsync<StartGameStageData>(agent, $"--start_game stage --source \"{startGameStartRoot}\"", "StartGame", TimeSpan.FromSeconds(30));
                if (st is { Ok: true }) Log.Information("MP-StartGame start save staged: playline{Pl}/{Name} (new world {Tag})", st.Pl, st.Name, st.SeedTag);
                else
                {
                    startGameNewWhy = st is null ? "The start save could not be prepared (see the launcher log)." : PlainNewWhy(st.Why);
                    Log.Information("MP-StartGame no new adventure this launch: {Why}", st?.Why ?? "the helper did not answer");
                }
            }
            StartGamePlaceLogo();
            startGameSaves = await AgentHelper.RunAsync<StartGameSavesData>(agent, "--start_game saves", "StartGame", TimeSpan.FromSeconds(60));
            Log.Information("MP-StartGame worlds: {N} usable, staged={Staged}, hidden=\"{Hidden}\"", startGameSaves?.Usable.Count ?? -1,
                startGameSaves?.Staged is { } s ? $"playline{s.Pl}/{s.Name}" : "none", startGameSaves?.Hidden ?? "");
            return true;
        }

        private static string PlainNewWhy(string why) =>
            why.StartsWith("All five", StringComparison.Ordinal) ? why
            : why.StartsWith("no start save", StringComparison.Ordinal) ? "This installation has no start saves (they come with the installer)."
            : "The start save could not be prepared: " + why;

        /// <summary>From LaunchGame, once the game process is up.</summary>
        private void StartGameStartDriver(Process game, bool hosting, DateTime gameStartLocal)
        {
            StartInGameMenu(game, gameStartLocal);
            startGameTakeover = settings.MenuTakeover;
            if (!startGameTakeover) { Log.Information("MP-StartGame MenuTakeover off: the game's own menu"); return; }
            _ = Task.Run(() => StartGameDriverAsync(game, hosting, gameStartLocal));
        }

        /// <summary>From ResetLaunchState: the menu is the game's again (the exit watch still takes back an unplayed start save).</summary>
        private void StartGameOnLaunchReset() => startGameTakeover = false;

        private string StartGameModel(bool hosting)
        {
            var s = startGameSaves;
            var worlds = (s?.Usable ?? new()).Select(w => new MenuTakeoverRule.WorldEntry(w.Pl, w.Name, w.Label)).ToList();
            var adv = s?.Staged is { } st && st.Pl is >= 0 and <= 4 ? new MenuTakeoverRule.WorldEntry(st.Pl, st.Name, "") : null;
            string why = adv is not null ? "" : startGameNewWhy.Length > 0 ? startGameNewWhy : "No new adventure is available.";
            var (bring, msg) = MenuTakeoverRule.JoinChoice(s?.Usable.Count ?? 0, s?.HiddenRegular ?? 0, s?.HiddenCopies ?? 0);
            return MenuTakeoverRule.ModelCall(hosting, worlds, s?.Hidden ?? "", adv, why, startGameJoinState, bring, msg, startGameStyles,
                startGameJoinState == "joining" ? joinStatusMessage ?? "" : "");
        }

        private async Task StartGameDriverAsync(Process game, bool hosting, DateTime gameStartLocal)
        {
            using var console = new GameConsole();
            var clock = Stopwatch.StartNew();
            KcdLogFollower? follower = null;
            double nextFind = 0, nextBeat = 0;
            long lastSeq = -1;
            string pushed = "";
            bool loadIssued = false, armedLogged = false, choicePosted = false, menuLoad = false, hostConnectTried = false;
            bool recapPending = false, recapSent = false;
            GameStage lastStage = GameStage.Starting;
            DateTime connectedAt = DateTime.MinValue;
            Log.Information("MP-StartGame menu driver on ({Role})", hosting ? "host" : "joiner");
            try
            {
                while (!game.HasExited)
                {
                    double now = clock.Elapsed.TotalSeconds;
                    if (follower is null && now >= nextFind)
                    {
                        if (await Task.Run(() => LogBundle.FindKcdLog(LogBundleGameRoot)) is string path)
                        {
                            follower = new KcdLogFollower(path, gameStartLocal);
                            follower.OnLine = line =>
                            {
                                if (MenuTakeoverRule.Parse(line) is { } c) { Log.Information("MP-StartGame menu line read: {Kind} #{Seq}", c.Kind, c.Seq); startGameChoices.Enqueue(c); }
                            };
                            Log.Information("MP-StartGame reading the game's log: {Path}", path);
                        }
                        else nextFind = now + 5;
                    }
                    follower?.Poll(now);
                    var stage = follower?.State.Stage ?? GameStage.Starting;
                    if (now >= nextBeat) { nextBeat = now + 30; Log.Information("MP-StartGame driver alive: stage={Stage} takeover={Takeover} loadIssued={Load} follower={Follower}", stage, startGameTakeover, loadIssued, follower is not null); }
                    if (stage != lastStage)
                    {
                        if (stage == GameStage.Menu) { loadIssued = false; pushed = ""; }   // back at the menu (a failed load too): draw again
                        lastStage = stage;
                    }

                    while (startGameChoices.TryDequeue(out var c))
                    {
                        if (c.Seq <= lastSeq) continue;   // a line read twice
                        lastSeq = c.Seq;
                        if (!startGameTakeover) { Log.Information("MP-StartGame a menu choice ({Kind}) was read, but this launch's menu is the game's own again (the launch was cancelled or reset): not acted on", c.Kind); continue; }
                        if (c.Kind is "load" or "newadv" && stage != GameStage.Menu)
                        {
                            Log.Warning("MP-StartGame a menu choice ({Kind}) read while the game is not at its menu ({Stage}): not acted on", c.Kind, stage);
                            continue;
                        }
                        Log.Information("MP-StartGame the menu chose {Kind} {Pl} {Name}{Join}", c.Kind, c.Playline, c.Name, c.Join);
                        switch (c.Kind)
                        {
                            case "load" when hosting && !loadIssued:
                                loadIssued = await StartGameLoadAsync(console, c.Playline, c.Name, false);
                                menuLoad |= loadIssued;
                                break;
                            case "newadv" when hosting && !loadIssued && startGameSaves?.Staged is { } st:
                            {
                                // the chosen playstyle's start save into the staged slot, then the load
                                var sw = await AgentHelper.RunAsync<StartGameStageData>(AgentExe(),
                                    $"--start_game swap --path \"{st.Path}\" --source \"{startGameStartRoot}\" --style {c.Style}", "StartGame", TimeSpan.FromSeconds(30));
                                if (sw is not { Ok: true })
                                {
                                    Log.Warning("MP-StartGame the {Style} start save could not be put in place: {Why}", c.Style, sw?.Why ?? "the helper did not answer");
                                    await console.LuaAsync(MenuTakeoverRule.HudLine("The new adventure could not be prepared. Switch to the launcher window for details."));
                                    break;
                                }
                                Log.Information("MP-StartGame new adventure: {Style}, a new world {Tag}; prologue {How}", c.Style, sw.SeedTag, c.Watch ? "watched" : "skipped");
                                loadIssued = await StartGameLoadAsync(console, st.Pl, st.Name, true);
                                menuLoad |= loadIssued;
                                recapPending = loadIssued && c.Watch;
                                break;
                            }
                            case "recap":
                                Log.Information("MP-StartGame the prologue's cutscenes {How}", c.Name);
                                break;
                            case "join" when !hosting:
                                startGameJoinChoice = c.Join == "fresh" ? "fresh:" + c.Style : "bring";
                                startGameJoinState = "waiting";
                                break;
                            case "cancel" when !hosting && startGameJoinState == "waiting":
                                startGameJoinChoice = "";
                                startGameJoinState = "idle";
                                break;
                        }
                    }

                    // "Watch the prologue": its rendered cutscenes start once the New adventure's world is there
                    if (recapPending && !recapSent && stage == GameStage.World && follower is not null
                        && now - follower.State.WorldSinceS >= 2)
                    {
                        recapSent = await console.LuaAsync(MenuTakeoverRule.RecapCall());
                        Log.Information("MP-StartGame the prologue's cutscenes {State} ({Min} min, skipping one skips them all)", recapSent ? "started" : "NOT started", MenuTakeoverRule.RecapMinutes);
                    }

                    // the host: CONNECT pressed by the launcher once the world Start Game loaded has settled (no Alt+Tab)
                    if (startGameTakeover && !hostConnectTried && MenuTakeoverRule.HostAutoConnect(hosting, menuLoad, stage,
                            follower?.State.WorldSinceS ?? double.NaN, now, launchStage == LaunchStage.WaitingForConnect, ConnectEnabled))
                    {
                        hostConnectTried = true;
                        Log.Information("MP-StartGame the world Start Game loaded has settled: CONNECT for the host");
                        await InvokeAsync(ConnectToGame);
                        if (launchStage != LaunchStage.Connected) await console.LuaAsync(MenuTakeoverRule.HudLine(MenuTakeoverRule.ConnectFailedText));
                    }

                    // the joiner: CONNECT as soon as the host is ready (the beta hardening gate), then the first-join answer
                    if (!hosting && startGameTakeover && startGameJoinState == "waiting" && launchStage == LaunchStage.WaitingForConnect && ConnectEnabled)
                    {
                        startGameJoinState = "joining";
                        Log.Information("MP-StartGame the host is ready: CONNECT for the menu's Join ({Choice})", startGameJoinChoice);
                        await InvokeAsync(ConnectToGame);
                        if (launchStage != LaunchStage.Connected) await console.LuaAsync(MenuTakeoverRule.HudLine(MenuTakeoverRule.ConnectFailedText));
                        connectedAt = DateTime.UtcNow;
                    }
                    if (!hosting && startGameJoinState == "joining" && !choicePosted && launchStage == LaunchStage.Connected && startGameJoinChoice.Length > 0)
                    {
                        if (await NetService.PostJoinChoiceAsync(settings.VersionIpcPort, startGameJoinChoice))
                        {
                            choicePosted = true;
                            Log.Information("MP-StartGame first-join answer sent to the agent: {Choice}", startGameJoinChoice);
                        }
                        else if ((DateTime.UtcNow - connectedAt).TotalSeconds > 90)
                        {
                            choicePosted = true;
                            Log.Warning("MP-StartGame the agent did not take the first-join answer in 90 s; the launcher's own buttons ask again");
                        }
                    }
                    if (!hosting && startGameJoinState == "joining" && launchStage is LaunchStage.Idle or LaunchStage.Failed)
                        startGameJoinState = "idle";   // the connect did not happen (refused by Windows, reset): the choice page again

                    if (startGameTakeover && stage == GameStage.Menu && !loadIssued)
                    {
                        string model = StartGameModel(hosting);
                        if (model != pushed && await console.LuaAsync(model)) pushed = model;
                        if (await console.LuaAsync(MenuTakeoverRule.TickCall) && !armedLogged)
                        {
                            armedLogged = true;
                            Log.Information("MP-StartGame menu armed: Start Game / Join Game");
                        }
                    }
                    await Task.Delay(StartGameTickMs);
                }
            }
            catch (Exception ex) { Log.Warning("MP-StartGame menu driver stopped: {Kind}: {Msg}", ex.GetType().Name, ex.Message); }
            try { await game.WaitForExitAsync(); } catch { }
            Log.Information("MP-StartGame the game exited");
            await StartGameRemoveLogoAsync();
            string agent = AgentExe();
            if (await AgentHelper.RunAsync<StartGameUnstageData>(agent, "--start_game unstage", "StartGame", TimeSpan.FromSeconds(20)) is { } u && u.Lines.Count > 0)
                Log.Information("MP-StartGame start saves after the game: {Lines}", string.Join("; ", u.Lines));
        }

        // ------------------------------------------------------------ our logo on the main menu (launcher-started only)
        //
        // The game draws its menu logo from Libs/UI/Textures/KCDLogo.dds. The mod's brand pak (part of the pack; the contract names it) carries ours under that name. It is put into the mod's
        // Data folder only for a game this launcher starts and taken out when that game exits, so a game started any other
        // way shows the game's own logo. A pak left behind by a launcher that was killed is removed by the next launch's
        // exit, or by Setup (it prunes what is not the mod's).
        private string BrandPak => Path.GetFileName(BrandPakFile()) is { Length: > 0 } n ? n : "brand.pak";

        private string? StartGameBrandTarget() =>
            string.IsNullOrEmpty(settings.GamePath) ? null : Path.Combine(ModDirFor(settings.GamePath), "Data", BrandPak);

        private void StartGamePlaceLogo()
        {
            string src = BrandPakFile();
            if (!File.Exists(src) || StartGameBrandTarget() is not string dst) { Log.Information("MP-StartGame no menu logo pak beside the launcher: the game's own logo"); return; }
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(dst)!);
                File.Copy(src, dst, overwrite: true);
                Log.Information("MP-StartGame our logo placed for this game ({Pak})", BrandPak);
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { Log.Warning("MP-StartGame our logo could not be placed: {Kind}", ex.GetType().Name); }
        }

        private async Task StartGameRemoveLogoAsync()
        {
            if (StartGameBrandTarget() is not string dst || !File.Exists(dst)) return;
            // the game can hold the pak for a moment after its process ended (0.45.6: IOException at the exit)
            for (int attempt = 1; ; attempt++)
            {
                try { File.Delete(dst); Log.Information("MP-StartGame our logo taken out again (the game's own for a normal start)"); return; }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                {
                    if (attempt >= 15) { Log.Warning("MP-StartGame our logo could not be taken out: {Kind} (the next launch's exit or Setup removes it)", ex.GetType().Name); return; }
                    await Task.Delay(1000);
                }
            }
        }

        /// <summary>The menu's load: the menu's video stopped first (beta hardening's freeze), then the join flow load.</summary>
        private async Task<bool> StartGameLoadAsync(GameConsole console, int pl, string name, bool newAdventure)
        {
            if (!MenuTakeoverRule.IsSaveName(name) || pl is < 0 or > 4) return false;
            await console.LuaAsync(ModLua.Call(ModLua.StartGameDisarm, "\"load\""));
            await console.CommandAsync("wh_ui_StopMovie");
            await Task.Delay(StartGameStopVideoSettleMs);
            bool ok = await console.LuaAsync(MenuTakeoverRule.LoadCall(pl, name));
            Log.Information("MP-StartGame {What}: wh_sys_LoadGame {Pl} {Name} -> {Ok}", newAdventure ? "new adventure" : "load", pl, name, ok ? "sent" : "NOT sent");
            if (ok && newAdventure && startGameSaves?.Staged is { } st)
            {
                // a normal save from now on: never taken back, even before its first save
                string agent = AgentExe();
                await AgentHelper.RunAsync<StartGameStageData>(agent, $"--start_game used --path \"{st.Path}\"", "StartGame", TimeSpan.FromSeconds(20));
            }
            return ok;
        }
    }
}
