// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.IO.Pipes;
using System.Security.Cryptography;
using System.Text;

namespace Cairn.Setup;

/// <summary>
/// The launcher's end of the bridge (<see cref="BridgeProtocol"/>): a named pipe, open to the current user only, with a name nobody can
/// guess. A game's companion program connects, says hello, and from then on sends the menu and the notices; the launcher sends back what
/// the player did. One bridge at a time; if it goes away and comes back (the companion restarted) the new connection takes over.
/// </summary>
public sealed class BridgeServer : IDisposable
{
    private readonly CancellationTokenSource _cts = new();
    private readonly object _gate = new();
    private Stream? _stream;
    private readonly Task _loop;

    /// <summary>The pipe's name (without <c>\\.\pipe\</c>); given to the companion in <see cref="BridgeProtocol.PipeEnvironmentVariable"/>.</summary>
    public string PipeName { get; }

    /// <summary>The game id the connected bridge said it belongs to; empty while none is connected.</summary>
    public string Game { get; private set; } = "";

    public bool Connected { get { lock (_gate) return _stream is not null && Game.Length > 0; } }

    /// <summary>A message arrived (never a hello: that is handled here). Raised on a pool thread.</summary>
    public event Action<BridgeIn>? Received;

    /// <summary>A bridge said hello and was welcomed (its game id), or went away (empty). Raised on a pool thread.</summary>
    public event Action<string>? ConnectionChanged;

    public BridgeServer()
    {
        PipeName = "cairn-bridge-" + Convert.ToHexString(RandomNumberGenerator.GetBytes(16)).ToLowerInvariant();
        _loop = Task.Run(() => RunAsync(_cts.Token));
    }

    /// <summary>Sends one message (a line of the protocol) to the connected bridge; false when none is connected or the write failed.</summary>
    public bool Send(string line)
    {
        Stream? s;
        lock (_gate) s = Game.Length > 0 ? _stream : null;
        if (s is null) return false;
        try
        {
            var bytes = Encoding.UTF8.GetBytes(line + "\n");
            lock (s) { if (!s.WriteAsync(bytes).AsTask().Wait(3000)) return false; }   // async: a synchronous write on a pipe with a read pending can wait forever
            return true;
        }
        catch (Exception ex) when (ex is IOException or ObjectDisposedException or AggregateException) { return false; }
    }

    private async Task RunAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            NamedPipeServerStream? pipe = null;
            try
            {
                pipe = new NamedPipeServerStream(PipeName, PipeDirection.InOut, 1, PipeTransmissionMode.Byte, PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly, 64 * 1024, 64 * 1024);   // buffered: a write does not wait for the other side to read
                await pipe.WaitForConnectionAsync(ct).ConfigureAwait(false);
                await ServeAsync(pipe, ct).ConfigureAwait(false);
            }
            catch (OperationCanceledException) { break; }
            catch (Exception ex) when (ex is IOException or ObjectDisposedException or UnauthorizedAccessException) { /* a bridge that went away: wait for the next one */ }
            finally
            {
                bool had;
                lock (_gate) { had = Game.Length > 0; _stream = null; Game = ""; }
                try { pipe?.Dispose(); } catch (Exception) { }
                if (had) { try { ConnectionChanged?.Invoke(""); } catch (Exception) { } }
            }
            try { await Task.Delay(100, ct).ConfigureAwait(false); } catch (OperationCanceledException) { break; }
        }
    }

    private async Task ServeAsync(NamedPipeServerStream pipe, CancellationToken ct)
    {
        lock (_gate) { _stream = pipe; Game = ""; }
        var buf = new byte[4096];
        var line = new List<byte>(256);
        bool greeted = false;
        while (!ct.IsCancellationRequested)
        {
            int n = await pipe.ReadAsync(buf, ct).ConfigureAwait(false);
            if (n == 0) return;
            for (int i = 0; i < n; i++)
            {
                if (buf[i] != (byte)'\n')
                {
                    line.Add(buf[i]);
                    if (line.Count > BridgeProtocol.MaxLine) return;   // a line that never ends: the connection ends
                    continue;
                }
                string text = Encoding.UTF8.GetString(line.ToArray()).TrimEnd('\r');
                line.Clear();
                if (text.Length == 0) continue;
                var msg = BridgeProtocol.Parse(text);
                if (!greeted)
                {
                    if (msg is not BridgeHello hello) return;   // the first thing a bridge says is hello
                    if (hello.Protocol > BridgeProtocol.Version)
                    {
                        WriteDirect(pipe, BridgeProtocol.Bye("the mod needs a newer launcher"));
                        return;
                    }
                    greeted = true;
                    WriteDirect(pipe, BridgeProtocol.Welcome());
                    lock (_gate) Game = hello.Game;
                    try { ConnectionChanged?.Invoke(hello.Game); } catch (Exception) { }
                    continue;
                }
                if (msg is null or BridgeHello) continue;
                try { Received?.Invoke(msg); } catch (Exception) { /* a listener's fault is not the bridge's */ }
            }
        }
    }

    private static void WriteDirect(Stream s, string line)
    {
        var bytes = Encoding.UTF8.GetBytes(line + "\n");
        lock (s) { s.Write(bytes, 0, bytes.Length); s.Flush(); }
    }

    public void Dispose()
    {
        _cts.Cancel();
        lock (_gate) { try { _stream?.Dispose(); } catch (Exception) { } }
        try { _loop.Wait(1000); } catch (Exception) { }
        _cts.Dispose();
    }
}
