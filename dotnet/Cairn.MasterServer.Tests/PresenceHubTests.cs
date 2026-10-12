// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.MasterServer.Features.Presence;
using Cairn.Wire;
using Xunit;

namespace Cairn.MasterServer.Tests;

public class PresenceHubTests
{
    private const string A = "76561198000000001", B = "76561198000000002", C = "76561198000000003";

    private sealed class Client
    {
        public readonly List<PresenceMessage> Got = new();
        public bool Closed;
        public PresenceHub.Peer? Peer;
        public Task Send(PresenceMessage m) { lock (Got) Got.Add(m); return Task.CompletedTask; }
        public Task Close() { Closed = true; return Task.CompletedTask; }
        public IEnumerable<PresenceFriend> Friends => Got.Where(m => m.Type == PresenceApi.MessageType.Friend).Select(m => m.Friend!);
    }

    private static async Task<Client> Join(PresenceHub hub, string id, string name, string[] friends, string activity = "online", string game = "")
    {
        var c = new Client();
        var (peer, refused) = await hub.HelloAsync(new PresenceHello { Id = id, Name = name, Friends = friends, Status = new PresenceStatus { Activity = activity, Game = game } }, c.Send, c.Close);
        Assert.Null(refused);
        c.Peer = peer;
        return c;
    }

    [Fact]
    public async Task Mutual_friends_see_each_other_come_online()
    {
        var hub = new PresenceHub();
        var a = await Join(hub, A, "Alice", [B]);
        var b = await Join(hub, B, "Bob", [A], "playing", "Kingdom Come: Deliverance II");
        Assert.Contains(a.Got, m => m.Type == PresenceApi.MessageType.Welcome);
        Assert.Contains(a.Friends, f => f.Id == B && f.Online && f.Activity == "playing" && f.Game == "Kingdom Come: Deliverance II");
        Assert.Contains(b.Friends, f => f.Id == A && f.Online);
    }

    [Fact]
    public async Task A_one_sided_friendship_shows_nothing()
    {
        var hub = new PresenceHub();
        var a = await Join(hub, A, "Alice", [B]);
        var b = await Join(hub, B, "Bob", [C]);   // Bob does not list Alice
        Assert.Empty(a.Friends);
        Assert.Empty(b.Friends);
    }

    [Fact]
    public async Task A_status_change_reaches_friends_and_a_leave_says_offline()
    {
        var hub = new PresenceHub();
        var a = await Join(hub, A, "Alice", [B]);
        var b = await Join(hub, B, "Bob", [A]);
        await hub.StatusAsync(b.Peer!, new PresenceStatus { Activity = "hosting", Game = "KCD2" });
        Assert.Contains(a.Friends, f => f.Id == B && f.Activity == "hosting" && f.Game == "KCD2");
        await hub.LeaveAsync(b.Peer!);
        Assert.Contains(a.Friends, f => f.Id == B && !f.Online);
    }

    [Fact]
    public async Task An_invitation_reaches_only_an_online_mutual_friend()
    {
        var hub = new PresenceHub();
        var a = await Join(hub, A, "Alice", [B, C]);
        var b = await Join(hub, B, "Bob", [A]);
        var c = await Join(hub, C, "Carol", []);   // does not list Alice back
        Assert.Null(await hub.InviteAsync(a.Peer!, new PresenceInvite { To = B, Game = "KCD2", Code = "ABC123" }, DateTimeOffset.UtcNow));
        var inv = b.Got.Single(m => m.Type == PresenceApi.MessageType.Invited).Invited!;
        Assert.Equal((A, "Alice", "KCD2", "ABC123"), (inv.From, inv.Name, inv.Game, inv.Code));
        Assert.NotNull(await hub.InviteAsync(a.Peer!, new PresenceInvite { To = C, Code = "x" }, DateTimeOffset.UtcNow));
        Assert.DoesNotContain(c.Got, m => m.Type == PresenceApi.MessageType.Invited);
        Assert.NotNull(await hub.InviteAsync(a.Peer!, new PresenceInvite { To = B, Code = "" }, DateTimeOffset.UtcNow));
    }

    [Fact]
    public async Task Invitations_are_rate_limited()
    {
        var hub = new PresenceHub();
        var a = await Join(hub, A, "Alice", [B]);
        await Join(hub, B, "Bob", [A]);
        var now = DateTimeOffset.UtcNow;
        for (int i = 0; i < PresenceApi.InvitesPerMinute; i++) Assert.Null(await hub.InviteAsync(a.Peer!, new PresenceInvite { To = B, Code = "c" }, now));
        Assert.NotNull(await hub.InviteAsync(a.Peer!, new PresenceInvite { To = B, Code = "c" }, now));
        Assert.Null(await hub.InviteAsync(a.Peer!, new PresenceInvite { To = B, Code = "c" }, now.AddMinutes(2)));
    }

    [Fact]
    public async Task Signing_in_again_replaces_the_older_socket()
    {
        var hub = new PresenceHub();
        var first = await Join(hub, A, "Alice", [B]);
        var second = await Join(hub, A, "Alice", [B]);
        Assert.True(first.Closed);
        await hub.LeaveAsync(first.Peer!);   // the old socket's own cleanup must not take the new one offline
        Assert.Equal(1, hub.Online);
        Assert.False(second.Closed);
    }

    [Theory]
    [InlineData("1234", "Alice")]
    [InlineData("76561198000000001", "  ")]
    public async Task A_bad_hello_is_refused(string id, string name)
    {
        var hub = new PresenceHub();
        var (peer, refused) = await hub.HelloAsync(new PresenceHello { Id = id, Name = name }, new Client().Send, new Client().Close);
        Assert.Null(peer);
        Assert.NotNull(refused);
    }

    [Fact]
    public void Ids_and_text_are_cleaned_for_the_wire()
    {
        Assert.True(PresenceApi.ValidId(A));
        Assert.False(PresenceApi.ValidId("76561198000000"));
        Assert.False(PresenceApi.ValidId("1234567890123456a"));
        Assert.Equal("Bob", PresenceApi.CleanText("  Bo\u0007b ", 48));
        Assert.Equal(5, PresenceApi.CleanText("abcdefgh", 5).Length);
    }
}

public class PresenceNoticeTests
{
    private static PresenceFriend F(bool online, string activity = "online", string game = "") => new() { Id = "76561198000000002", Name = "Bob", Online = online, Activity = activity, Game = game };

    [Fact]
    public void A_friend_coming_online_is_announced_with_the_game_when_there_is_one()
    {
        Assert.Equal("Bob is online", PresenceNotices.ForFriend(null, F(true)));
        Assert.Equal("Bob is online and playing KCD2", PresenceNotices.ForFriend(null, F(true, "playing", "KCD2")));
        Assert.Equal("Bob is online and hosting KCD2", PresenceNotices.ForFriend(F(false), F(true, "hosting", "KCD2")));
    }

    [Fact]
    public void A_friend_starting_a_game_is_announced_once()
    {
        Assert.Equal("Bob is playing KCD2", PresenceNotices.ForFriend(F(true), F(true, "playing", "KCD2")));
        Assert.Null(PresenceNotices.ForFriend(F(true, "playing", "KCD2"), F(true, "playing", "KCD2")));
        Assert.Equal("Bob is hosting KCD2", PresenceNotices.ForFriend(F(true, "playing", "KCD2"), F(true, "hosting", "KCD2")));
    }

    [Fact]
    public void Going_offline_or_stopping_says_nothing()
    {
        Assert.Null(PresenceNotices.ForFriend(F(true), F(false)));
        Assert.Null(PresenceNotices.ForFriend(F(true, "playing", "KCD2"), F(true)));
    }

    [Fact]
    public void An_invitation_names_who_and_what()
    {
        Assert.Equal("Bob is inviting you to play KCD2", PresenceNotices.ForInvite(new PresenceInvited { Name = "Bob", Game = "KCD2", Code = "x" }));
        Assert.Equal("Bob is inviting you to play", PresenceNotices.ForInvite(new PresenceInvited { Name = "Bob", Code = "x" }));
    }
}
