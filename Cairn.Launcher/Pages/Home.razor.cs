// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
// Portions from the original project, marczukmichal/kcd2-multiplayer; its author keeps their copyright (AUTHORS.md).
using System;
using Cairn.Setup;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using Cairn.Launcher.Components;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Launcher.Services;
using Microsoft.AspNetCore.Components;
using Microsoft.JSInterop;
using Serilog;
using Cairn.Wire;


// TO DO
// SPLIT THIS MF INTO SEVERAL COMPONENTS, THIS FILE IS GETTING OUT OF HAND

namespace Cairn.Launcher.Pages
{
    public partial class Home
    {

        [Inject] public required IJSRuntime JSRuntime { get; set; }
        [Inject] public required UiService UiService { get; set; }

        [Inject] public required NetService NetService { get; set; }

        #region Constants
        private const string SettingsFileName = "settings.json";
        #endregion

        #region State
        private bool isLoading = false;



        // Modals
        private bool showExitConfirm = false;
        private bool showHostInfo = false;
        private bool showReportBug = false;
        private bool showVersionMismatch = false;
        private string versionMismatchMessage = "";
        // the joiner's world transfer ("Receiving the world... 62%"), polled from the agent.
        private string joinStatusMessage = "";
        private string joinStatusState = "idle";   // "choose" shows the first-join buttons
        private bool joinCanFresh;                 // no usable save -- "Join with a new character"
        // a joiner waits at the main menu and joins the host's world from there.
        internal const string JoinerReadyText = "Stay at the main menu. Don't load a save. Click CONNECT -- you'll join your host's world automatically.";

        private DotNetObjectReference<Home>? objRef;
        private AppSettings settings = new AppSettings();
        #endregion

        #region Host state
        private Process? hostedRelayProcess = null;
        private List<string> hostLanAddresses = new List<string>();
        private string hostErrorMessage = "";
        #endregion

        #region Master server state
        private Process? hostedMasterServerProcess = null;
        #endregion

        #region Launch/connect state
        // The launcher used to inject the instant WHGame.dll became loadable,
        // which happens well before the game finishes loading a save -- the
        // native DLL's own liveness check then finds zero engine ticks and
        // silently aborts (see docs/VERIFICATION-REPORT.md, "Real finding").
        // Re-injecting the same DLL path a second time does not help: Windows
        // does not re-run DllMain for an already-loaded module. So injection
        // now happens exactly once, gated on the player confirming they are
        // actually in a loaded world, and the result is verified against the
        // DLL's own log rather than trusted from the injector's exit code.
        private enum LaunchStage { Idle, WaitingForConnect, Injecting, Verifying, Connected, Failed }
        private LaunchStage launchStage = LaunchStage.Idle;
        private Process? pendingGameProcess = null;
        private string launchStatusMessage = "";
        // the agent started by ConnectToGame, tracked so a later Connect
        // (e.g. after the game was closed and relaunched without restarting the
        // launcher) can kill the old one first instead of leaving it running
        // alongside a new one. Two agents both claiming the same local player
        // identity to the relay was observed live to cause an infinite
        // reconnect/respawn war -- ~4000 spawn cycles in ~10 minutes -- because
        // neither side's ghost could ever be cleanly removed while the other
        // kept reasserting it.
        #endregion

        #region Filter State
        private bool isFilterOpen = false;
        #endregion

        #region Data & Logic

        // Dummy Data
        private List<ServerInfo> servers = new List<ServerInfo>();
    //    private List<ServerInfo> servers = new List<ServerInfo>
    //{
    //    new ServerInfo { Name = "Official PL Server #1", Ip = "127.0.0.1", Port = 27015, MapName = "skalitz_survival", Players = 12, MaxPlayers = 64, Ping = 25 },
    //    new ServerInfo { Name = "Roleplay Tavern", Ip = "192.168.1.5", Port = 7777, MapName = "rataje_city", Players = 60, MaxPlayers = 64, Ping = 42 },
    //    new ServerInfo { Name = "High Ping Test", Ip = "10.0.0.2", Port = 28000, MapName = "debug_map", Players = 1, MaxPlayers = 10, Ping = 150 },
    //    new ServerInfo { Name = "Laggy Server", Ip = "10.0.0.2", Port = 28000, MapName = "forest", Players = 5, MaxPlayers = 10, Ping = 80 },
    //    new ServerInfo { Name = "Polish Hussars", Ip = "10.0.0.5", Port = 28000, MapName = "skalitz_survival", Players = 32, MaxPlayers = 64, Ping = 30 },
    //};
        private const string CustomServersFileName = "custom_servers.json";
        private List<ServerInfo> customServers = new List<ServerInfo>();

        public void DisplayError()
        {
            UiService.ShowError("This is a test error message!");
        }

        #endregion

        #region Lifecycle & Methods
        protected override async Task OnInitializedAsync()
        {
            LoadSettings();
            if (string.Equals(settings.StartPage, "library", StringComparison.OrdinalIgnoreCase)) currentView = AppView.Library;   // Settings > General: the page Cairn opens on
            LoadCustomServers();
            try
            {
                // a mod that lost files (an installer sweep, an antivirus) is not "downloaded": it is offered for download again instead of failing at Play
                foreach (var id in PackStore.RemoveBroken(PacksRoot)) Log.Warning("packs: {Pack} had lost files, so it is downloaded again when asked", id);
            }
            catch (Exception ex) { Log.Debug(ex, "packs: the check for lost files failed"); }
            StartSetupCheck();   // in the background; replaces the old "open Settings when no game path" check
            _ = RefreshCatalogAndPacksAsync();   // which games there are, then which mods they offer
            StartAutoUpdateLoop();
            LoadSteamSelf();
            _ = RefreshSteamFriendStatusAsync();   // the Friends tile is filled as the launcher opens, not at the first minute's look
            _ = LoadChangelogAsync();   // the Versions page and Home show what changed
            StartHomeFriendsLoop();        // who is hosting, while Home is showing
            showTour = !listPrefs.TourSeen;   // the first-run tour
            CheckInstallIntegrity();
            Globals.OnStyleChanged += OnStyleChanged;
            Downloads.Changed += OnDownloadsChanged;
            GameProfileRegistry.Changed += OnGamesChanged;
            await EnsureLocalMasterServerAsync();
            StartPresence();   // friends on Cairn: who is online, invitations
            await RefreshApp();
        }

        /// <summary>
        /// NPC sync follow-up: a mixed install (some DLLs from one build, some
        /// from another -- the half-applied-installer failure) is invisible
        /// to the wire protocol because the mix is inside one machine, so
        /// the launcher checks for it the moment it starts. Reuses the
        /// version-mismatch modal: same shape of problem, same "someone
        /// needs to update" conversation, just with yourself.
        /// </summary>
        private void CheckInstallIntegrity()
        {
            var problem = InstallIntegrity.CheckForPartialInstall();
            if (problem is null) return;
            Log.Warning("Partial install detected: {Problem}", problem);
            versionMismatchMessage = problem;
            showVersionMismatch = true;
        }

        // the accent colour choices of Settings > Appearance: id, the colour, the brighter one beside it
        private static readonly (string Id, string Color, string Bright)[] Accents =
        {
            ("gold", "#E8A35A", "#F2BA80"), ("blue", "#5AA9E8", "#80C0F2"), ("green", "#6FCF8A", "#92DEA8"),
            ("pink", "#E86AA8", "#F28FC0"), ("violet", "#9B7BE8", "#B7A0F2"), ("red", "#E8635A", "#F2857E"),
        };

        private void OnStyleChanged() => _ = ApplyStyleProfile();

        /// <summary>
        /// Pushes the current StyleProfile's palette over site.css's :root
        /// defaults (see StyleProfile.ToCssVariables). Called once after first
        /// render -- JS interop is not available before then -- and again on
        /// every Globals.OnStyleChanged, so switching or editing a profile
        /// takes effect immediately without a restart.
        /// </summary>
        private async Task ApplyStyleProfile()
        {
            try
            {
                var accent = Accents.FirstOrDefault(a => a.Id == settings.AccentColor);
                if (accent.Id is not null)
                {
                    Globals.CurrentStyleProfile.ColorGold = accent.Color;
                    Globals.CurrentStyleProfile.ColorGoldBright = accent.Bright;
                }
                await JSRuntime.InvokeVoidAsync("applyCairnTheme", Globals.CurrentStyleProfile.ToCssVariables());
                await JSRuntime.InvokeVoidAsync("setCairnFlag", "reduce-motion", settings.ReduceMotion);
            }
            catch (Exception ex) { Log.Debug(ex, "Could not apply the style profile"); }
        }

        //CUSTOM SERVERS LOGIC
        private void LoadCustomServers()
        {
            try
            {
                if (File.Exists(CustomServersFileName))
                {
                    var json = File.ReadAllText(CustomServersFileName);
                    var loaded = JsonSerializer.Deserialize<List<ServerInfo>>(json);
                    if (loaded != null) customServers = loaded;
                }
            }
            catch { }
            NoteCustomServerKeys();   // each entry's address as the file has it, for an edit
        }


        protected override async Task OnAfterRenderAsync(bool firstRender)
        {
            if (firstRender)
            {
                objRef = DotNetObjectReference.Create(this);
                await JSRuntime.InvokeVoidAsync("registerGlobalKeys", objRef);
                await ApplyStyleProfile();
                try { await JSRuntime.InvokeVoidAsync("cairnGamepad"); } catch (Exception) { /* controller support is a convenience */ }
                _ = CheckForUpdateAsync();   // Cairn: is a newer version out (an orange dot shows it, nothing opens)
                _ = WatchGameAsync();             // Cairn: is the game running

                await JSRuntime.InvokeVoidAsync("eval", @"
                document.addEventListener('keydown', (e) => {
                    if(e.key === 'F10') {
                        e.preventDefault();
                        DotNet.invokeMethodAsync('Cairn.Launcher', 'ToggleSettingsStatic');
                    }
                })
            ");
                // the mouse's side buttons go back and forward through the pages, like a browser (4th = back, 5th = forward)
                await JSRuntime.InvokeVoidAsync("eval", @"
                window.__cairnNav = (ref) => {
                    const swallow = (e) => { if (e.button === 3 || e.button === 4) { e.preventDefault(); e.stopPropagation(); } };
                    window.addEventListener('mousedown', swallow, true);
                    window.addEventListener('auxclick', swallow, true);
                    window.addEventListener('mouseup', (e) => {
                        if (e.button === 3) { e.preventDefault(); ref.invokeMethodAsync('NavBackJS'); }
                        else if (e.button === 4) { e.preventDefault(); ref.invokeMethodAsync('NavForwardJS'); }
                    }, true);
                };
            ");
                await JSRuntime.InvokeVoidAsync("__cairnNav", objRef);
            }
        }

        // a download moved or the queue changed: repaint the game page, Home and the footer
        private void OnDownloadsChanged() => _ = InvokeAsync(StateHasChanged);

        public void Dispose()
        {
            Globals.OnStyleChanged -= OnStyleChanged;
            Downloads.Changed -= OnDownloadsChanged;
            GameProfileRegistry.Changed -= OnGamesChanged;
            gameWatchCts.Cancel();
            packSession?.Dispose();
            StopPresence();
            objRef?.Dispose();
        }

        // Cairn: "is the game running?" -- one small line on the menu, looked at every few seconds
        private readonly CancellationTokenSource gameWatchCts = new();
        private bool gameRunning;
        private string launchedAs = "";   // "host" or "join": what this launcher started the running game as
        private string launchedGameId = "";   // id of the game profile that was launched

        // HOST GAME starts hosting; while this launcher's own hosted game runs the same button shows the Host window again (the code)
        private async Task OpenHostOrCode()
        {
            if (gameRunning && launchedAs == "host" && HostRelayRunning)
            {
                hostErrorMessage = "";
                hostLanAddresses = NetService.GetLocalIPv4Addresses();
                showHostInfo = true;
                return;
            }
            await OpenHostModal();
        }

        private bool hostCodeCopied;

        private async Task CopyHostCodeAsync()
        {
            if (string.IsNullOrWhiteSpace(hostSteamCode))
            {
                // If Steam code isn't polled yet, show the host info modal so they can see LAN IPs or wait for code
                await OpenHostOrCode();
                return;
            }
            try
            {
                await JSRuntime.InvokeVoidAsync("navigator.clipboard.writeText", hostSteamCode);
                hostCodeCopied = true;
                ShowToast("Host code copied to clipboard");
                _ = Task.Delay(2500).ContinueWith(_ =>
                {
                    hostCodeCopied = false;
                    return InvokeAsync(StateHasChanged);
                });
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "Failed to copy host code to clipboard");
                ShowToast("Could not copy host code to clipboard", error: true);
            }
        }

        private async Task WatchGameAsync()
        {
            try
            {
                int tick = 0;
                while (!gameWatchCts.IsCancellationRequested)
                {
                    bool now = false;
                    try
                    {
                        if (pendingGameProcess is { HasExited: false })
                        {
                            now = true;
                        }
                        else
                        {
                            string exeName = "KingdomCome";
                            var curProfile = GameProfileRegistry.Get(launchedGameId) ?? SelectedProfile;
                            if (curProfile != null && !string.IsNullOrWhiteSpace(curProfile.DefaultExeName))
                                exeName = Path.GetFileNameWithoutExtension(curProfile.DefaultExeName);
                            var ps = Process.GetProcessesByName(exeName);
                            now = ps.Length > 0;
                            foreach (var p in ps) p.Dispose();
                        }
                    }
                    catch { }
                    if (now != gameRunning)
                    {
                        gameRunning = now;
                        PublishPresenceStatus();   // friends see online / playing / hosting
                        if (!now)
                        {
                            bool wasHost = launchedAs == "host";
                            launchedAs = "";
                            launchedGameId = "";
                            // a relay this launcher started for the hosted game goes with it: left running it holds the host port, and the
                            // next Host finds the port taken
                            // (the pack's session program stops its relay, its agent and the Modding Tools' helpers when the game ends)
                            // the game closed on its own (the player quit it, or it crashed): the launch it belonged to is over, so the next
                            // Host or Join is not refused with "a launch is already in progress"
                            if (launchStage != LaunchStage.Idle) await InvokeAsync(ResetLaunchState);
                        }
                        await InvokeAsync(StateHasChanged);
                    }

                    // Cairn: the "Ready to play" chip is only as true as the last setup check, so while nothing else is going on the
                    // check is taken again every 15 seconds (it takes about 60 ms; nothing is shown, nothing is changed).
                    if (++tick % 5 == 0 && !now && !setupBusy && !showSetup && launchStage == LaunchStage.Idle)
                    {
                        try
                        {
                            await InvokeAsync(async () => { await RefreshPackSetupAsync(); StateHasChanged(); });
                            
                            await UpdatePreflightStatusAsync(showModal: false);
                        }
                        catch (Exception ex) { Log.Debug(ex, "setup: the periodic check failed"); }
                    }
                    await Task.Delay(3000, gameWatchCts.Token);
                }
            }
            catch (OperationCanceledException) { }
            catch (Exception ex) { Log.Warning(ex, "game watch stopped"); }
        }

        // JS Invokables
        [JSInvokable("ToggleSettingsStatic")] public static void ToggleSettingsStatic() { }

        [JSInvokable]
        public void ToggleFiltersJS()
        {
            isFilterOpen = !isFilterOpen;
            StateHasChanged();
        }

        [JSInvokable]
        public void ToggleSettingsJS()
        {
            // opening re-reads settings.json (a value the mod menu wrote shows as saved);
            // closing without SAVE puts the unsaved edits back.
            if (showSettings) CancelSettings();
            else OpenSettings();
            StateHasChanged();
        }

        [JSInvokable]
        public void ExitApp()
        {
            if (searchBar is { IsOpen: true }) { searchBar.Close(); StateHasChanged(); return; }
            if (menuGameId is not null) { menuGameId = null; StateHasChanged(); return; }
            if (showSettings) { CancelSettings(); StateHasChanged(); return; }
            if (showGameSettings) { CancelGameSettings(); StateHasChanged(); return; }
            if (isFilterOpen) { isFilterOpen = false; StateHasChanged(); return; }
            if (!settings.ConfirmQuit) return;   // Settings > General: Escape does not offer to close Cairn
            showExitConfirm = true;
            StateHasChanged();
        }

        [JSInvokable]
        public async Task RefreshApp()
        {
            if (isLoading) return;
            isLoading = true;
            StateHasChanged();

            // Independent of the list fetch below: an empty list is what "the
            // master is fine and nobody is hosting" looks like too, and the
            // status bar must not show that as "offline".
            Globals.IsMasterOnline = await NetService.IsMasterServerOnlineAsync(settings.MasterServerUrl);

            var fetchedServers = await NetService.GetServersFromMasterAsync(settings.MasterServerUrl);
            servers = fetchedServers;
            StateHasChanged();

            var tasks = servers.Concat(customServers).Select(async server =>
            {
                server.Ping = await NetService.SendPingServerAsync(server.Ip);

                // The info endpoint is a different port from the one peers
                // connect on. The master server now publishes each
                // relay's real one; fall back to the configured guess only
                // for a manually-added server, which the master never told
                // us about.
                var infoPort = server.InfoPort > 0 ? server.InfoPort : settings.ServerInfoPort;
                var details = await NetService.GetDedicatedServerInfoAsync(server.Ip, infoPort);

                if (details != null)
                {
                    server.MapName = details.MapName;
                    server.Players = details.Players;
                    server.MaxPlayers = details.MaxPlayers;
                    server.IsOnline = true;
                }
                else
                {
                    // Registered with the master but not answering. Leave the
                    // registered map name in place and say it is offline rather
                    // than showing counts we do not have.
                    server.IsOnline = false;
                }
                await InvokeAsync(StateHasChanged);
            });

            await Task.WhenAll(tasks);

            isLoading = false;
            StateHasChanged();
        }


        // Actions

        private void CancelPendingConnect() => ResetLaunchState();

        private async Task LaunchAsHost()
        {
            showHostInfo = false;
            // hosting needs its own relay: without it the pack's program would treat 127.0.0.1 as somebody else's host and start the game as a joiner
            if (UsePackSession && !await StartHostRelayAsync()) return;
            var hostServer = new ServerInfo
            {
                Name = "(Hosting)",
                Ip = "127.0.0.1",
                Port = settings.HostPort,
                Ping = 0
            };
            await LaunchGame(hostServer);
        }

        /// <summary>
        /// Starts the master server locally, alongside the launcher itself,
        /// so a default install has something answering MasterServerUrl
        /// instead of showing "Could not connect to Master Server!" on every
        /// launch. Unlike the relay, this runs unconditionally at
        /// startup rather than only on Host -- the browser fetches the list
        /// immediately in OnInitializedAsync, before the player has done
        /// anything.
        ///
        /// Only when the configured URL is loopback: pointing MasterServerUrl
        /// at a friend's real address means their instance is the one meant
        /// to answer, and starting a private, empty one of our own here would
        /// just be a wasted background process that nobody's relay announces
        /// to.
        /// </summary>
        private async Task EnsureLocalMasterServerAsync()
        {
            Log.Debug("EnsureLocalMasterServerAsync: MasterServerUrl = \"{Url}\"", settings.MasterServerUrl);

            if (!Uri.TryCreate(settings.MasterServerUrl, UriKind.Absolute, out var uri) || !IsLoopbackHost(uri.Host))
            {
                Log.Debug("EnsureLocalMasterServerAsync: not a loopback URL, not starting a local instance.");
                return;
            }

            if (await IsMasterServerRespondingAsync(uri))
            {
                Log.Debug("EnsureLocalMasterServerAsync: something is already answering on port {Port}.", uri.Port);
                return;
            }

            if (hostedMasterServerProcess != null && !hostedMasterServerProcess.HasExited)
            {
                return;
            }

            string masterServerPath = ResolveAgainstLauncher(settings.MasterServerPath);
            if (!File.Exists(masterServerPath))
            {
                Log.Warning("Master server executable not found at: {Path}. The status bar will read \"Master: Offline\" until one is installed or Settings -> Master Server URL points elsewhere.", masterServerPath);
                return;
            }

            try
            {
                var startInfo = new ProcessStartInfo
                {
                    FileName = masterServerPath,
                    Arguments = $"--urls http://0.0.0.0:{uri.Port}",
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    WorkingDirectory = Path.GetDirectoryName(masterServerPath)
                };
                hostedMasterServerProcess = Process.Start(startInfo);
                if (hostedMasterServerProcess is null)
                {
                    Log.Warning("Process.Start returned null for the master server; not retrying this launch.");
                    return;
                }
                Log.Information("Starting local master server: {Path} (pid {Pid})", masterServerPath, hostedMasterServerProcess.Id);

                // Polled rather than a single fixed delay: a self-contained
                // ASP.NET Core app's cold start (JIT, Kestrel bind) can take
                // longer than any one guessed wait, and a single check right
                // after Process.Start can race RefreshApp's first fetch --
                // exactly what this replaces, found live: the process started
                // and kept running, but RefreshApp's request landed before
                // Kestrel had actually opened the port.
                var deadline = DateTime.UtcNow.AddSeconds(8);
                var up = false;
                while (DateTime.UtcNow < deadline)
                {
                    if (hostedMasterServerProcess.HasExited)
                    {
                        // The most common real reason: two launcher instances
                        // both decided to start one (this check and
                        // Process.Start above are not atomic together), and
                        // this one lost the port to the other. That other
                        // instance's copy is a perfectly good master server,
                        // so check for it once before giving up rather than
                        // reporting a failure that is not actually happening.
                        Log.Information("This launcher's own master server process exited early (code {Code}); checking whether another instance already claimed the port.",
                            hostedMasterServerProcess.ExitCode);
                        up = await IsMasterServerRespondingAsync(uri);
                        break;
                    }

                    if (await IsMasterServerRespondingAsync(uri))
                    {
                        up = true;
                        break;
                    }

                    await Task.Delay(200);
                }

                if (up)
                {
                    Log.Information("Master server is up on port {Port}.", uri.Port);
                }
                else
                {
                    Log.Warning("Local master server did not answer on port {Port} within 8s; giving up for this launch.", uri.Port);
                    hostedMasterServerProcess = null;
                }
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "Could not start the local master server");
                hostedMasterServerProcess = null;
            }
        }

        /// <summary>Cheap liveness probe: is anything answering the master API at all, ours or someone else's.</summary>
        private static async Task<bool> IsMasterServerRespondingAsync(Uri baseUri)
        {
            try
            {
                using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(1) };
                using var response = await http.GetAsync(new Uri(baseUri, MasterApi.StatusPath));
                return response.IsSuccessStatusCode;
            }
            catch
            {
                return false;
            }
        }

        private static bool IsLoopbackHost(string host) =>
            host.Equals("localhost", StringComparison.OrdinalIgnoreCase) ||
            (System.Net.IPAddress.TryParse(host, out var ip) && System.Net.IPAddress.IsLoopback(ip));

        private void StopHostedMasterServer()
        {
            try
            {
                if (hostedMasterServerProcess != null && !hostedMasterServerProcess.HasExited)
                {
                    hostedMasterServerProcess.Kill();
                }
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "Failed to stop the hosted master server process");
            }
            hostedMasterServerProcess = null;
        }

        /// <summary>
        /// The install root, two levels above KingdomCome.exe
        /// (&lt;root&gt;\Bin\&lt;config&gt;\KingdomCome.exe) -- where Warhorse ships
        /// steam_appid.txt for the Modding Tools build, not beside the exe.
        ///
        /// This matters because we start the game with Process.Start rather
        /// than through Steam itself, so there is no Steam-injected app-id
        /// context for the process -- SteamAPI_Init() falls back to reading
        /// steam_appid.txt from the process's *working directory*. Launching
        /// with WorkingDirectory set to the exe's own folder (the previous
        /// behavior) means that file is never found, which is consistent with
        /// what observed: the game runs, but any save referencing DLC
        /// content is greyed out in the load list, matching a process that
        /// never established which App ID it should check DLC ownership
        /// against.
        ///
        /// Falls back to the exe's own directory -- the old behavior -- if
        /// steam_appid.txt cannot be found within a few levels, so an
        /// unexpected install layout degrades to what shipped before rather
        /// than breaking outright.
        /// </summary>
        public static string GameRootOf(string gamePath)
        {
            string dir = Path.GetDirectoryName(gamePath) ?? "";
            string fallback = dir;

            for (int i = 0; i < 4 && dir.Length > 0; i++)
            {
                if (File.Exists(Path.Combine(dir, "steam_appid.txt")))
                    return dir;
                dir = Path.GetDirectoryName(dir) ?? "";
            }

            return fallback;
        }

        /// <summary>
        /// combat visualisation, item K: where the log bundle should look for kcd.log --
        /// derived from the configured game path; empty when unconfigured
        /// (LogBundle skips what it cannot find).
        /// </summary>
        private string LogBundleGameRoot =>
            packGameDirs.TryGetValue(ActiveGameId, out var dir) ? dir : GameExeOf(ActiveGameId) is { } exe ? GameRootOf(exe) : "";

        /// <summary>
        /// A relative path in settings means "next to the launcher", which is
        /// where a packaged build puts the programs it ships.
        /// </summary>
        private static string ResolveAgainstLauncher(string path)
        {
            if (string.IsNullOrWhiteSpace(path)) return "";
            if (!Path.IsPathRooted(path))
                path = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, path);
            return Path.GetFullPath(path);
        }


        private void ConfirmExit()
        {
            setupPollCts?.Cancel();   // first-run setup
            StopHostedRelay();
            packSession?.Dispose();
            StopHostedMasterServer();
            Environment.Exit(0);
        }

        // Persistence

        /// <summary>
        /// settings.json is read key by key (LauncherSettingsStore): a value that does not
        /// read keeps its default instead of resetting every setting, and the values as read are
        /// the baseline every later save is compared with. The migrations below correct the
        /// in-memory value only -- the file keeps what the player has until the player changes it.
        /// </summary>
        private void LoadSettings()
        {
            var problems = new List<string>();
            settings = settingsStore.Load(problems);
            foreach (var p in problems) Log.Warning("settings: {Problem}", p);
        }

        /// <summary>The launcher's own corrections of known stale values (memory only, see LoadSettings).</summary>
        private static void MigrateStaleSettings(AppSettings s)
        {
            MigrateStaleMasterServerUrl(s);
            RenamedPrograms.Apply(s);   // 0.1.x settings name the programs by their old file names
            MigrateStaleMasterServerPath(s);
        }

        /// <summary>
        /// An existing settings.json predating has the old Flask-shaped
        /// default baked in ("http://localhost:5000/servers/servers_list") --
        /// a fresh AppSettings() only supplies the new default
        /// (EnsureLocalMasterServerAsync's own logic) for a settings.json that
        /// does not exist yet. Migrated only when it is exactly that known old
        /// literal, never when it looks like something the player configured
        /// on purpose.
        /// </summary>
        private static readonly string[] KnownStaleMasterServerUrls =
        [
            "http://localhost:5000/servers/servers_list", // pre-(Flask)
            "http://localhost:5100",                      // first pass -- see AppModels.cs
        ];

        /// <summary>
        /// Same idea as <see cref="MigrateStaleMasterServerUrl"/>: a
        /// settings.json saved before the master server got its own
        /// MasterServer\ subfolder (see AppModels.cs and
        /// tools/Publish-Release.ps1) would otherwise keep pointing at a bare
        /// filename that no longer exists there.
        /// </summary>
        private static readonly string[] KnownStaleMasterServerPaths =
        [
            "CairnMasterServer.exe", // flat-merged, pre-subfolder-isolation
        ];

        private static void MigrateStaleMasterServerUrl(AppSettings s)
        {
            if (Array.IndexOf(KnownStaleMasterServerUrls, s.MasterServerUrl) >= 0)
            {
                s.MasterServerUrl = new AppSettings().MasterServerUrl;
            }
        }

        private static void MigrateStaleMasterServerPath(AppSettings s)
        {
            if (Array.IndexOf(KnownStaleMasterServerPaths, s.MasterServerPath) >= 0)
            {
                s.MasterServerPath = new AppSettings().MasterServerPath;
            }
        }

        /// <summary>The settings window's SAVE: only what the player changed is written.</summary>
        private void SaveSettings()
        {
            WriteSettings("the settings window");
            showSettings = false;
            _ = ApplyStyleProfile();   // the accent colour and reduce motion show at once
        }


        #endregion
    }
}

