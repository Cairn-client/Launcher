// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Cairn.Wire;

/// <summary>
/// The presence contract: who is online with Cairn and what they play, and the invitations between friends.
///
/// A launcher holds one WebSocket to the master server for as long as it is open. It says who it is (a Steam id and a display name), which
/// Steam friends it has, and what it is doing; the master tells it which of those friends are online and what they do, and carries an
/// invitation from one player to another. Friendship must be mutual: a player sees another only when each has the other in their own
/// friends list, so a stranger who guesses an id learns nothing and cannot invite anyone.
/// </summary>
public static class PresenceApi
{
    /// <summary>Version of the messages below; the master refuses a hello that does not match.</summary>
    public const int Version = 1;

    public const string ConnectPath = "/api/v1/presence/connect";

    /// <summary>How often a launcher sends something (a status or a bare ping); the master drops a socket silent for <see cref="TimeoutSeconds"/>.</summary>
    public const int PingSeconds = 20;
    public const int TimeoutSeconds = 70;

    public const int MaxFriends = 2000;
    public const int MaxMessageBytes = 64 * 1024;
    public const int MaxNameLength = 48;
    public const int MaxGameLength = 64;
    public const int MaxCodeLength = 256;

    /// <summary>The most invitations one player may send in a minute.</summary>
    public const int InvitesPerMinute = 6;

    public static readonly JsonSerializerOptions Json = new(JsonSerializerDefaults.Web) { DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull };

    public static class MessageType
    {
        /// <summary>Launcher to master, first message: who I am and who my friends are.</summary>
        public const string Hello = "hello";
        /// <summary>Launcher to master: what I am doing now.</summary>
        public const string Status = "status";
        /// <summary>Launcher to master: invite this friend.</summary>
        public const string Invite = "invite";
        /// <summary>Launcher to master: still here.</summary>
        public const string Ping = "ping";
        /// <summary>Master to launcher: the hello was accepted.</summary>
        public const string Welcome = "welcome";
        /// <summary>Master to launcher: a friend came online, went offline or changed what they do.</summary>
        public const string Friend = "friend";
        /// <summary>Master to launcher: a friend invites you.</summary>
        public const string Invited = "invited";
        /// <summary>Master to launcher: something you sent was refused (the socket stays open unless the hello was refused).</summary>
        public const string Refused = "refused";
    }

    /// <summary>What a player is doing; the launcher shows "X is online" and "X is playing GAME".</summary>
    public static class Activity
    {
        public const string Online = "online";
        public const string Playing = "playing";
        public const string Hosting = "hosting";

        public static bool Valid(string? a) => a is Online or Playing or Hosting;
    }

    /// <summary>A Steam id as the contract carries it: 17 digits starting with 7656119 (an individual account).</summary>
    public static bool ValidId(string? id)
    {
        if (id is null || id.Length != 17 || !id.StartsWith("7656119", StringComparison.Ordinal)) return false;
        foreach (char c in id) if (c is < '0' or > '9') return false;
        return true;
    }

    /// <summary>A display name cleaned for the wire: control characters out, trimmed, cut to <see cref="MaxNameLength"/>.</summary>
    public static string CleanText(string? s, int max)
    {
        if (string.IsNullOrWhiteSpace(s)) return "";
        var sb = new System.Text.StringBuilder(Math.Min(s.Length, max));
        foreach (char c in s)
        {
            if (char.IsControl(c)) continue;
            sb.Append(c);
            if (sb.Length >= max) break;
        }
        return sb.ToString().Trim();
    }
}

public sealed record PresenceMessage
{
    public string Type { get; init; } = "";
    public int ApiVersion { get; init; }

    public PresenceHello? Hello { get; init; }
    public PresenceStatus? Status { get; init; }
    public PresenceInvite? Invite { get; init; }
    public PresenceFriend? Friend { get; init; }
    public PresenceInvited? Invited { get; init; }
    public string? Reason { get; init; }
}

public sealed record PresenceHello
{
    /// <summary>This player's Steam id.</summary>
    public string Id { get; init; } = "";
    public string Name { get; init; } = "";
    /// <summary>The Steam ids of this player's Steam friends (the master only reveals those that list this player back).</summary>
    public string[] Friends { get; init; } = [];
    public PresenceStatus Status { get; init; } = new();
}

public sealed record PresenceStatus
{
    /// <summary>One of <see cref="PresenceApi.Activity"/>.</summary>
    public string Activity { get; init; } = PresenceApi.Activity.Online;
    /// <summary>The game being played ("Kingdom Come: Deliverance II"), empty when only online.</summary>
    public string Game { get; init; } = "";
}

public sealed record PresenceInvite
{
    public string To { get; init; } = "";
    public string Game { get; init; } = "";
    /// <summary>What the invited launcher needs to join (the host code); opaque to the master.</summary>
    public string Code { get; init; } = "";
}

public sealed record PresenceFriend
{
    public string Id { get; init; } = "";
    public string Name { get; init; } = "";
    public bool Online { get; init; }
    public string Activity { get; init; } = PresenceApi.Activity.Online;
    public string Game { get; init; } = "";
}

public sealed record PresenceInvited
{
    public string From { get; init; } = "";
    public string Name { get; init; } = "";
    public string Game { get; init; } = "";
    public string Code { get; init; } = "";
}

/// <summary>The words of the launcher's notifications, in one place so they can be tested.</summary>
public static class PresenceNotices
{
    /// <summary>The notice for a friend's change, or null when it is not worth one (offline, or nothing new).</summary>
    public static string? ForFriend(PresenceFriend? before, PresenceFriend now)
    {
        if (!now.Online) return null;
        bool active = now.Activity is PresenceApi.Activity.Playing or PresenceApi.Activity.Hosting && now.Game.Length > 0;
        string verb = now.Activity == PresenceApi.Activity.Hosting ? "hosting" : "playing";
        if (before is not { Online: true })
            return active ? $"{now.Name} is online and {verb} {now.Game}" : $"{now.Name} is online";
        bool changed = active && (before.Game != now.Game || before.Activity != now.Activity);
        return changed ? $"{now.Name} is {verb} {now.Game}" : null;
    }

    public static string ForInvite(PresenceInvited i) =>
        i.Game.Length > 0 ? $"{i.Name} is inviting you to play {i.Game}" : $"{i.Name} is inviting you to play";
}
