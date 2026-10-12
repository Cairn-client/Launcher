// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Text.Json;

namespace Cairn.Setup.Tests;

public class BridgeProtocolTests
{
    [Fact]
    public void AHelloNamesTheProtocolAndTheGame()
    {
        var h = Assert.IsType<BridgeHello>(BridgeProtocol.Parse("""{"t":"hello","protocol":1,"game":"noita"}"""));
        Assert.Equal(1, h.Protocol);
        Assert.Equal("noita", h.Game);
    }

    [Theory]
    [InlineData("""{"t":"hello","protocol":0,"game":"noita"}""")]
    [InlineData("""{"t":"hello","protocol":1,"game":"../evil"}""")]
    [InlineData("""{"t":"hello","protocol":"1","game":"noita"}""")]
    [InlineData("""{"t":"hello","game":"noita"}""")]
    [InlineData("not json")]
    [InlineData("[]")]
    [InlineData("""{"t":"somethingnew"}""")]
    [InlineData("")]
    public void AnythingThatIsNotAKnownGoodMessageIsIgnored(string line) => Assert.Null(BridgeProtocol.Parse(line));

    [Fact]
    public void AMenuIsReadRowByRow()
    {
        var m = Assert.IsType<BridgeMenu>(BridgeProtocol.Parse("""
            {"t":"menu","title":"Noita","blocked":"","note":"Host: Ann","items":[
              {"id":"friendly_fire","group":"Rules","kind":"onoff","on":true,"locked":true,"label":"Friendly fire","help":"Players can hurt each other","gate":"f3"},
              {"id":"leave","group":"Session","kind":"action","label":"Leave"},
              {"id":"ping","kind":"info","value":"23 ms","label":"Ping"}]}
            """));
        Assert.Equal("Noita", m.Title);
        Assert.Equal("Host: Ann", m.Note);
        Assert.Equal(3, m.Items.Count);
        Assert.Equal(true, m.Items[0].On);
        Assert.True(m.Items[0].Locked);
        Assert.Equal("f3", m.Items[0].Gate);
        Assert.Null(m.Items[1].On);
        Assert.Equal("23 ms", m.Items[2].Value);
    }

    [Fact]
    public void ADamagedRowIsLeftOutNotTheWholeMenu()
    {
        var m = (BridgeMenu)BridgeProtocol.Parse("""
            {"t":"menu","items":[{"id":"a b","kind":"onoff","label":"bad id"},{"id":"x","kind":"slider","label":"bad kind"},{"id":"ok","kind":"action","label":"Fine"}]}
            """)!;
        Assert.Equal(new[] { "ok" }, m.Items.Select(i => i.Id));
    }

    [Fact]
    public void TheMenuIsCappedAndTextIsCleaned()
    {
        var rows = string.Join(",", Enumerable.Range(0, 200).Select(i => $"{{\"id\":\"r{i}\",\"kind\":\"action\",\"label\":\"L{i}\"}}"));
        var m = (BridgeMenu)BridgeProtocol.Parse("{\"t\":\"menu\",\"items\":[" + rows + "]}")!;
        Assert.Equal(BridgeProtocol.MaxItems, m.Items.Count);

        var long80 = (BridgeMenu)BridgeProtocol.Parse("{\"t\":\"menu\",\"items\":[{\"id\":\"a\",\"kind\":\"action\",\"label\":\"" + new string('x', 500) + "\"}]}")!;
        Assert.Equal(80, long80.Items[0].Label.Length);

        var ctrl = (BridgeMenu)BridgeProtocol.Parse("{\"t\":\"menu\",\"items\":[{\"id\":\"a\",\"kind\":\"action\",\"label\":\"a\\u0007b\\nc\"}]}")!;
        Assert.Equal("abc", ctrl.Items[0].Label);   // control characters never reach the screen
    }

    [Fact]
    public void ALineBiggerThanTheLimitIsRefused() =>
        Assert.Null(BridgeProtocol.Parse("{\"t\":\"notice\",\"text\":\"" + new string('x', BridgeProtocol.MaxLine) + "\"}"));

    [Fact]
    public void NoticesAndSubtitlesAreReadAndLimited()
    {
        Assert.Equal(new BridgeNotice(true, "Ann joined^^Welcome"), BridgeProtocol.Parse("""{"t":"notice","text":"Ann joined^^Welcome"}"""));
        Assert.Equal(new BridgeNotice(false, ""), BridgeProtocol.Parse("""{"t":"notice_hide"}"""));
        Assert.Null(BridgeProtocol.Parse("""{"t":"notice","text":""}"""));
        Assert.Equal(15, ((BridgeSubtitle)BridgeProtocol.Parse("""{"t":"subtitle","seconds":900,"text":"Hello"}""")!).Seconds);
        Assert.Equal(1, ((BridgeSubtitle)BridgeProtocol.Parse("""{"t":"subtitle","seconds":0,"text":"Hello"}""")!).Seconds);
        Assert.Null(BridgeProtocol.Parse("""{"t":"subtitle","text":"no seconds"}"""));
    }

    [Fact]
    public void TheLaunchersMessagesAreOneLineOfJson()
    {
        Assert.Equal("""{"t":"welcome","protocol":1}""", BridgeProtocol.Welcome());
        Assert.Equal("""{"t":"attach"}""", BridgeProtocol.Attach());
        Assert.Equal("""{"t":"state"}""", BridgeProtocol.AskState());
        Assert.Equal("""{"t":"do","id":"friendly_fire","verb":"nudge"}""", BridgeProtocol.Do("friendly_fire", "nudge"));
        Assert.Equal("""{"t":"bye","reason":"too new"}""", BridgeProtocol.Bye("too new"));
        foreach (var l in new[] { BridgeProtocol.Welcome(), BridgeProtocol.Attach(), BridgeProtocol.Bye("a\nb") })
        {
            Assert.DoesNotContain('\n', l);
            JsonDocument.Parse(l).Dispose();
        }
    }

    [Theory]
    [InlineData("a b", "enter")]
    [InlineData("a\"}", "enter")]
    [InlineData("ok", "delete")]
    [InlineData("", "enter")]
    public void ADoCallNeverCarriesAnythingButAPlainIdAndAKnownVerb(string id, string verb) => Assert.Null(BridgeProtocol.Do(id, verb));

    [Fact]
    public void TheSavesAreReadSlotBySlot()
    {
        var s = Assert.IsType<BridgeSaves>(BridgeProtocol.Parse("""
            {"t":"saves","slots":[
              {"id":"run-12","name":"Ann's run","detail":"depth 4, 3 perks","kind":"world","when":"2026-10-12T10:30:00Z"},
              {"id":"ann","name":"Ann","kind":"character"},
              {"id":"../x","name":"escapes","kind":"world"},
              {"id":"ok","name":"","kind":"world"},
              {"id":"ok2","name":"Strange","kind":"sandwich"}]}
            """));
        Assert.Equal(new[] { "run-12", "ann" }, s.Slots.Select(x => x.Id));   // unsafe ids, nameless and unknown-kind slots are left out
        Assert.Equal(new DateTime(2026, 10, 12, 10, 30, 0, DateTimeKind.Utc), s.Slots[0].When);
        Assert.Null(s.Slots[1].When);
        Assert.Equal("character", s.Slots[1].Kind);
    }

    [Fact]
    public void TheSavesListIsCapped()
    {
        var rows = string.Join(",", Enumerable.Range(0, 300).Select(i => $"{{\"id\":\"s{i}\",\"name\":\"S{i}\",\"kind\":\"world\"}}"));
        Assert.Equal(BridgeProtocol.MaxSaves, ((BridgeSaves)BridgeProtocol.Parse("{\"t\":\"saves\",\"slots\":[" + rows + "]}")!).Slots.Count);
        Assert.Null(BridgeProtocol.Parse("""{"t":"saves"}"""));
    }

    [Fact]
    public void TheSaveRequestsAreBuiltOnlyFromPlainIds()
    {
        Assert.Equal("""{"t":"saves_ask"}""", BridgeProtocol.AskSaves());
        Assert.Equal("""{"t":"save_delete","id":"run-12"}""", BridgeProtocol.DeleteSave("run-12"));
        Assert.Null(BridgeProtocol.DeleteSave("../run"));
        Assert.Null(BridgeProtocol.DeleteSave("a b"));
        Assert.Null(BridgeProtocol.DeleteSave(""));
    }
}
