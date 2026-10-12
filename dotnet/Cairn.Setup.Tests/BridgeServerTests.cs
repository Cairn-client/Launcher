// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Collections.Concurrent;
using System.IO.Pipes;
using System.Text;

namespace Cairn.Setup.Tests;

public class BridgeServerTests : IDisposable
{
    private readonly BridgeServer _server = new();
    private readonly ConcurrentQueue<BridgeIn> _got = new();
    private readonly ConcurrentQueue<string> _changes = new();

    public BridgeServerTests()
    {
        _server.Received += m => _got.Enqueue(m);
        _server.ConnectionChanged += g => _changes.Enqueue(g);
    }

    public void Dispose() => _server.Dispose();

    private async Task<(NamedPipeClientStream Pipe, StreamReader In, StreamWriter Out)> ConnectAsync()
    {
        var pipe = new NamedPipeClientStream(".", _server.PipeName, PipeDirection.InOut, PipeOptions.Asynchronous);
        await pipe.ConnectAsync(5000);
        return (pipe, new StreamReader(pipe, new UTF8Encoding(false)), new StreamWriter(pipe, new UTF8Encoding(false)) { AutoFlush = true, NewLine = "\n" });
    }

    private static async Task<string?> ReadLine(StreamReader r) => await r.ReadLineAsync().WaitAsync(TimeSpan.FromSeconds(5));

    private static async Task Until(Func<bool> done)
    {
        for (int i = 0; i < 200 && !done(); i++) await Task.Delay(25);
        Assert.True(done(), "timed out");
    }

    [Fact]
    public void ThePipeNameCannotBeGuessed()
    {
        using var other = new BridgeServer();
        Assert.NotEqual(_server.PipeName, other.PipeName);
        Assert.Matches("^cairn-bridge-[0-9a-f]{32}$", _server.PipeName);
    }

    [Fact]
    public async Task ABridgeSaysHelloIsWelcomedAndItsMessagesArriveChecked()
    {
        var (pipe, input, output) = await ConnectAsync();
        using (pipe)
        {
            await output.WriteLineAsync("""{"t":"hello","protocol":1,"game":"noita"}""");
            Assert.Equal(BridgeProtocol.Welcome(), await ReadLine(input));
            await Until(() => _server.Connected);
            Assert.Equal("noita", _server.Game);
            Assert.Equal(new[] { "noita" }, _changes.ToArray());

            await output.WriteLineAsync("""{"t":"notice","text":"Ann joined"}""");
            await output.WriteLineAsync("garbage that is not json");
            await output.WriteLineAsync("""{"t":"somethingnew"}""");
            await output.WriteLineAsync("""{"t":"notice_hide"}""");
            await Until(() => _got.Count >= 2);
            Assert.Equal(new BridgeIn[] { new BridgeNotice(true, "Ann joined"), new BridgeNotice(false, "") }, _got.ToArray());   // the damaged lines were skipped
        }
    }

    [Fact]
    public async Task TheLauncherCanAnswerTheBridge()
    {
        var (pipe, input, output) = await ConnectAsync();
        using (pipe)
        {
            await output.WriteLineAsync("""{"t":"hello","protocol":1,"game":"noita"}""");
            await ReadLine(input);   // welcome
            await Until(() => _server.Connected);
            Assert.True(_server.Send(BridgeProtocol.Attach()));
            Assert.Equal("""{"t":"attach"}""", await ReadLine(input));
            Assert.True(_server.Send(BridgeProtocol.Do("friendly_fire", "enter")!));
            Assert.Equal("""{"t":"do","id":"friendly_fire","verb":"enter"}""", await ReadLine(input));
        }
    }

    [Fact]
    public void WithNoBridgeNothingIsSent() => Assert.False(_server.Send(BridgeProtocol.Attach()));

    [Fact]
    public async Task ABridgeThatNeedsANewerLauncherIsToldSoAndDisconnected()
    {
        var (pipe, input, output) = await ConnectAsync();
        using (pipe)
        {
            await output.WriteLineAsync("{\"t\":\"hello\",\"protocol\":" + (BridgeProtocol.Version + 1) + ",\"game\":\"noita\"}");
            Assert.Contains("newer launcher", await ReadLine(input));
            Assert.Null(await ReadLine(input));   // the connection ended
            Assert.False(_server.Connected);
        }
    }

    [Fact]
    public async Task ABridgeThatDoesNotSayHelloFirstIsDropped()
    {
        var (pipe, input, output) = await ConnectAsync();
        using (pipe)
        {
            await output.WriteLineAsync("""{"t":"notice","text":"sneaking in"}""");
            Assert.Null(await ReadLine(input));
            Assert.Empty(_got);
            Assert.False(_server.Connected);
        }
    }

    [Fact]
    public async Task ALineThatNeverEndsDropsTheConnection()
    {
        var (pipe, input, output) = await ConnectAsync();
        using (pipe)
        {
            await output.WriteLineAsync("""{"t":"hello","protocol":1,"game":"noita"}""");
            await ReadLine(input);
            await Until(() => _server.Connected);
            await output.WriteAsync(new string('x', BridgeProtocol.MaxLine + 100));
            await Until(() => !_server.Connected);
        }
    }

    [Fact]
    public async Task ARestartedBridgeTakesOver()
    {
        var first = await ConnectAsync();
        await first.Out.WriteLineAsync("""{"t":"hello","protocol":1,"game":"noita"}""");
        await ReadLine(first.In);
        await Until(() => _server.Connected);
        first.Pipe.Dispose();
        await Until(() => !_server.Connected);

        var second = await ConnectAsync();
        using (second.Pipe)
        {
            await second.Out.WriteLineAsync("""{"t":"hello","protocol":1,"game":"noita"}""");
            Assert.Equal(BridgeProtocol.Welcome(), await ReadLine(second.In));
            await Until(() => _server.Connected);
        }
    }
}
