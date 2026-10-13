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
        private string? PackSessionProgram(string? gameId = null) => ProgramCached(ref sessionProgramCache, gameId ?? ActiveGameId, p => p is { Session: true });

        private (long At, string Id, string? Program) sessionProgramCache, setupProgramCache;
        private string? packSessionGameId;

        /// <summary>The game the pack-run features are about: the session's game while one is on, else the game selected on the left.</summary>
        private string ActiveGameId => packSession is { Alive: true } && !string.IsNullOrEmpty(packSessionGameId) ? packSessionGameId : selectedGameId;

        /// <summary>The pack's program when its contract says so, looked up at most every 2 s (the screens ask on every repaint).</summary>
        private static string? ProgramCached(ref (long At, string Id, string? Program) cache, string gameId, Func<PackContract, bool> wants)
        {
            long now = Environment.TickCount64;
            if (cache.At != 0 && cache.Id == gameId && now - cache.At < 2000) return cache.Program;
            string? found = null;
            var root = PackStore.DefaultRoot;
            if (gameId.Length > 0 && PackStore.ReadContract(root, gameId) is { } c && wants(c) && ExternalInstall.OptionsProgramPath(root, gameId) is { } p && File.Exists(p)) found = p;
            cache = (now, gameId, found);
            return found;
        }

        /// <summary>A game whose pack runs its session (Host and Join, the setup, the start-game flow) instead of the game being started by hand.</summary>
        private bool IsPackRunGame(string gameId) => gameId.Length > 0 && PackSessionProgram(gameId) is not null;

        private bool UsePackSession => PackSessionProgram() is not null;

        private string ModHandshake() => ModLua.Handshake(ActiveGameId);

        private string ActiveGameName => GameProfileRegistry.Get(ActiveGameId)?.DisplayName ?? "Game";

        /// <summary>The game whose Steam friends the Home page shows: the first one whose pack runs a session.</summary>
        private string FriendsGameId => GameProfileRegistry.All().FirstOrDefault(g => IsPackRunGame(g.Id))?.Id ?? "";

        /// <summary>The host's relay is up: the pack's, or the launcher's own.</summary>
        private bool HostRelayRunning => UsePackSession ? packRelayUp : hostedRelayProcess != null && !hostedRelayProcess.HasExited;

        private PackSession? EnsurePackSession()
        {
            string id = selectedGameId;
            if (packSession is { Alive: true })
            {
                // another game was selected while this one has nothing going on: its session ends, the selected game's starts
                if (packSessionGameId == id || gameRunning || launchStage != LaunchStage.Idle || packRelayUp) return packSession;
                ReleasePackSession();
            }
            packSession?.Dispose();
            packSession = null;
            if (PackSessionProgram(id) is not { } program) return null;
            packSessionGameId = id;
            var s = PackSession.Start(program, line => Log.Information("{Line}", Redact.Text(line)));
            if (s is null) return null;
            s.State += v => _ = InvokeAsync(() => ApplyPackView(v));
            s.Problem += p => _ = InvokeAsync(() => OnPackProblem(p));
            s.Error += t => _ = InvokeAsync(() => { UiService.ShowError(t); });
            s.Message += (title, text, next) => _ = InvokeAsync(() => ShowMessage(title, text, next));
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
            packSession?.Configure(GameExeOf(packSessionGameId ?? ActiveGameId) ?? "", settings.HostPort, settings.ServerInfoPort, settings.VersionIpcPort, settings.SteamAppId, settings.HostAllowSteam,
                settings.RelayReuse, settings.ConnectGate, settings.InjectDelaySeconds, settings.MenuTakeover,
                string.IsNullOrWhiteSpace(settings.AgentPath) ? "" : ResolveAgainstLauncher(settings.AgentPath),
                string.IsNullOrWhiteSpace(settings.RelayPath) ? "" : ResolveAgainstLauncher(settings.RelayPath),
                string.IsNullOrWhiteSpace(settings.DllPath) ? "" : ResolveAgainstLauncher(settings.DllPath),
                StartSaveRootPath());
        }


        // ------------------------------------------------------------ the entry points (the pack's session is the only way to host and join)

        private static string SteamAppNameOf(uint app) => app switch { 2429020 => "KCD2 Modding Tools (2429020)", 480 => "Spacewar (480)", 1771300 => "KCD2 retail (1771300)", _ => $"app {app}" };

        private const string ModTooOldText = "This mod is too old for this launcher: update it from the game's page, then try again.";

        private async Task OpenHostModal()
        {
            if (await OfferUpdateBeforeAsync("host")) return;   // a newer version is offered once
            if (!await EnsureSetupReadyAsync()) return;          // before a relay is started
            await OpenHostModalViaPackAsync();
        }

        private async Task LaunchGame(ServerInfo server)
        {
            if (!await EnsureSetupReadyAsync()) return;   // Host and Join unlock at Ready!
            await LaunchGameViaPackAsync(server);
        }

        private Task ConnectToGame() => ConnectToGameViaPackAsync();

        private void StopHostedRelay()
        {
            packSession?.RelayStop();
            packRelayUp = false;
        }

        private Task SendJoinChoiceAsync(string choice)
        {
            packSession?.Choice(choice);
            return Task.CompletedTask;
        }

        private void ResetLaunchState()
        {
            Log.Information("MP-LAUNCH launch state reset (stage was {Stage})", launchStage);
            invitedFriends.Clear();
            packSession?.Cancel();
            launchStage = LaunchStage.Idle;
            pendingGameProcess = null;
            launchStatusMessage = "";
            StartGameOnLaunchReset();   // the game's own menu again
            connectGateOpen = true;
            connectGateReason = "";
            StateHasChanged();
        }

        /// <summary>This launch's main menu is the pack's Start Game / Join Game (the screens only need to know it is on).</summary>
        private volatile bool startGameTakeover;
        private void StartGameOnLaunchReset() => startGameTakeover = false;

        /// <summary>Where the start saves are (the pack's session is told, as it runs from the mod's own folder).</summary>
        private string StartSaveRootPath() => Environment.GetEnvironmentVariable("CAIRN_START_SAVE") is { Length: > 0 } envRoot ? envRoot : ResolveAgainstLauncher(settings.StartSavePath);

        private Task FindSteamFriendsAsync()
        {
            if (EnsurePackSession() is not { } ps) { UiService.ShowError(ModTooOldText); return Task.CompletedTask; }
            steamBusy = true; steamBusyText = "Asking Steam which friends are hosting..."; steamFriendsNote = ""; StateHasChanged();
            ConfigurePackSession();
            ps.AskFriends();
            return Task.CompletedTask;
        }

        /// <summary>After a Steam failure: the same game, the agent restarted with the host's address.</summary>
        private async Task ConnectByAddressAfterSteamAsync()
        {
            string text = fallbackAddress.Trim();
            string host = text; int port = settings.HostPort;
            int colon = text.LastIndexOf(':');
            if (colon > 0 && text.IndexOf(':') == colon && int.TryParse(text[(colon + 1)..], out int p)) { host = text[..colon]; port = p; }
            if (!NetService.ValidateServerAddress(host, showError: false) || port is <= 0 or > 65535)
            {
                ShowMessage("STEAM CONNECTION FAILED", "That address doesn't look right. Type it like 203.0.113.7:7778 or myhost.example.net:7778.", "", addressFallback: true);
                return;
            }
            showMessage = false;
            if (packSession is { Alive: true } ps && launchStage == LaunchStage.Connected) { ConfigurePackSession(); ps.Fallback(host, port); }
            else await LaunchGame(new ServerInfo { Name = "(Address)", Ip = host, Port = port });
            StateHasChanged();
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
            if (sessionStartedUtc is { } started && launchedGameId.Length > 0) recent.AddPlaytime(launchedGameId, (int)(DateTime.UtcNow - started).TotalMinutes);
            sessionStartedUtc = null;
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
            if (s is null) { UiService.ShowError(ModTooOldText); return; }
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
            if (s is null) { UiService.ShowError(ModTooOldText); return; }
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
