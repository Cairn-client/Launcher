// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using System.Net.Http;
using System.Xml.Linq;

namespace Cairn.Setup;

/// <summary>
/// What a Steam friend is doing now, read from the friend's public Steam Community profile (steamcommunity.com/profiles/&lt;id&gt;?xml=1: no key,
/// no sign-in). A profile that is not public says nothing, and the friend shows as unknown.
/// </summary>
public static class SteamProfileStatus
{
    public const string Online = "online", InGame = "in-game", Offline = "offline", Unknown = "unknown";

    /// <param name="State">online, in-game, offline or unknown</param>
    /// <param name="Game">The game being played, when in-game.</param>
    /// <param name="Message">Steam's own line: "Online", "Away", "Last Online 2 hrs, 5 mins ago"...</param>
    public sealed record Status(string State, string Game, string Message);

    public static Status NoStatus { get; } = new(Unknown, "", "");

    /// <summary>One profile's XML -> the status (Unknown for anything that is not a readable public profile).</summary>
    public static Status Parse(string xml)
    {
        try
        {
            var p = XDocument.Parse(xml).Root;
            if (p is null || p.Name != "profile") return NoStatus;
            string state = (string?)p.Element("onlineState") ?? "";
            if (state is not (Online or InGame or Offline)) return NoStatus;
            string game = (string?)p.Element("inGameInfo")?.Element("gameName") ?? "";
            string message = Plain((string?)p.Element("stateMessage") ?? "");
            return new Status(state == InGame && game.Length == 0 ? Online : state, game, message);
        }
        catch { return NoStatus; }
    }

    /// <summary>"In-Game&lt;br/&gt;Kingdom Come" -> "In-Game": the first line, without markup.</summary>
    private static string Plain(string s)
    {
        int cut = s.IndexOf('<');
        return (cut >= 0 ? s[..cut] : s).Trim();
    }

    public static async Task<Status> FetchAsync(HttpClient http, string steamId, CancellationToken ct)
    {
        try
        {
            string xml = await http.GetStringAsync($"https://steamcommunity.com/profiles/{steamId}/?xml=1", ct);
            return Parse(xml);
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException) { return NoStatus; }
    }
}
