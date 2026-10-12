// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using Cairn.Setup;

namespace Cairn.Setup.Tests;

public class PackSetupTests : IDisposable
{
    private readonly string _dir = Path.Combine(Path.GetTempPath(), "cairn-psetup-" + Guid.NewGuid().ToString("N")[..8]);
    public PackSetupTests() => Directory.CreateDirectory(_dir);
    public void Dispose() { try { Directory.Delete(_dir, true); } catch { } }

    private string Script(string body)
    {
        var p = Path.Combine(_dir, "prog" + Guid.NewGuid().ToString("N")[..4] + ".cmd");
        File.WriteAllText(p, "@echo off\r\n" + body + "\r\n");
        return p;
    }

    [Fact]
    public void AStepLineIsReadWithItsLimits()
    {
        var s = PackSetup.ParseStep("{\"step\":{\"id\":\"workspace\",\"title\":\"Linked\",\"status\":\"working\",\"detail\":\"Linking\",\"action\":\"link-workspace\",\"actionLabel\":\"Try again\",\"progress\":1.7,\"optional\":true,\"system\":true,\"auto\":\"while\"}}");
        Assert.NotNull(s);
        Assert.Equal(new PackStep("workspace", "Linked", PackStepStatus.Working, "Linking", "link-workspace", "Try again", 1.0, true, true, "while"), s);
    }

    [Theory]
    [InlineData("not json")]
    [InlineData("{\"step\":{\"id\":\"a b\",\"title\":\"T\",\"status\":\"done\"}}")]          // a bad id
    [InlineData("{\"step\":{\"id\":\"a\",\"title\":\"T\",\"status\":\"almost\"}}")]            // an unknown status
    [InlineData("{\"step\":{\"id\":\"a\",\"title\":\"\",\"status\":\"done\"}}")]                // no title
    [InlineData("{\"step\":{\"id\":\"a\",\"title\":\"T\",\"status\":\"done\",\"action\":\"x y\"}}")]   // a bad action
    [InlineData("{\"step\":{\"id\":\"a\",\"title\":\"T\",\"status\":\"done\",\"auto\":\"always\"}}")]  // an unknown auto
    [InlineData("{\"done\":true}")]
    public void ALineThatIsNotAStepIsNotOne(string line) => Assert.Null(PackSetup.ParseStep(line));

    [Fact]
    public async Task TheChecklistIsReadFromTheProgramsLines()
    {
        var p = Script(
            "echo {\"step\":{\"id\":\"steam\",\"title\":\"Steam\",\"status\":\"done\",\"detail\":\"ok\",\"system\":true}}\r\n" +
            "echo {\"step\":{\"id\":\"mod\",\"title\":\"Mod\",\"status\":\"needs-you\",\"detail\":\"no\",\"action\":\"place-mod\",\"actionLabel\":\"Place it\"}}\r\n" +
            "echo {\"done\":true,\"ready\":true}\r\n" +
            "echo {\"found\":{\"moddingToolsExe\":\"C:\\\\mt\\\\KingdomCome.exe\"}}");
        var r = await PackSetup.CheckAsync(p, null);
        Assert.NotNull(r);
        Assert.Equal(2, r!.Steps.Count);
        Assert.False(r.Ready);                       // the program said ready, but a step is not done: the launcher decides
        Assert.Equal("C:\\mt\\KingdomCome.exe", r.GameExe);
        Assert.Equal("place-mod", r.Steps[1].Action);
    }

    [Fact]
    public async Task TheProgramsFoundLineGivesTheExecutableAndTheFolderItsOptionsBelongIn()
    {
        var p = Script(
            "echo {\"step\":{\"id\":\"a\",\"title\":\"A\",\"status\":\"done\"}}\r\n" +
            "echo {\"done\":true,\"ready\":true}\r\n" +
            "echo {\"gameExeNote\":\"That is the wrong build.\"}\r\n" +
            "echo {\"found\":{\"gameExe\":\"C:\\\\mt\\\\Bin\\\\k.exe\",\"gameDir\":\"C:\\\\mt\"}}");
        var r = await PackSetup.CheckAsync(p, null);
        Assert.Equal("C:\\mt\\Bin\\k.exe", r!.GameExe);
        Assert.Equal("C:\\mt", r.GameDir);
        Assert.Equal("That is the wrong build.", r.GameExeNote);
    }

    [Fact]
    public async Task AProgramThatSaysNothingOfTheFolderOrTheExecutableLeavesThemOut()
    {
        var r = await PackSetup.CheckAsync(Script("echo {\"step\":{\"id\":\"a\",\"title\":\"A\",\"status\":\"done\"}}\r\necho {\"done\":true,\"ready\":true}"), null);
        Assert.Null(r!.GameDir);
        Assert.Null(r.GameExeNote);
    }

    [Fact]
    public async Task AllStepsDoneAndTheProgramSayingSoIsReady()
    {
        var p = Script("echo {\"step\":{\"id\":\"a\",\"title\":\"A\",\"status\":\"done\"}}\r\necho {\"step\":{\"id\":\"b\",\"title\":\"B\",\"status\":\"needs-you\",\"optional\":true}}\r\necho {\"done\":true,\"ready\":true,\"optionalNeedsYou\":true}");
        var r = await PackSetup.CheckAsync(p, null);
        Assert.True(r!.Ready);
        Assert.True(r.OptionalNeedsYou);
    }

    [Fact]
    public async Task AProgramThatGivesNoListOrNoEndIsNotAChecklist()
    {
        Assert.Null(await PackSetup.CheckAsync(Script("echo garbage"), null));
        Assert.Null(await PackSetup.CheckAsync(Script("echo {\"step\":{\"id\":\"a\",\"title\":\"A\",\"status\":\"done\"}}"), null));   // never said it was done
        Assert.Null(await PackSetup.CheckAsync(Path.Combine(_dir, "missing.cmd"), null));
    }

    [Fact]
    public async Task TheProgramIsGivenTheWordsOfTheContractAndTheModdingToolsOnlyAsAnArgument()
    {
        var p = Script("echo {\"step\":{\"id\":\"a\",\"title\":\"%~1 %~2 %~3 %~nx4\",\"status\":\"done\"}}\r\necho {\"done\":true,\"ready\":true}");
        var r = await PackSetup.CheckAsync(p, "C:\\Games\\mt.exe");
        Assert.Equal("setup check --game-exe mt.exe", r!.Steps[0].Title);
    }

    [Fact]
    public async Task AnActionReportsItsProgressAndItsEnd()
    {
        var said = new List<string>(); var progress = new List<double>();
        var p = Script("echo {\"say\":\"Linking\"}\r\necho {\"progress\":0.5}\r\necho {\"ok\":true,\"message\":\"Done.\",\"pending\":90}");
        var r = await PackSetup.DoAsync(p, "link-workspace", null, said.Add, progress.Add);
        Assert.Equal(new PackActionResult(true, "Done.", 90), r);
        Assert.Equal(new[] { "Linking" }, said);
        Assert.Equal(new[] { 0.5 }, progress);
    }

    [Fact]
    public async Task AnActionTheProgramRefusesOrDoesNotAnswerFails()
    {
        Assert.Equal(new PackActionResult(false, "Close the game."), await PackSetup.DoAsync(Script("echo {\"ok\":false,\"message\":\"Close the game.\"}"), "place-mod", null, _ => { }, _ => { }));
        Assert.False((await PackSetup.DoAsync(Script("echo nothing"), "place-mod", null, _ => { }, _ => { })).Ok);
        Assert.False((await PackSetup.DoAsync(Script("echo {\"ok\":true}"), "not an action!", null, _ => { }, _ => { })).Ok);   // never run
    }

    [Fact]
    public void ThePackContractReadsTheSetupFlagAndNeedsTheProgram()
    {
        string Json(string extra) => "{\"schema\":2,\"id\":\"kcd2\",\"version\":\"1.0.0\",\"modFolder\":\"kdcmp\",\"luaPrefix\":\"K_\",\"agent\":\"bin/a.exe\",\"relay\":\"bin/r.exe\",\"native\":\"bin/n.dll\"" + extra + "}";
        Assert.True(PackContract.Parse(Json(",\"optionsProgram\":\"bin/p.exe\",\"setup\":true"))!.Setup);
        Assert.False(PackContract.Parse(Json(",\"optionsProgram\":\"bin/p.exe\""))!.Setup);
        Assert.Null(PackContract.Parse(Json(",\"setup\":true")));   // a setup with no program to do it
    }
}
