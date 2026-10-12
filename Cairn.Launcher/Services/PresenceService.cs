// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using System.Collections.Concurrent;
using System.IO;
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using Cairn.Wire;

namespace Cairn.Launcher.Services
{
    /// <summary>
    /// The launcher's end of the presence feature (<see cref="PresenceApi"/>): one WebSocket to the master server for as long as the launcher
    /// is open. It tells the master who this player is and what they do, and raises events when a Steam friend who also runs Cairn comes
    /// online, changes what they play, or invites this player.
    /// </summary>
    public sealed class PresenceService : IDisposable
    {
        /// <summary>Who we are and which friends we have, read again at every connect (the friends list changes).</summary>
        public delegate (string Id, string Name, IReadOnlyList<string> Friends)? IdentityProvider();

        private readonly IdentityProvider _identity;
        private readonly string _url;
        private readonly CancellationTokenSource _cts = new();
        private readonly ConcurrentDictionary<string, PresenceFriend> _friends = new();
        private readonly SemaphoreSlim _sendGate = new(1, 1);
        private ClientWebSocket? _socket;
        private PresenceStatus _status = new();

        public PresenceService(string masterUrl, IdentityProvider identity)
        {
            _url = SocketUrl(masterUrl) ?? "";
            _identity = identity;
        }

        public event Action<PresenceFriend>? FriendChanged;
        public event Action<PresenceInvited>? Invited;
        public event Action<bool>? ConnectionChanged;

        public bool Connected { get; private set; }

        /// <summary>The master's address for presence, or null when it is this machine (a default install has no shared master: nobody else is there to see).</summary>
        public static string? SocketUrl(string? masterUrl)
        {
            if (string.IsNullOrWhiteSpace(masterUrl) || !Uri.TryCreate(masterUrl.Trim(), UriKind.Absolute, out var u)) return null;
            if (u.IsLoopback) return null;
            string scheme = u.Scheme switch { "https" or "wss" => "wss", "http" or "ws" => "ws", _ => "" };
            if (scheme.Length == 0) return null;
            return $"{scheme}://{u.Authority}{PresenceApi.ConnectPath}";
        }

        public bool Enabled => _url.Length > 0;

        public IReadOnlyList<PresenceFriend> OnlineFriends => _friends.Values.Where(f => f.Online).OrderBy(f => f.Name, StringComparer.OrdinalIgnoreCase).ToList();

        public void Start()
        {
            if (!Enabled) return;
            _ = Task.Run(RunAsync);
        }

        /// <summary>What this player is doing now; sent at once when connected and again at the next connect.</summary>
        public async Task SetStatusAsync(string activity, string game)
        {
            var s = new PresenceStatus { Activity = PresenceApi.Activity.Valid(activity) ? activity : PresenceApi.Activity.Online, Game = game ?? "" };
            if (s == _status) return;
            _status = s;
            await SendAsync(new PresenceMessage { Type = PresenceApi.MessageType.Status, ApiVersion = PresenceApi.Version, Status = s });
        }

        public Task InviteAsync(string friendId, string game, string code) =>
            SendAsync(new PresenceMessage { Type = PresenceApi.MessageType.Invite, ApiVersion = PresenceApi.Version, Invite = new PresenceInvite { To = friendId, Game = game, Code = code } });

        private async Task RunAsync()
        {
            int backoff = 5;
            while (!_cts.IsCancellationRequested)
            {
                try
                {
                    var me = _identity();
                    if (me is null) { await Task.Delay(TimeSpan.FromSeconds(60), _cts.Token); continue; }
                    using var socket = new ClientWebSocket();
                    _socket = socket;
                    await socket.ConnectAsync(new Uri(_url), _cts.Token);
                    await SendAsync(new PresenceMessage
                    {
                        Type = PresenceApi.MessageType.Hello, ApiVersion = PresenceApi.Version,
                        Hello = new PresenceHello { Id = me.Value.Id, Name = me.Value.Name, Friends = me.Value.Friends.ToArray(), Status = _status },
                    });
                    backoff = 5;
                    using var ping = new CancellationTokenSource();
                    var pinger = PingLoopAsync(ping.Token);
                    try { await ReceiveLoopAsync(socket); }
                    finally { ping.Cancel(); }
                    await pinger;
                }
                catch (OperationCanceledException) { break; }
                catch (Exception ex) { Log.Debug(ex, "presence: connection ended"); }
                finally { SetConnected(false); _friends.Clear(); }
                try { await Task.Delay(TimeSpan.FromSeconds(backoff), _cts.Token); } catch { break; }
                backoff = Math.Min(backoff * 2, 120);
            }
        }

        private async Task ReceiveLoopAsync(ClientWebSocket socket)
        {
            var buffer = new byte[4096];
            while (socket.State == WebSocketState.Open && !_cts.IsCancellationRequested)
            {
                using var ms = new MemoryStream();
                WebSocketReceiveResult r;
                do
                {
                    r = await socket.ReceiveAsync(buffer, _cts.Token);
                    if (r.MessageType == WebSocketMessageType.Close) return;
                    ms.Write(buffer, 0, r.Count);
                    if (ms.Length > PresenceApi.MaxMessageBytes) return;
                } while (!r.EndOfMessage);
                PresenceMessage? m;
                try { m = JsonSerializer.Deserialize<PresenceMessage>(Encoding.UTF8.GetString(ms.GetBuffer(), 0, (int)ms.Length), PresenceApi.Json); }
                catch (JsonException) { continue; }
                if (m is not null) Handle(m);
            }
        }

        /// <summary>One message from the master (public so the rules can be tested without a socket).</summary>
        public void Handle(PresenceMessage m)
        {
            switch (m.Type)
            {
                case PresenceApi.MessageType.Welcome:
                    SetConnected(true);
                    break;
                case PresenceApi.MessageType.Friend when m.Friend is { } f && PresenceApi.ValidId(f.Id):
                    if (f.Online) _friends[f.Id] = f; else _friends.TryRemove(f.Id, out _);
                    FriendChanged?.Invoke(f);
                    break;
                case PresenceApi.MessageType.Invited when m.Invited is { } i && PresenceApi.ValidId(i.From) && i.Code.Length > 0:
                    Invited?.Invoke(i);
                    break;
                case PresenceApi.MessageType.Refused:
                    Log.Information("presence: the master said no: {Reason}", m.Reason);
                    break;
            }
        }

        private async Task PingLoopAsync(CancellationToken ct)
        {
            try
            {
                while (!ct.IsCancellationRequested)
                {
                    await Task.Delay(TimeSpan.FromSeconds(PresenceApi.PingSeconds), ct);
                    await SendAsync(new PresenceMessage { Type = PresenceApi.MessageType.Ping, ApiVersion = PresenceApi.Version });
                }
            }
            catch (OperationCanceledException) { }
        }

        private async Task SendAsync(PresenceMessage m)
        {
            var socket = _socket;
            if (socket is null || socket.State != WebSocketState.Open) return;
            var bytes = JsonSerializer.SerializeToUtf8Bytes(m, PresenceApi.Json);
            await _sendGate.WaitAsync();
            try { await socket.SendAsync(bytes, WebSocketMessageType.Text, true, _cts.Token); }
            catch (Exception ex) when (ex is WebSocketException or OperationCanceledException or ObjectDisposedException) { }
            finally { _sendGate.Release(); }
        }

        private void SetConnected(bool on)
        {
            if (Connected == on) return;
            Connected = on;
            ConnectionChanged?.Invoke(on);
        }

        public void Dispose()
        {
            _cts.Cancel();
            try { _socket?.Dispose(); } catch { }
        }
    }
}
