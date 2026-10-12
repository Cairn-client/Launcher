// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Wire;

namespace Cairn.MasterServer.Features.Presence;

/// <summary>
/// Who is online with Cairn, and who may see and invite whom (the rules are in <see cref="PresenceApi"/>). In memory only: nothing about a
/// player outlives their socket. A player is visible to another only when each lists the other as a Steam friend.
/// </summary>
public sealed class PresenceHub
{
    public sealed class Peer
    {
        internal Peer(string id, string name, HashSet<string> friends, PresenceStatus status, Func<PresenceMessage, Task> send, Func<Task> close)
        { Id = id; Name = name; Friends = friends; Status = status; Send = send; Close = close; }

        public string Id { get; }
        public string Name { get; }
        internal HashSet<string> Friends { get; }
        internal PresenceStatus Status { get; set; }
        internal Func<PresenceMessage, Task> Send { get; }
        internal Func<Task> Close { get; }
        internal Queue<DateTimeOffset> Invites { get; } = new();
    }

    private readonly Dictionary<string, Peer> _peers = new(StringComparer.Ordinal);
    private readonly object _gate = new();

    public int Online { get { lock (_gate) return _peers.Count; } }

    private static bool Mutual(Peer a, Peer b) => a.Friends.Contains(b.Id) && b.Friends.Contains(a.Id);

    private static PresenceMessage FriendMessage(Peer p, bool online) => new()
    {
        Type = PresenceApi.MessageType.Friend,
        ApiVersion = PresenceApi.Version,
        Friend = new PresenceFriend { Id = p.Id, Name = p.Name, Online = online, Activity = p.Status.Activity, Game = p.Status.Game },
    };

    /// <summary>A hello: null and the peer when accepted (its friends already told), else the reason it was refused.</summary>
    public async Task<(Peer? Peer, string? Refused)> HelloAsync(PresenceHello h, Func<PresenceMessage, Task> send, Func<Task> close)
    {
        if (!PresenceApi.ValidId(h.Id)) return (null, "That is not a Steam id.");
        if (h.Friends.Length > PresenceApi.MaxFriends) return (null, "Too many friends listed.");
        string name = PresenceApi.CleanText(h.Name, PresenceApi.MaxNameLength);
        if (name.Length == 0) return (null, "A name is needed.");
        var friends = new HashSet<string>(StringComparer.Ordinal);
        foreach (var f in h.Friends) if (PresenceApi.ValidId(f) && f != h.Id) friends.Add(f);
        var status = CleanStatus(h.Status);
        var peer = new Peer(h.Id, name, friends, status, send, close);

        Peer? replaced;
        List<Peer> mutual;
        lock (_gate)
        {
            _peers.TryGetValue(h.Id, out replaced);
            _peers[h.Id] = peer;
            mutual = _peers.Values.Where(p => p != peer && Mutual(peer, p)).ToList();
        }
        if (replaced is not null) { try { await replaced.Close(); } catch { } }   // the same player signed in elsewhere: the older socket goes
        await SafeSend(peer, new PresenceMessage { Type = PresenceApi.MessageType.Welcome, ApiVersion = PresenceApi.Version });
        foreach (var p in mutual)
        {
            await SafeSend(peer, FriendMessage(p, true));
            await SafeSend(p, FriendMessage(peer, true));
        }
        return (peer, null);
    }

    public async Task StatusAsync(Peer peer, PresenceStatus status)
    {
        List<Peer> mutual;
        lock (_gate)
        {
            if (!_peers.TryGetValue(peer.Id, out var cur) || cur != peer) return;
            peer.Status = CleanStatus(status);
            mutual = _peers.Values.Where(p => p != peer && Mutual(peer, p)).ToList();
        }
        foreach (var p in mutual) await SafeSend(p, FriendMessage(peer, true));
    }

    /// <summary>An invitation: null when delivered, else the reason (sent back to the inviter).</summary>
    public async Task<string?> InviteAsync(Peer from, PresenceInvite invite, DateTimeOffset now)
    {
        if (!PresenceApi.ValidId(invite.To)) return "That is not a Steam id.";
        string game = PresenceApi.CleanText(invite.Game, PresenceApi.MaxGameLength);
        string code = PresenceApi.CleanText(invite.Code, PresenceApi.MaxCodeLength);
        if (code.Length == 0) return "An invitation needs a code.";
        Peer? to;
        lock (_gate)
        {
            if (!_peers.TryGetValue(from.Id, out var cur) || cur != from) return "You are not connected.";
            while (from.Invites.Count > 0 && now - from.Invites.Peek() > TimeSpan.FromMinutes(1)) from.Invites.Dequeue();
            if (from.Invites.Count >= PresenceApi.InvitesPerMinute) return "Too many invitations; wait a minute.";
            _peers.TryGetValue(invite.To, out to);
            if (to is null || !Mutual(from, to)) return "That friend is not online with Cairn.";
            from.Invites.Enqueue(now);
        }
        await SafeSend(to, new PresenceMessage
        {
            Type = PresenceApi.MessageType.Invited,
            ApiVersion = PresenceApi.Version,
            Invited = new PresenceInvited { From = from.Id, Name = from.Name, Game = game, Code = code },
        });
        return null;
    }

    public async Task LeaveAsync(Peer peer)
    {
        List<Peer> mutual;
        lock (_gate)
        {
            if (!_peers.TryGetValue(peer.Id, out var cur) || cur != peer) return;   // already replaced by a newer socket
            _peers.Remove(peer.Id);
            mutual = _peers.Values.Where(p => Mutual(peer, p)).ToList();
        }
        foreach (var p in mutual) await SafeSend(p, FriendMessage(peer, false));
    }

    private static PresenceStatus CleanStatus(PresenceStatus s) => new()
    {
        Activity = PresenceApi.Activity.Valid(s.Activity) ? s.Activity : PresenceApi.Activity.Online,
        Game = PresenceApi.CleanText(s.Game, PresenceApi.MaxGameLength),
    };

    private static async Task SafeSend(Peer p, PresenceMessage m)
    {
        try { await p.Send(m); } catch { /* a socket that cannot be written to is closing; its own loop ends it */ }
    }
}
