// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// A game's connection side run by its pack (docs/SESSION.md): when the downloaded pack says <c>"session": true</c>, Host and Join ask the pack's program to start the
    /// relay, the game, the plugin and the agent, and the launcher only shows what it reports. The state it sends back is copied into the fields the launch panel, the host
    /// window and the banners already read, so those screens are the same either way. A pack without it keeps the launcher's own code (Home.razor.cs, Home.Beta.cs,
    /// Home.ConnectPath.cs), used whenever <see cref="UsePackSession"/> is false.
    /// </summary>
    public partial class Home
    {
        private PackSession? packSession;
        private bool packRelayUp;
        private bool packRelayProblem;
        private TaskCompletionSource<bool>? relayWait, connectWait;
        private LaunchStage packPrevStage = LaunchStage.Idle;
        private string? lastRefusedKey;

        /// <summary>The program of the game's pack when the pack runs the session; null otherwise.</summary>
        private string? PackSessionProgram() => ProgramCached(ref sessionProgramCache, p => p is { Session: true });

        private (long At, string? Program) sessionProgramCache, setupProgramCache;

        /// <summary>The pack's program when its contract says so, looked up at most every 2 s (the screens ask on every repaint).</summary>
        private static string? ProgramCached(ref (long At, string? Program) cache, Func<PackContract, bool> wants)
        {
            long now = Environment.TickCount64;
            if (cache.At != 0 && now - cache.At < 2000) return cache.Program;
            string? found = null;
            var root = PackStore.DefaultRoot;
            if (PackStore.ReadContract(root, ModInstall.PackId) is { } c && wants(c) && ExternalInstall.OptionsProgramPath(root, ModInstall.PackId) is { } p && File.Exists(p)) found = p;
            cache = (now, found);
            return found;
        }

        private bool UsePackSession => PackSessionProgram() is not null;

        /// <summary>The host's relay is up: the pack's, or the launcher's own.</summary>
        private bool HostRelayRunning => UsePackSession ? packRelayUp : hostedRelayProcess != null && !hostedRelayProcess.HasExited;

        private PackSession? EnsurePackSession()
        {
            if (packSession is { Alive: true }) return packSession;
            packSession?.Dispose();
            packSession = null;
            if (PackSessionProgram() is not { } program) return null;
            var s = PackSession.Start(program, line => Log.Information("{Line}", Redact.Text(line)));
            if (s is null) return null;
            s.State += v => _ = InvokeAsync(() => ApplyPackView(v));
            s.Problem += p => _ = InvokeAsync(() => OnPackProblem(p));
            s.Error += t => _ = InvokeAsync(() => { UiService.ShowError(t); });
            s.Message += (title, text) => _ = InvokeAsync(() => ShowMessage(title, text, ""));
            s.Toast += t => _ = InvokeAsync(() => ShowToast(t));
            s.Game += g => _ = InvokeAsync(() => OnPackGame(g));
            s.GameExit += () => _ = InvokeAsync(OnPackGameExit);
            s.Refused += (text, key) => _ = InvokeAsync(() =>
            {
                if (key != lastRefusedKey && !showHostInfo) { lastRefusedKey = key; ShowMessage("VERSION MISMATCH", text, ""); }
                lastRefusedKey ??= key;
            });
            s.VersionMismatch += t => _ = InvokeAsync(() => { versionMismatchMessage = t; showVersionMismatch = true; StateHasChanged(); });
            s.Friends += f => _ = InvokeAsync(() => OnPackFriends(f));
            s.Exited += () => _ = InvokeAsync(OnPackSessionExited);
            packSession = s;
            Log.Information("session: the pack's program runs the connection side");
            return s;
        }

        /// <summary>The player's settings, sent before a command that uses them (the file is read again first: the game's menu may have changed one).</summary>
        private void ConfigurePackSession()
        {
            var taken = settingsStore.Refresh(settings);
            if (taken.Count > 0) Log.Information("settings: changed outside the launcher since it read them: {Keys}", string.Join(", ", taken));
            packSession?.Configure(settings.GamePath ?? "", settings.HostPort, settings.ServerInfoPort, settings.VersionIpcPort, settings.SteamAppId, settings.HostAllowSteam,
                settings.RelayReuse, settings.ConnectGate, settings.InjectDelaySeconds, settings.MenuTakeover,
                string.IsNullOrWhiteSpace(settings.AgentPath) ? "" : ResolveAgainstLauncher(settings.AgentPath),
                string.IsNullOrWhiteSpace(settings.RelayPath) ? "" : ResolveAgainstLauncher(settings.RelayPath),
                string.IsNullOrWhiteSpace(settings.DllPath) ? "" : ResolveAgainstLauncher(settings.DllPath),
                StartSaveRootPath());
        }

        // ------------------------------------------------------------ what the pack reports

        private void ApplyPackView(SessionView v)
        {
            var was = launchStage;
            launchStage = v.Stage switch
            {
                "waiting-connect" => LaunchStage.WaitingForConnect, "injecting" => LaunchStage.Injecting, "verifying" => LaunchStage.Verifying,
                "connected" => LaunchStage.Connected, "failed" => LaunchStage.Failed, _ => LaunchStage.Idle,
            };
            launchStatusMessage = v.Status;
            connectGateOpen = v.ConnectEnabled;
            connectGateReason = v.ConnectReason;
            connStatusLine = v.ConnText; connStatusBad = v.ConnBad;
            joinStatusMessage = v.JoinMessage; joinStatusState = v.JoinState; joinCanFresh = v.JoinCanFresh;
            hostErrorMessage = v.Host.Error; hostSteamState = v.Host.SteamState; hostSteamMessage = v.Host.SteamMessage; hostSteamCode = v.Host.SteamCode;
            hostRefusedNotice = v.Host.Refused;
            packRelayUp = v.Host.RelayUp;

            if (was != LaunchStage.Idle && launchStage == LaunchStage.Idle) StartGameOnLaunchReset();   // the game's own menu again
            if (relayWait is { } rw && (v.Host.RelayUp || v.Host.Error.Length > 0)) { relayWait = null; rw.TrySetResult(true); }
            // CONNECT is done when the launch reached Connected or Failed, or is back to waiting (Windows refused the agent), or was reset
            if (connectWait is { } cw && (launchStage is LaunchStage.Connected or LaunchStage.Failed or LaunchStage.Idle
                                          || (launchStage == LaunchStage.WaitingForConnect && packPrevStage is LaunchStage.Injecting or LaunchStage.Verifying)))
            { connectWait = null; cw.TrySetResult(true); }
            packPrevStage = launchStage;
            StateHasChanged();
        }

        private void OnPackProblem(SessionProblem p)
        {
            packRelayProblem = true;
            if (relayWait is { } rw) { relayWait = null; rw.TrySetResult(false); }
            ShowMessage(p.Title, p.Text, p.Next, addressFallback: p.AddressFallback, reportBug: p.ReportBug, launchAnyway: p.LaunchAnyway);
        }

        private void OnPackGame(SessionGame g)
        {
            Process? proc;
            try { proc = Process.GetProcessById(g.Pid); }
            catch (ArgumentException) { return; }
            pendingGameProcess = proc;
            launchedAs = g.Hosting ? "host" : "join";
            launchedGameId = selectedGameId;
            RecordSession(launchedAs);   // the Continue card on Home
            gameRunning = true;          // the buttons grey out at once, not at the next look
            PublishPresenceStatus();
            StartInGameMenu(proc, g.StartedLocal);        // the mod's menu, drawn over the game
            startGameTakeover = settings.MenuTakeover;     // the start-game flow itself is the pack's (the screens only need to know it is on)
            StateHasChanged();
        }

        private void OnPackGameExit()
        {
            gameRunning = false;
            launchedAs = "";
            launchedGameId = "";
            pendingGameProcess = null;
            PublishPresenceStatus();
            _ = Task.Run(CloseModdingTools);
            ReleasePackSession();   // the session is over: nothing of the mod's folder stays open, so an update of the mod can replace its files
            StateHasChanged();
        }

        /// <summary>Ends the pack's program (its relay goes with it). Called when the game is over, before the mod is updated or removed, and when the launcher closes.</summary>
        private void ReleasePackSession()
        {
            var s = packSession;
            packSession = null;
            packRelayUp = false;
            hostSteamState = "off"; hostSteamMessage = ""; hostSteamCode = null; hostRefusedNotice = "";
            if (s is null) return;
            try { s.RelayStop(); } catch (Exception ex) { Log.Debug(ex, "session: the relay could not be asked to stop"); }
            s.Dispose();
        }

        private void OnPackFriends(SessionFriends f)
        {
            steamBusy = false;
            if (f.State != "ok") steamFriendsNote = f.State == "no-answer" ? "Steam didn't answer. Type the host's code instead." : $"{f.Message} {f.Next}".Trim();
            else
            {
                steamFriends = f.Friends.Select(x => new SteamFriendData { Name = x.Name, Code = x.Code, Release = x.Release }).ToList();
                steamFriendsNote = f.Friends.Count == 0 ? "No friends playing" : "Click a friend to use their code.";
            }
            Log.Information("Steam friends lookup: state={State} hosting={Count}", f.State, f.Friends.Count);
            StateHasChanged();
        }

        private void OnPackSessionExited()
        {
            if (packSession is { Alive: true }) return;
            packRelayUp = false;
            if (launchStage != LaunchStage.Idle || relayWait is not null || connectWait is not null)
            {
                Log.Warning("session: the pack's program stopped while a launch was going on");
                if (relayWait is { } rw) { relayWait = null; rw.TrySetResult(false); }
                if (connectWait is { } cw) { connectWait = null; cw.TrySetResult(false); }
                UiService.ShowError("The multiplayer helper stopped unexpectedly. Try again; if it keeps failing, send the logs with Report a bug.");
                launchStage = LaunchStage.Idle; launchStatusMessage = "";
                StartGameOnLaunchReset();
            }
            StateHasChanged();
        }

        // ------------------------------------------------------------ what the player asks

        private async Task OpenHostModalViaPackAsync()
        {
            hostErrorMessage = "";
            hostLanAddresses = NetService.GetLocalIPv4Addresses();
            var s = EnsurePackSession();
            if (s is null) { UiService.ShowError("Hosting couldn't start: the mod's program is missing. Reinstall the mod from the game's page."); return; }
            if (!packRelayUp)
            {
                ConfigurePackSession();
                packRelayProblem = false;
                var tcs = relayWait = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
                s.Relay();
                await Task.WhenAny(tcs.Task, Task.Delay(TimeSpan.FromSeconds(30)));
                relayWait = null;
                if (packRelayProblem) return;   // Windows refused it, or a file is gone: told
            }
            showHostInfo = true;
            StateHasChanged();
        }

        private async Task LaunchGameViaPackAsync(ServerInfo server)
        {
            lastLaunchServer = server;   // LAUNCH ANYWAY repeats this launch
            if (launchStage != LaunchStage.Idle && launchStage != LaunchStage.Failed) { UiService.ShowError("A launch is already in progress."); return; }
            var s = EnsurePackSession();
            if (s is null) { UiService.ShowError("The game couldn't be started: the mod's program is missing. Reinstall the mod from the game's page."); return; }
            ConfigurePackSession();
            bool skip = skipModFileCheckOnce;
            skipModFileCheckOnce = false;
            s.Launch(server.Ip ?? "", server.Port, server.SteamCode, server.Ping, skip);
        }

        private async Task ConnectToGameViaPackAsync()
        {
            if (packSession is not { Alive: true } s || launchStage != LaunchStage.WaitingForConnect) return;
            if (!ConnectEnabled) { Log.Information("MP-LAUNCH CONNECT not taken: {Reason}", connectGateReason); return; }
            ConfigurePackSession();
            var tcs = connectWait = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            s.Connect();
            await Task.WhenAny(tcs.Task, Task.Delay(TimeSpan.FromSeconds(75)));
            connectWait = null;
        }
    }
}
