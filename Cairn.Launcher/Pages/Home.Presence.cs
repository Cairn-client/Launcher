// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Launcher.Models;
using Cairn.Launcher.Services;
using Serilog;
using Cairn.Setup;
using Cairn.Wire;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// Friends on Cairn: a notice when a Steam friend who also runs Cairn comes online or starts playing, an invitation that can be accepted
    /// from the corner of the launcher, and an Invite button while this player hosts. The master server carries it (PresenceApi); nothing
    /// is shown for a friend who does not also list this player as a friend.
    /// </summary>
    public partial class Home
    {
        private PresenceService? presence;
        private readonly Dictionary<string, PresenceFriend> friendSeen = new();
        private bool showInvite;
        private string inviteNote = "";

        private bool PresenceOn => presence is { Enabled: true };

        private void StartPresence()
        {
            if (!settings.PresenceEnabled || presence is not null) return;
            var service = new PresenceService(settings.MasterServerUrl, ReadSteamIdentity);
            if (!service.Enabled)
            {
                Log.Information("presence: the master server is this machine, so there are no friends to see; off");
                service.Dispose();
                return;
            }
            presence = service;
            service.FriendChanged += OnPresenceFriend;
            service.Invited += OnPresenceInvited;
            service.ConnectionChanged += on => { Log.Information("presence: {State}", on ? "connected" : "disconnected"); _ = InvokeAsync(StateHasChanged); };
            service.Start();
        }

        private static (string Id, string Name, IReadOnlyList<string> Friends)? ReadSteamIdentity()
        {
            var me = SteamIdentity.Read(RealSetupHost.Instance.SteamRootFromRegistry());
            return me is null ? null : (me.Id, me.Name, me.Friends);
        }

        private void StopPresence()
        {
            presence?.Dispose();
            presence = null;
        }

        private void OnPresenceFriend(PresenceFriend f)
        {
            friendSeen.TryGetValue(f.Id, out var before);
            if (f.Online) friendSeen[f.Id] = f; else friendSeen.Remove(f.Id);
            // a friend who starts hosting: look for their code now rather than at the next minute, so the Join button is there at once
            if (f.Online && f.Activity == PresenceApi.Activity.Hosting && before?.Activity != f.Activity) { homeFriendsAt = DateTime.MinValue; _ = RefreshHomeFriendsIfStaleAsync(); }
            if (!settings.NotifyFriends) return;
            if (PresenceNotices.ForFriend(before, f) is { } text)
            {
                if (f.Online && f.Activity == PresenceApi.Activity.Hosting) ShowToast(text, error: false, action: "Join", onAction: () => JoinHostingFriendAsync(f), milliseconds: 30_000);
                else ShowToast(text);
            }
            _ = InvokeAsync(StateHasChanged);
        }

        /// <summary>The Join button of a "friend is hosting" notice: the friend's code is looked up again, then the Join window opens with it.</summary>
        private async Task JoinHostingFriendAsync(PresenceFriend f)
        {
            homeFriendsAt = DateTime.MinValue;
            await RefreshHomeFriendsIfStaleAsync();
            var hosting = homeFriends?.FirstOrDefault(h => string.Equals(h.Name, f.Name, StringComparison.OrdinalIgnoreCase));
            if (hosting is null) { ShowToast($"{f.Name}'s code is not ready yet. Try again in a moment.", true); return; }
            await JoinFriendFromHomeAsync(hosting);
        }

        private void OnPresenceInvited(PresenceInvited i)
        {
            if (!settings.NotifyFriends) return;
            string text = PresenceNotices.ForInvite(i);
            ShowToast(text, error: false, action: "Join", onAction: () => AcceptInviteAsync(i), milliseconds: 60_000);
        }

        private async Task AcceptInviteAsync(PresenceInvited i)
        {
            if (gameRunning) { ShowToast("The game is already running. Close it first, then accept the invitation again.", true); return; }
            Log.Information("presence: accepting an invitation to {Game}", i.Game);
            steamCodeInput = i.Code;
            await JoinThroughSteamAsync();
        }

        /// <summary>What this player is doing, from the game this launcher started (online when nothing runs).</summary>
        private void PublishPresenceStatus()
        {
            if (presence is null) return;
            string game = SelectedProfile?.DisplayName ?? "";
            if (launchedGameId.Length > 0 && GameProfileRegistry.Get(launchedGameId) is { } g) game = g.DisplayName;
            string activity = !gameRunning ? PresenceApi.Activity.Online : launchedAs == "host" ? PresenceApi.Activity.Hosting : PresenceApi.Activity.Playing;
            _ = presence.SetStatusAsync(activity, gameRunning ? game : "");
        }

        private void OpenInvite()
        {
            inviteNote = string.IsNullOrWhiteSpace(hostSteamCode) ? "Your host code is not ready yet." : "";
            showInvite = true;
        }

        private async Task InviteFriendAsync(PresenceFriend f)
        {
            if (presence is null || string.IsNullOrWhiteSpace(hostSteamCode)) { inviteNote = "Your host code is not ready yet."; return; }
            string game = launchedGameId.Length > 0 && GameProfileRegistry.Get(launchedGameId) is { } g ? g.DisplayName : SelectedProfile?.DisplayName ?? "";
            await presence.InviteAsync(f.Id, game, hostSteamCode);
            ShowToast($"Invitation sent to {f.Name}");
            showInvite = false;
        }
    }
}
