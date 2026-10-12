// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using Cairn.Setup;

namespace Cairn.Setup.Tests;

/// <summary>What the launcher reads from a pack's session program (PackSession.Handle): bounded, plain, and nothing it does not know.</summary>
public class PackSessionTests
{
    private static PackSession New() => new(null);

    private const string StateLine =
        "{\"t\":\"state\",\"stage\":\"waiting-connect\",\"status\":\"Load your save.\",\"connect\":{\"enabled\":false,\"reason\":\"Load your save first.\"}," +
        "\"conn\":{\"text\":\"Connecting...\",\"bad\":true},\"join\":{\"state\":\"choose\",\"message\":\"Pick one\",\"canFresh\":true}," +
        "\"host\":{\"relay\":true,\"error\":\"\",\"steamState\":\"ready\",\"steamMessage\":\"Steam is ready\",\"steamCode\":\"ABC-123\",\"refused\":\"\"},\"game\":true}";

    [Fact]
    public void AStateIsReadWhole()
    {
        var s = New(); SessionView? got = null; s.State += v => got = v;
        s.Handle(StateLine);
        Assert.NotNull(got);
        Assert.Equal("waiting-connect", got!.Stage);
        Assert.False(got.ConnectEnabled); Assert.Equal("Load your save first.", got.ConnectReason);
        Assert.True(got.ConnBad); Assert.Equal("choose", got.JoinState); Assert.True(got.JoinCanFresh);
        Assert.True(got.Host.RelayUp); Assert.Equal("ABC-123", got.Host.SteamCode); Assert.True(got.GameRunning);
        Assert.Same(got, s.View);
    }

    [Fact]
    public void AStateWithAnUnknownStageIsIgnoredAndTheLastViewStays()
    {
        var s = New(); int n = 0; s.State += _ => n++;
        s.Handle(StateLine);
        s.Handle(StateLine.Replace("waiting-connect", "exploding"));
        Assert.Equal(1, n);
        Assert.Equal("waiting-connect", s.View.Stage);
    }

    [Fact]
    public void AProblemKeepsItsWordsAndItsButtons()
    {
        var s = New(); SessionProblem? got = null; s.Problem += p => got = p;
        s.Handle("{\"t\":\"problem\",\"title\":\"A MOD FILE WAS REMOVED\",\"text\":\"gone: x.dll\",\"next\":\"Restore it.\",\"reportBug\":true,\"launchAnyway\":true,\"log\":\"MP-LAUNCH blocked\"}");
        Assert.Equal(new SessionProblem("A MOD FILE WAS REMOVED", "gone: x.dll", "Restore it.", true, true, false), got);
    }

    [Fact]
    public void AMessageWithTheAddressFallbackIsAProblemWithThatButton()
    {
        var s = New(); SessionProblem? got = null; string? plain = null;
        s.Problem += p => got = p; s.Message += (title, text) => plain = title + "|" + text;
        s.Handle("{\"t\":\"message\",\"title\":\"STEAM CONNECTION FAILED\",\"text\":\"No route.\",\"next\":\"\",\"addressFallback\":true}");
        Assert.True(got!.AddressFallback);
        s.Handle("{\"t\":\"message\",\"title\":\"CONNECTION OK\",\"text\":\"Reached.\"}");
        Assert.Equal("CONNECTION OK|Reached.", plain);
    }

    [Fact]
    public void TheGameIsReportedWithItsPidAndStartTime()
    {
        var s = New(); SessionGame? got = null; s.Game += g => got = g;
        s.Handle("{\"t\":\"game\",\"pid\":1234,\"startedAt\":\"2026-10-12T22:00:00.0000000+02:00\",\"hosting\":true}");
        Assert.Equal(1234, got!.Pid); Assert.True(got.Hosting);
        s.Handle("{\"t\":\"game\",\"pid\":-5}");
        s.Handle("{\"t\":\"game\",\"pid\":\"x\"}");
        Assert.Equal(1234, got.Pid);
    }

    [Fact]
    public void FriendsAreBounded()
    {
        var s = New(); SessionFriends? got = null; s.Friends += f => got = f;
        var many = string.Join(",", Enumerable.Range(0, 500).Select(i => $"{{\"name\":\"n{i}\",\"code\":\"c{i}\",\"release\":\"0.2.0\"}}"));
        s.Handle($"{{\"t\":\"friends\",\"state\":\"ok\",\"message\":\"\",\"next\":\"\",\"friends\":[{many}]}}");
        Assert.Equal("ok", got!.State);
        Assert.Equal(100, got.Friends.Count);
    }

    [Theory]
    [InlineData("not json")]
    [InlineData("[1]")]
    [InlineData("{\"t\":5}")]
    [InlineData("{\"t\":\"nonsense\"}")]
    [InlineData("{\"t\":\"state\"}")]
    public void WhatItDoesNotKnowDoesNothing(string line)
    {
        var s = New(); int n = 0; s.State += _ => n++; s.Problem += _ => n++; s.Error += _ => n++; s.Game += _ => n++;
        s.Handle(line);
        Assert.Equal(0, n);
    }

    [Fact]
    public void ControlCharactersAreStrippedFromWhatIsShown()
    {
        var s = New(); string? err = null; s.Error += t => err = t;
        s.Handle("{\"t\":\"error\",\"text\":\"bad\\u0007 <b>news</b>\"}");
        Assert.Equal("bad bnews/b", err);
    }

    [Fact]
    public void AProgramThatIsNotThereGivesNoSession()
    {
        Assert.Null(PackSession.Start(Path.Combine(Path.GetTempPath(), "no-such-session-program.exe")));
    }
}
