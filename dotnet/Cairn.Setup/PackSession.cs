// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Diagnostics;
using System.Text;
using System.Text.Json;

namespace Cairn.Setup;

/// <summary>The host's side of what a session reports: whether its relay is up, why not, and what Steam says (the code a friend joins with).</summary>
public sealed record SessionHostView(bool RelayUp, string Error, string SteamState, string SteamMessage, string? SteamCode, string Refused);

/// <summary>Everything the launch panel shows, in one piece: the session sends the whole of it each time something changes.</summary>
public sealed record SessionView(string Stage, string Status, bool ConnectEnabled, string ConnectReason, string ConnText, bool ConnBad,
                                 string JoinState, string JoinMessage, bool JoinCanFresh, SessionHostView Host, bool GameRunning)
{
    public static SessionView Idle { get; } = new("idle", "", true, "", "", false, "idle", "", false, new SessionHostView(false, "", "off", "", null, ""), false);
}

/// <summary>A message to show: a problem Windows or a file caused (with Report a bug beside it), or a plain message.</summary>
public sealed record SessionProblem(string Title, string Text, string Next, bool ReportBug, bool LaunchAnyway, bool AddressFallback);

public sealed record SessionFriend(string Name, string Code, string Release);
public sealed record SessionFriends(string State, string Message, string Next, IReadOnlyList<SessionFriend> Friends);
public sealed record SessionGame(int Pid, DateTime StartedLocal, bool Hosting);

/// <summary>
/// A game's connection side run by its pack (<c>pack.json</c> <c>"session": true</c>; docs/SESSION.md): the pack's program (<c>optionsProgram</c>) started as
/// <c>program session</c>, spoken to in JSON lines. The launcher sends commands and draws the state it gets back; the relay, the game's start, the plugin and the agent
/// are the pack's. Everything read from the program is bounded and plain; an unknown message is ignored.
/// </summary>
public sealed class PackSession : IDisposable
{
    public const int Protocol = 1;
    public const int MaxLine = 64 * 1024;

    private readonly Process? _process;
    private readonly object _write = new();
    private volatile bool _gone;

    public event Action<SessionView>? State;
    public event Action<SessionProblem>? Problem;
    public event Action<string>? Error;
    public event Action<string, string, string>? Message;     // title, text, next
    public event Action<SessionGame>? Game;
    public event Action? GameExit;
    public event Action<string, string>? Refused;      // text, key
    public event Action<string>? VersionMismatch;
    public event Action<string>? Toast;
    public event Action<SessionFriends>? Friends;
    public event Action<string>? Log;
    public event Action? Exited;

    public bool Alive => !_gone;
    public SessionView View { get; private set; } = SessionView.Idle;

    internal PackSession(Process? p) { _process = p; _gone = p is null; }

    /// <summary>Starts the pack's program; null when it is not there or did not start.</summary>
    public static PackSession? Start(string program, Action<string>? log = null)
    {
        if (!File.Exists(program)) return null;
        var psi = new ProcessStartInfo(program)
        {
            WorkingDirectory = Path.GetDirectoryName(program)!, UseShellExecute = false, CreateNoWindow = true,
            RedirectStandardInput = true, RedirectStandardOutput = true,
            StandardInputEncoding = new UTF8Encoding(false), StandardOutputEncoding = Encoding.UTF8,
        };
        psi.ArgumentList.Add("session");
        Process? p;
        try { p = Process.Start(psi); }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { log?.Invoke("session: the pack's program did not start (" + ex.Message + ")"); return null; }
        if (p is null) return null;
        var s = new PackSession(p);
        if (log is not null) s.Log += log;
        _ = Task.Run(s.ReadLoopAsync);
        return s;
    }

    private async Task ReadLoopAsync()
    {
        try
        {
            string? line;
            while ((line = await _process!.StandardOutput.ReadLineAsync().ConfigureAwait(false)) is not null)
                if (line.Length is > 0 and <= MaxLine) Handle(line);
        }
        catch (Exception ex) when (ex is IOException or ObjectDisposedException or InvalidOperationException) { }
        _gone = true;
        Exited?.Invoke();
    }

    /// <summary>One line from the program; unknown or damaged lines do nothing.</summary>
    public void Handle(string line)
    {
        try
        {
            using var doc = JsonDocument.Parse(line);
            var r = doc.RootElement;
            if (r.ValueKind != JsonValueKind.Object || !r.TryGetProperty("t", out var tv) || tv.ValueKind != JsonValueKind.String) return;
            string Str(JsonElement e, string n, int max = 600) => e.ValueKind == JsonValueKind.Object && e.TryGetProperty(n, out var v) && v.ValueKind == JsonValueKind.String ? CatalogLimits.Text(v.GetString() ?? "", max) : "";
            bool Bool(JsonElement e, string n) => e.ValueKind == JsonValueKind.Object && e.TryGetProperty(n, out var v) && v.ValueKind == JsonValueKind.True;
            JsonElement Obj(JsonElement e, string n) => e.TryGetProperty(n, out var v) && v.ValueKind == JsonValueKind.Object ? v : default;
            switch (tv.GetString())
            {
                case "state":
                {
                    var host = Obj(r, "host"); var conn = Obj(r, "conn"); var join = Obj(r, "join"); var connect = Obj(r, "connect");
                    string stage = Str(r, "stage", 20);
                    if (stage is not ("idle" or "waiting-connect" or "injecting" or "verifying" or "connected" or "failed")) return;
                    string? code = host.ValueKind == JsonValueKind.Object && host.TryGetProperty("steamCode", out var sc) && sc.ValueKind == JsonValueKind.String ? CatalogLimits.Text(sc.GetString() ?? "", 200) : null;
                    string joinState = Str(join, "state", 30);
                    View = new SessionView(stage, Str(r, "status"), connect.ValueKind != JsonValueKind.Object || Bool(connect, "enabled"), Str(connect, "reason"),
                        Str(conn, "text"), Bool(conn, "bad"), joinState.Length > 0 ? joinState : "idle", Str(join, "message"), Bool(join, "canFresh"),
                        new SessionHostView(Bool(host, "relay"), Str(host, "error"), Str(host, "steamState", 30) is { Length: > 0 } ss ? ss : "off", Str(host, "steamMessage"), code, Str(host, "refused", 800)),
                        r.TryGetProperty("game", out var g) && g.ValueKind == JsonValueKind.True);
                    State?.Invoke(View);
                    break;
                }
                case "problem": Problem?.Invoke(new SessionProblem(Str(r, "title", 80), Str(r, "text", 1200), Str(r, "next", 1500), Bool(r, "reportBug"), Bool(r, "launchAnyway"), false)); Log?.Invoke("session: " + Str(r, "log", 300)); break;
                case "message":
                    if (Bool(r, "addressFallback")) Problem?.Invoke(new SessionProblem(Str(r, "title", 80), Str(r, "text", 1200), Str(r, "next", 1500), false, false, true));
                    else Message?.Invoke(Str(r, "title", 80), Str(r, "text", 1200), Str(r, "next", 1500));
                    break;
                case "error": if (Str(r, "text", 800) is { Length: > 0 } et) Error?.Invoke(et); break;
                case "toast": if (Str(r, "text", 200) is { Length: > 0 } tt) Toast?.Invoke(tt); break;
                case "log": if (Str(r, "line", 600) is { Length: > 0 } ll) Log?.Invoke("session: " + ll); break;
                case "game":
                    if (r.TryGetProperty("pid", out var pid) && pid.ValueKind == JsonValueKind.Number && pid.TryGetInt32(out var pidv) && pidv > 0)
                    {
                        var started = DateTime.TryParse(Str(r, "startedAt", 40), null, System.Globalization.DateTimeStyles.RoundtripKind, out var st) ? st.ToLocalTime() : DateTime.Now;
                        Game?.Invoke(new SessionGame(pidv, started, Bool(r, "hosting")));
                    }
                    break;
                case "game_exit": GameExit?.Invoke(); break;
                case "refused": Refused?.Invoke(Str(r, "text", 800), Str(r, "key", 60)); break;
                case "version_mismatch": VersionMismatch?.Invoke(Str(r, "text", 600)); break;
                case "friends":
                {
                    var list = new List<SessionFriend>();
                    if (r.TryGetProperty("friends", out var fs) && fs.ValueKind == JsonValueKind.Array)
                        foreach (var f in fs.EnumerateArray().Take(100))
                            list.Add(new SessionFriend(Str(f, "name", 80), Str(f, "code", 200), Str(f, "release", 20)));
                    Friends?.Invoke(new SessionFriends(Str(r, "state", 30), Str(r, "message"), Str(r, "next"), list));
                    break;
                }
            }
        }
        catch (JsonException) { }
    }


    /// <summary>Runs one of the pack program's one-shot verbs and feeds its output lines to a reader the caller wired (the same lines a session sends). False when it did not start or timed out.</summary>
    public static async Task<bool> RunOnceAsync(string program, IReadOnlyList<string> args, Action<PackSession> wire, TimeSpan? timeout = null, CancellationToken ct = default)
    {
        if (!File.Exists(program)) return false;
        var psi = new ProcessStartInfo(program) { WorkingDirectory = Path.GetDirectoryName(program)!, UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, StandardOutputEncoding = Encoding.UTF8 };
        foreach (var a in args) psi.ArgumentList.Add(a);
        Process? p;
        try { p = Process.Start(psi); } catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { return false; }
        if (p is null) return false;
        using (p)
        {
            var reader = new PackSession(null);
            wire(reader);
            using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
            cts.CancelAfter(timeout ?? TimeSpan.FromSeconds(90));
            try
            {
                string? line;
                while ((line = await p.StandardOutput.ReadLineAsync(cts.Token).ConfigureAwait(false)) is not null)
                    if (line.Length is > 0 and <= MaxLine) reader.Handle(line);
                await p.WaitForExitAsync(cts.Token).ConfigureAwait(false);
                return true;
            }
            catch (OperationCanceledException)
            {
                try { if (!p.HasExited) p.Kill(); } catch (Exception ex) when (ex is InvalidOperationException or System.ComponentModel.Win32Exception) { }
                return false;
            }
        }
    }

    /// <summary>Which Steam friends are hosting, asked once with no session running: the pack's program run as <c>friends --steam-app N</c>. Null when it did not answer.</summary>
    public static async Task<SessionFriends?> FriendsOnceAsync(string program, uint steamAppId, string gameExe, CancellationToken ct = default)
    {
        var args = new List<string> { "friends", "--steam-app", steamAppId.ToString() };
        if (!string.IsNullOrWhiteSpace(gameExe) && gameExe.Length <= 260 && !gameExe.Any(char.IsControl)) { args.Add("--game-exe"); args.Add(gameExe); }
        SessionFriends? got = null;
        await RunOnceAsync(program, args, s => s.Friends += f => got = f, TimeSpan.FromSeconds(40), ct).ConfigureAwait(false);
        return got;
    }
    // ---- commands ----

    private void Send(object message)
    {
        if (_gone || _process is null) return;
        try { lock (_write) { _process.StandardInput.WriteLine(JsonSerializer.Serialize(message)); _process.StandardInput.Flush(); } }
        catch (Exception ex) when (ex is IOException or ObjectDisposedException or InvalidOperationException) { _gone = true; }
    }

    /// <summary>The player's settings the session needs; sent before a command that uses them, so a value the game's menu changed meanwhile is the current one.</summary>
    public void Configure(string gameExe, int hostPort, int infoPort, int versionPort, uint steamAppId, bool allowSteam, bool relayReuse, bool connectGate,
                          int injectDelaySeconds, bool menuTakeover, string agentPath, string relayPath, string dllPath, string startSaves) =>
        Send(new { t = "config", gameExe, hostPort, infoPort, versionPort, steamAppId, allowSteam, relayReuse, connectGate, injectDelaySeconds, menuTakeover, agentPath, relayPath, dllPath, startSaves });

    public void Relay() => Send(new { t = "relay" });
    public void RelayStop() => Send(new { t = "relay_stop" });
    public void Launch(string ip, int port, string? steamCode, int pingMs, bool skipModCheck) => Send(new { t = "launch", ip, port, steam = steamCode ?? "", ping = pingMs, skipModCheck });
    public void Connect() => Send(new { t = "connect" });
    public void Choice(string choice) => Send(new { t = "choice", choice });
    public void Fallback(string host, int port) => Send(new { t = "fallback", host, port });
    public void Cancel() => Send(new { t = "cancel" });
    public void AskFriends() => Send(new { t = "friends" });
    public void TestConnection(string ip, int port, string? steamCode) => Send(new { t = "test", ip, port, steam = steamCode ?? "" });
    public void CloseGame() => Send(new { t = "close_game" });

    public void Dispose()
    {
        if (_process is null) return;
        Send(new { t = "stop" });
        try { _process.StandardInput.Close(); } catch (Exception ex) when (ex is IOException or ObjectDisposedException or InvalidOperationException) { }
        try { if (!_process.WaitForExit(2000)) _process.Kill(); } catch (Exception ex) when (ex is InvalidOperationException or System.ComponentModel.Win32Exception) { }
        _process.Dispose();
    }
}
