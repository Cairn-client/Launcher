// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using Cairn.Launcher.Models;

namespace Cairn.Setup.Tests;

public class DownloadManagerTests
{
    private static DownloadItem Item(string game, long size = 1000) =>
        new(game, game.ToUpperInvariant(), null, new PackInfo(game, game, "1.0.0", game + ".zip", new string('a', 64), size), new Uri("https://example.test/" + game));

    /// <summary>A runner that waits for the test to open its gate, so the order of the queue can be looked at while it is held.</summary>
    private sealed class Gated
    {
        public readonly List<string> Ran = new();
        public readonly Dictionary<string, TaskCompletionSource> Gates = new();
        public DownloadManager.Runner Runner => async (item, progress, ct) =>
        {
            Ran.Add(item.GameId);
            var gate = Gates.TryGetValue(item.GameId, out var g) ? g : null;
            if (gate is not null) await gate.Task.WaitAsync(ct);
            progress.Report(1);
            return (true, "ok");
        };
    }

    private static async Task Until(Func<bool> condition)
    {
        for (int i = 0; i < 200 && !condition(); i++) await Task.Delay(10);
        Assert.True(condition(), "the condition was not reached in time");
    }

    [Fact]
    public async Task DownloadsRunOneAfterTheOtherInTheOrderQueued()
    {
        var g = new Gated();
        var m = new DownloadManager(g.Runner);
        var items = new[] { Item("a"), Item("b"), Item("c") };
        foreach (var i in items) m.Enqueue(i);
        foreach (var i in items) Assert.True(await i.Finished);
        Assert.Equal(new[] { "a", "b", "c" }, g.Ran);
        Assert.All(items, i => Assert.Equal(DownloadState.Done, i.State));
        Assert.False(m.Busy);
    }

    [Fact]
    public async Task AWaitingDownloadCanBeMovedAndTheRunningOneStaysFirst()
    {
        var g = new Gated();
        g.Gates["a"] = new TaskCompletionSource();
        var m = new DownloadManager(g.Runner);
        var a = Item("a"); var b = Item("b"); var c = Item("c"); var d = Item("d");
        foreach (var i in new[] { a, b, c, d }) m.Enqueue(i);
        await Until(() => a.State == DownloadState.Downloading);

        m.MoveFirst(d.Id);   // d jumps ahead of b and c, but not ahead of the running a
        Assert.Equal(new[] { "a", "d", "b", "c" }, m.Snapshot().Select(i => i.GameId));
        m.Move(d.Id, +1);
        Assert.Equal(new[] { "a", "b", "d", "c" }, m.Snapshot().Select(i => i.GameId));
        m.Move(b.Id, -1);    // already first of the waiting ones: nothing
        m.Move(c.Id, +1);    // already last: nothing
        Assert.Equal(new[] { "a", "b", "d", "c" }, m.Snapshot().Select(i => i.GameId));
        m.Move(a.Id, +1);    // the running one cannot be moved
        Assert.Equal("a", m.Snapshot()[0].GameId);

        g.Gates["a"].SetResult();
        foreach (var i in new[] { a, b, c, d }) await i.Finished;
        Assert.Equal(new[] { "a", "b", "d", "c" }, g.Ran);
    }

    [Fact]
    public async Task CancellingAWaitingOneSkipsItAndCancellingTheRunningOneStopsIt()
    {
        var g = new Gated();
        g.Gates["a"] = new TaskCompletionSource();
        var m = new DownloadManager(g.Runner);
        var a = Item("a"); var b = Item("b"); var c = Item("c");
        foreach (var i in new[] { a, b, c }) m.Enqueue(i);
        await Until(() => a.State == DownloadState.Downloading);

        m.Cancel(b.Id);
        Assert.Equal(DownloadState.Cancelled, b.State);
        Assert.False(await b.Finished);

        m.Cancel(a.Id);      // the runner sees the token and the queue goes on with c
        Assert.False(await a.Finished);
        Assert.Equal(DownloadState.Cancelled, a.State);
        Assert.True(await c.Finished);
        Assert.Equal(new[] { "a", "c" }, g.Ran);
    }

    [Fact]
    public async Task AGameAlreadyInTheQueueKeepsItsPlaceAndFailuresAreReported()
    {
        var gate = new TaskCompletionSource();
        var m = new DownloadManager(async (item, progress, ct) =>
        {
            await gate.Task;
            return item.GameId == "bad" ? (false, "the checksum is wrong") : (true, "ok");
        });
        var first = m.Enqueue(Item("a"));
        var again = m.Enqueue(Item("a"));
        Assert.Same(first, again);
        var bad = m.Enqueue(Item("bad"));
        gate.SetResult();
        Assert.True(await first.Finished);
        Assert.False(await bad.Finished);
        Assert.Equal(DownloadState.Failed, bad.State);
        Assert.Equal("the checksum is wrong", bad.Message);

        m.ClearFinished();
        Assert.Empty(m.Snapshot());
    }

    [Fact]
    public async Task ProgressIsKeptAndSpeedIsZeroWhenNothingRuns()
    {
        var gate = new TaskCompletionSource();
        var m = new DownloadManager(async (item, progress, ct) =>
        {
            progress.Report(0.25);
            await Task.Delay(300, ct);
            progress.Report(0.75);
            await gate.Task;
            return (true, "ok");
        });
        Assert.Equal(0, m.SpeedBytesPerSecond);
        var i = m.Enqueue(Item("a", 1_000_000));
        await Until(() => i.Progress >= 0.75);
        Assert.Equal(750_000, i.BytesDone);
        Assert.True(m.SpeedBytesPerSecond > 0);
        Assert.Equal(750_000, m.TotalBytes);
        gate.SetResult();
        await i.Finished;
        Assert.Equal(0, m.SpeedBytesPerSecond);
    }
}