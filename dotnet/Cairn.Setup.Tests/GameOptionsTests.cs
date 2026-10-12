// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
namespace Cairn.Setup.Tests;

public class GameOptionsCatalogTests
{
    private static string Manifest(string options) =>
        "{ \"schema\": 1, \"id\": \"kcd2\", \"name\": \"KCD2\", \"exe\": \"KingdomCome.exe\", \"accent\": \"#E8C872\"" + (options.Length == 0 ? "" : ", \"options\": " + options) + " }";

    [Fact]
    public void AGamesOptionsAreReadFromItsGameJson()
    {
        var m = GameManifest.Parse(Manifest("""[ { "id": "dlss5", "title": "DLSS 5 (RTX cards)", "hint": "Needs an RTX card.", "requires": "nvidia-rtx" }, { "id": "plain", "title": "Another" } ]"""))!;
        Assert.Equal(2, m.Options!.Count);
        Assert.Equal(new GameOption("dlss5", "DLSS 5 (RTX cards)", "Needs an RTX card.", "nvidia-rtx"), m.Options[0]);
        Assert.Equal("", m.Options[1].Requires);
        Assert.Empty(GameManifest.Parse(Manifest(""))!.Options!);
    }

    [Theory]
    [InlineData("""[ { "id": "a b", "title": "x" } ]""")]                                          // a plain id
    [InlineData("""[ { "id": "x", "title": "" } ]""")]                                              // a title
    [InlineData("""[ { "id": "x", "title": "t", "requires": "root" } ]""")]                         // only the needs the launcher can see
    [InlineData("""[ { "id": "x", "title": "t" }, { "id": "x", "title": "again" } ]""")]            // each once
    [InlineData("""[ "dlss5" ]""")]
    public void AnOptionListThatIsNotPlainRefusesTheWholeFile(string options) => Assert.Null(GameManifest.Parse(Manifest(options)));

    [Fact]
    public void ThereAreAtMostEightOptions()
    {
        string many = "[" + string.Join(",", Enumerable.Range(0, 9).Select(i => $"{{ \"id\": \"o{i}\", \"title\": \"T\" }}")) + "]";
        Assert.Null(GameManifest.Parse(Manifest(many)));
    }

    [Fact]
    public void ThePacksOptionsProgramIsAPathInsideThePack()
    {
        Assert.Equal("bin/Bridge.exe", PackContract.Parse(FakeSteam.ContractJson.Replace("\"lua\": {", "\"optionsProgram\": \"bin/Bridge.exe\", \"lua\": {"))!.OptionsProgram);
        Assert.Null(PackContract.Parse(FakeSteam.ContractJson.Replace("\"lua\": {", "\"optionsProgram\": \"../x.exe\", \"lua\": {")));
        Assert.Null(PackContract.Parse("{ \"schema\": 2, \"id\": \"c\", \"version\": \"1.0.0\", \"kind\": \"content\", \"optionsProgram\": \"bin/x.exe\" }"));
    }
}

public class OptionRunnerTests : IDisposable
{
    private readonly string _dir = Path.Combine(Path.GetTempPath(), "cairn-opt-" + Guid.NewGuid().ToString("N")[..8]);
    public OptionRunnerTests() => Directory.CreateDirectory(_dir);
    public void Dispose() { try { Directory.Delete(_dir, true); } catch { } }

    private string Script(string body)
    {
        var p = Path.Combine(_dir, "prog" + Guid.NewGuid().ToString("N")[..4] + ".cmd");
        File.WriteAllText(p, "@echo off\r\n" + body + "\r\n");
        return p;
    }

    [Fact]
    public async Task CheckReadsTheProgramsAnswer()
    {
        var s = Script("echo {\"available\":true,\"on\":true,\"note\":\"RTX 5090\"}");
        Assert.Equal(new OptionStatus(true, true, "RTX 5090"), await OptionRunner.CheckAsync(s, "dlss5", _dir));
    }

    [Fact]
    public async Task ACheckTheProgramDoesNotAnswerIsNull()
    {
        Assert.Null(await OptionRunner.CheckAsync(Script("echo not json"), "dlss5", _dir));
        Assert.Null(await OptionRunner.CheckAsync(Path.Combine(_dir, "missing.cmd"), "dlss5", _dir));
    }

    [Fact]
    public async Task ApplyGivesTheProgramOnlyTheWordsThePlatformDefinesAndReportsProgress()
    {
        var said = new List<string>();
        var s = Script("echo {\"say\":\"args: %~1 %~2 %~3 %~4 %~5\"}\r\necho {\"ok\":true,\"message\":\"Done.\"}");
        var r = await OptionRunner.ApplyAsync(s, "dlss5", on: true, _dir, said.Add);
        Assert.Equal(new OptionRun(true, "Done."), r);
        Assert.Single(said);
        Assert.Equal("args: options apply dlss5 on --game-dir", said[0]);
    }

    [Fact]
    public async Task AFailureIsReportedWithTheProgramsWords()
    {
        var r = await OptionRunner.ApplyAsync(Script("echo {\"ok\":false,\"message\":\"The game is running.\"}"), "dlss5", false, _dir, _ => { });
        Assert.Equal(new OptionRun(false, "The game is running."), r);
        var none = await OptionRunner.ApplyAsync(Script("echo nothing useful"), "dlss5", false, _dir, _ => { });
        Assert.False(none.Ok);
    }

    [Theory]
    [InlineData("a b")]
    [InlineData("x\"; calc")]
    [InlineData("")]
    public async Task AnIdThatIsNotPlainNeverReachesTheProgram(string id)
    {
        var marker = Path.Combine(_dir, "ran.txt");
        var s = Script($"echo ran> \"{marker}\"\r\necho {{\"ok\":true,\"message\":\"\"}}");
        var r = await OptionRunner.ApplyAsync(s, id, true, _dir, _ => { });
        Assert.False(r.Ok);
        Assert.False(File.Exists(marker));
    }

    [Fact]
    public async Task AProgramThatTakesTooLongIsStopped()
    {
        var r = await OptionRunner.ApplyAsync(Script("ping -n 30 127.0.0.1 > nul"), "dlss5", true, _dir, _ => { }, timeout: TimeSpan.FromMilliseconds(700));
        Assert.False(r.Ok);
        Assert.Contains("too long", r.Message);
    }
}

public class BetaCatalogTests : IDisposable
{
    private readonly string _cache = Path.Combine(Path.GetTempPath(), "cairn-cat-" + Guid.NewGuid().ToString("N")[..8]);
    public void Dispose() { try { Directory.Delete(_cache, true); } catch { } }

    private sealed class Fake : HttpMessageHandler
    {
        public readonly List<string> Asked = new();
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage r, CancellationToken ct)
        {
            string url = r.RequestUri!.ToString(); Asked.Add(url);
            string? body = url switch
            {
                var u when u.EndsWith("/games.json") => """{ "schema": 1, "games": [ { "id": "kcd2", "repo": "Cairn-client/mod-kcd2" } ] }""",
                var u when u.StartsWith("https://api.github.com/repos/Cairn-client/mod-kcd2/releases") => """[ { "tag_name": "0.2.5", "draft": false }, { "tag_name": "0.2.4", "draft": false } ]""",
                var u when u.Contains("/releases/download/0.2.5/game.json") => GameJson(withOptions: true),
                var u when u.Contains("/releases/latest/download/game.json") => GameJson(withOptions: false),
                _ => null,
            };
            return Task.FromResult(body is null ? new HttpResponseMessage(System.Net.HttpStatusCode.NotFound) : new HttpResponseMessage(System.Net.HttpStatusCode.OK) { Content = new StringContent(body) });
        }
        private static string GameJson(bool withOptions) =>
            "{ \"schema\": 1, \"id\": \"kcd2\", \"name\": \"KCD2\", \"exe\": \"KingdomCome.exe\", \"accent\": \"#E8C872\"" + (withOptions ? ", \"options\": [ { \"id\": \"dlss5\", \"title\": \"DLSS 5\" } ]" : "") + " }";
    }

    [Fact]
    public async Task APlayerOnTheBetaChannelReadsTheNewestReleasesGameJson()
    {
        var handler = new Fake();
        var beta = await new CatalogClient(new HttpClient(handler), _cache) { FollowsBeta = id => id == "kcd2" }.LoadAsync();
        Assert.Single(beta!.Games[0].Manifest.Options!);
        Assert.EndsWith("/releases/download/0.2.5/", beta.Games[0].BaseUrl);   // its pictures come from there too
    }

    [Fact]
    public async Task AStablePlayerReadsTheLatestStableReleasesAndNeverAsksForTheList()
    {
        var handler = new Fake();
        var stable = await new CatalogClient(new HttpClient(handler), _cache).LoadAsync();
        Assert.Empty(stable!.Games[0].Manifest.Options!);
        Assert.DoesNotContain(handler.Asked, u => u.StartsWith("https://api.github.com/"));
    }
}

public class SavesRunnerTests : IDisposable
{
    private readonly string _dir = Path.Combine(Path.GetTempPath(), "cairn-sav-" + Guid.NewGuid().ToString("N")[..8]);
    public SavesRunnerTests() => Directory.CreateDirectory(_dir);
    public void Dispose() { try { Directory.Delete(_dir, true); } catch { } }

    private string Script(string body) { var p = Path.Combine(_dir, "prog" + Guid.NewGuid().ToString("N")[..4] + ".cmd"); File.WriteAllText(p, "@echo off\r\n" + body + "\r\n"); return p; }

    private const string Saves = "{\"t\":\"saves\",\"slots\":[{\"id\":\"w-1\",\"name\":\"Exit save\",\"detail\":\"Playline 0\",\"kind\":\"world\",\"when\":\"2026-10-12T16:12:00Z\"}]}";

    [Fact]
    public async Task TheProgramsListIsRead()
    {
        var slots = await SavesRunner.ListAsync(Script("echo " + Saves));
        Assert.Equal("Exit save", Assert.Single(slots!).Name);
    }

    [Fact]
    public async Task AListTheProgramDoesNotGiveIsNull()
    {
        Assert.Null(await SavesRunner.ListAsync(Script("echo nothing")));
        Assert.Null(await SavesRunner.ListAsync(Path.Combine(_dir, "missing.cmd")));
    }

    [Fact]
    public async Task ADeleteGivesTheMessageAndTheSavesThatAreLeft()
    {
        var (ok, message, slots) = await SavesRunner.DeleteAsync(Script("echo {\"t\":\"toast\",\"text\":\"Deleted Exit save.\",\"error\":false}\r\necho " + Saves), "w-1");
        Assert.True(ok); Assert.Equal("Deleted Exit save.", message); Assert.Single(slots!);
    }

    [Fact]
    public async Task ARefusalIsSaidAsTheProgramSaidIt()
    {
        var (ok, message, _) = await SavesRunner.DeleteAsync(Script("echo {\"t\":\"toast\",\"text\":\"The game may be using it.\",\"error\":true}"), "w-1");
        Assert.False(ok); Assert.Equal("The game may be using it.", message);
    }

    [Theory]
    [InlineData("../../x")]
    [InlineData("a b")]
    [InlineData("")]
    public async Task AnIdThatIsNotPlainNeverReachesTheProgram(string id)
    {
        var marker = Path.Combine(_dir, "ran.txt");
        var (ok, _, _) = await SavesRunner.DeleteAsync(Script($"echo ran> \"{marker}\""), id);
        Assert.False(ok);
        Assert.False(File.Exists(marker));
    }

    [Fact]
    public async Task ArgumentsAreExactlyTheWordsOfTheContract()
    {
        var said = new List<string>();
        var s = Script("echo {\"t\":\"toast\",\"text\":\"args: %~1 %~2 %~3\",\"error\":false}");
        var (_, message, _) = await SavesRunner.DeleteAsync(s, "w-abc123");
        Assert.Equal("args: saves delete w-abc123", message);
    }
}
