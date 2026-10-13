// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Setup;
using Xunit;

namespace Cairn.Setup.Tests;

public class SteamIdentityTests
{
    private const string Users = """
        "users"
        {
            "76561198000000010"
            {
                "AccountName"   "old"
                "PersonaName"   "Old Name"
                "MostRecent"    "0"
            }
            "76561198000000001"
            {
                "AccountName"   "alice"
                "PersonaName"   "Alice \"the\" Brave"
                "RememberPassword"  "1"
                "MostRecent"    "1"
            }
        }
        """;

    private const string Config = """
        "UserLocalConfigStore"
        {
            "Software" { "Valve" { "Steam" { "Apps" { "1771300" { "LastPlayed" "5" } } } } }
            "friends"
            {
                "PersonaName"   "Alice"
                "5"
                {
                    "name"  "Bob"
                    "tag"   ""
                }
                "37"
                {
                    "name"  "Carol"
                }
                "0"
                {
                    "name"  "nobody"
                }
            }
        }
        """;

    [Fact]
    public void The_most_recent_account_and_its_name_are_read()
    {
        var u = SteamIdentity.MostRecentUser(Users);
        Assert.NotNull(u);
        Assert.Equal("76561198000000001", u!.Value.Id);
        Assert.Equal("Alice \"the\" Brave", u.Value.Name);
    }

    [Fact]
    public void No_most_recent_account_means_nobody() =>
        Assert.Null(SteamIdentity.MostRecentUser("\"users\" { \"76561198000000010\" { \"PersonaName\" \"x\" \"MostRecent\" \"0\" } }"));

    [Fact]
    public void Friends_are_the_numbered_sections_as_64_bit_ids()
    {
        var f = SteamIdentity.FriendIds(Config);
        Assert.Equal([(SteamIdentity.IndividualBase + 5).ToString(), (SteamIdentity.IndividualBase + 37).ToString()], f);
    }

    [Fact]
    public void Friend_names_come_with_their_ids()
    {
        var n = SteamIdentity.FriendNames(Config);
        Assert.Equal(["Bob", "Carol"], n.Select(x => x.Name));
        Assert.Equal((SteamIdentity.IndividualBase + 5).ToString(), n[0].Id);
    }

    [Theory]
    [InlineData("<profile><onlineState>in-game</onlineState><stateMessage><![CDATA[In-Game<br/>Noita]]></stateMessage><inGameInfo><gameName>Noita</gameName></inGameInfo></profile>", "in-game", "Noita", "In-Game")]
    [InlineData("<profile><onlineState>online</onlineState><stateMessage>Online</stateMessage></profile>", "online", "", "Online")]
    [InlineData("<profile><onlineState>offline</onlineState><stateMessage><![CDATA[Last Online 2 hrs, 5 mins ago]]></stateMessage></profile>", "offline", "", "Last Online 2 hrs, 5 mins ago")]
    [InlineData("<response><error>The specified profile could not be found.</error></response>", "unknown", "", "")]
    [InlineData("not xml", "unknown", "", "")]
    public void A_public_profile_says_what_the_friend_is_doing(string xml, string state, string game, string message)
    {
        var s = SteamProfileStatus.Parse(xml);
        Assert.Equal((state, game, message), (s.State, s.Game, s.Message));
    }

    [Fact]
    public void A_config_without_friends_has_none() => Assert.Empty(SteamIdentity.FriendIds("\"UserLocalConfigStore\" { \"x\" \"y\" }"));

    [Fact]
    public void Steam_folders_are_read_from_disk()
    {
        string root = Path.Combine(Path.GetTempPath(), "cairn-steamid-" + Guid.NewGuid().ToString("n"));
        try
        {
            ulong id = 76561198000000001UL;
            ulong account = id - SteamIdentity.IndividualBase;
            Directory.CreateDirectory(Path.Combine(root, "config"));
            Directory.CreateDirectory(Path.Combine(root, "userdata", account.ToString(), "config"));
            File.WriteAllText(Path.Combine(root, "config", "loginusers.vdf"), Users);
            File.WriteAllText(Path.Combine(root, "userdata", account.ToString(), "config", "localconfig.vdf"), Config);
            var me = SteamIdentity.Read(root);
            Assert.NotNull(me);
            Assert.Equal("76561198000000001", me!.Id);
            Assert.Equal(2, me.Friends.Count);
            Assert.Null(SteamIdentity.Read(Path.Combine(root, "missing")));
        }
        finally { try { Directory.Delete(root, true); } catch { } }
    }
}
