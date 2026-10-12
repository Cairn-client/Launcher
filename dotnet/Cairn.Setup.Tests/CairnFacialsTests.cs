// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
namespace Cairn.Setup.Tests;

/// <summary>Cairn: NPC lip sync -- the regular game's facial paks linked into the Modding Tools.</summary>
public class CairnFacialsTests
{
    private static StepView Step(IReadOnlyList<StepView> s, StepId id) => s.Single(x => x.Id == id);

    private static string AddRetailFacial(FakeSteam f, string name, string content)
    {
        var dir = FacialsLink.SourceDir(f.GameRoot);
        Directory.CreateDirectory(dir);
        var p = Path.Combine(dir, name);
        File.WriteAllText(p, content);
        return p;
    }

    private sealed class FailingLinks(int hardError) : ILinkPrimitives
    {
        public int CreateHardLink(string newFile, string existingFile) => hardError;
        public int CreateSymlink(string link, string target) => 1;
        public int MoveReplace(string from, string to) => 0;
    }

    [Fact]
    public void Missing_facial_paks_are_a_step_that_needs_the_link_and_never_blocks()
    {
        using var f = new FakeSteam("facials-missing").WithGame().WithModdingTools().Linked().WithInstall().WithModPlaced();
        AddRetailFacial(f, "Facials_english-part0.pak", "retail facial data");
        var snap = f.Snapshot();
        Assert.Equal(1, snap.Facials.Total);
        Assert.False(snap.Facials.Complete);
        var steps = SetupChecklist.Evaluate(snap, SetupActivity.Idle);
        var step = Step(steps, StepId.Facials);
        Assert.Equal(StepStatus.NeedsYou, step.Status);
        Assert.Equal(StepAction.LinkFacials, step.Action);
        Assert.True(step.Optional);
        Assert.True(SetupChecklist.IsReady(steps));   // Host and Join are not held back
    }

    [Fact]
    public void Link_makes_a_hard_link_of_every_missing_pak_and_the_step_is_done()
    {
        using var f = new FakeSteam("facials-link").WithGame().WithModdingTools().Linked().WithInstall().WithModPlaced();
        var a = AddRetailFacial(f, "Facials_english-part0.pak", "part zero");
        AddRetailFacial(f, "Facials_english-part1.pak", "part one, longer");
        File.WriteAllText(Path.Combine(FacialsLink.SourceDir(f.GameRoot), "not-a-facial.txt"), "ignored");
        var snap = f.Snapshot();
        Assert.Equal(2, snap.Facials.Total);

        var run = FacialsLink.Link(snap.Facials);
        Assert.Equal(2, run.Linked);
        Assert.Equal(0, run.Failed);
        Assert.Null(run.Failure);
        var target = Path.Combine(FacialsLink.TargetDir(f.MtRoot), "Facials_english-part0.pak");
        Assert.Equal("part zero", File.ReadAllText(target));
        Assert.False(File.Exists(target + FacialsLink.TempSuffix));
        Assert.False(File.Exists(Path.Combine(FacialsLink.TargetDir(f.MtRoot), "not-a-facial.txt")));

        // a hard link is the same file: writing through the retail path shows in the target
        File.WriteAllText(a, "part ZERO");
        Assert.Equal("part ZERO", File.ReadAllText(target));

        var after = f.Snapshot();
        Assert.True(after.Facials.Complete);
        var step = Step(SetupChecklist.Evaluate(after, SetupActivity.Idle), StepId.Facials);
        Assert.Equal(StepStatus.Done, step.Status);
        Assert.Contains("mouths move", step.Detail);
    }

    [Fact]
    public void Nothing_to_link_when_the_regular_game_has_no_facial_data()
    {
        using var f = new FakeSteam("facials-none").WithGame().WithModdingTools().Linked().WithInstall().WithModPlaced();
        var snap = f.Snapshot();
        Assert.Equal(0, snap.Facials.Total);
        var step = Step(SetupChecklist.Evaluate(snap, SetupActivity.Idle), StepId.Facials);
        Assert.Equal(StepStatus.Done, step.Status);
        Assert.Contains("Nothing to link", step.Detail);
    }

    [Fact]
    public void A_pak_of_a_different_size_is_replaced_by_the_link()
    {
        using var f = new FakeSteam("facials-stale").WithGame().WithModdingTools().Linked().WithInstall().WithModPlaced();
        AddRetailFacial(f, "Facials_english-part0.pak", "the real, longer retail data");
        var dir = FacialsLink.TargetDir(f.MtRoot);
        Directory.CreateDirectory(dir);
        File.WriteAllText(Path.Combine(dir, "Facials_english-part0.pak"), "stale");
        var snap = f.Snapshot();
        Assert.False(snap.Facials.Complete);
        Assert.Equal(1, FacialsLink.Link(snap.Facials).Linked);
        Assert.Equal("the real, longer retail data", File.ReadAllText(Path.Combine(dir, "Facials_english-part0.pak")));
    }

    [Fact]
    public void Different_drives_say_so_and_leave_nothing_behind()
    {
        using var f = new FakeSteam("facials-drives").WithGame().WithModdingTools().Linked().WithInstall().WithModPlaced();
        AddRetailFacial(f, "Facials_english-part0.pak", "retail");
        var snap = f.Snapshot();
        var run = FacialsLink.Link(snap.Facials, new FailingLinks(Native.ERROR_NOT_SAME_DEVICE));
        Assert.Equal(0, run.Linked);
        Assert.Equal(1, run.Failed);
        Assert.Contains("different drives", run.Failure);
        Assert.False(File.Exists(Path.Combine(FacialsLink.TargetDir(f.MtRoot), "Facials_english-part0.pak")));

        var step = Step(SetupChecklist.Evaluate(snap, SetupActivity.Idle with { FacialsFailure = run.Failure }), StepId.Facials);
        Assert.Equal(StepStatus.NeedsYou, step.Status);
        Assert.Contains("different drives", step.Detail);
    }

    [Fact]
    public void While_linking_the_step_is_working()
    {
        using var f = new FakeSteam("facials-working").WithGame().WithModdingTools().Linked().WithInstall().WithModPlaced();
        AddRetailFacial(f, "Facials_english-part0.pak", "retail");
        var step = Step(SetupChecklist.Evaluate(f.Snapshot(), SetupActivity.Idle with { LinkingFacials = true }), StepId.Facials);
        Assert.Equal(StepStatus.Working, step.Status);
    }

    [Fact]
    public void A_voice_pak_the_Modding_Tools_lack_is_linked_and_one_they_have_is_left_alone()
    {
        using var f = new FakeSteam("voice-link").WithGame().WithModdingTools().Linked().WithInstall().WithModPlaced();
        var loc = Path.Combine(f.GameRoot, "Localization");
        File.WriteAllText(Path.Combine(loc, "english-part1.pak"), "voices part one");
        File.WriteAllText(Path.Combine(loc, "Czech_xml.pak"), "text, never linked");
        var mtLoc = Path.Combine(f.MtRoot, "Localization");
        Directory.CreateDirectory(mtLoc);
        File.WriteAllText(Path.Combine(mtLoc, "english-part0.pak"), "the Modding Tools' own, different");
        var snap = f.Snapshot();
        Assert.Equal(1, snap.Facials.Total);
        Assert.EndsWith("english-part1.pak", snap.Facials.Missing.Single().Target);
        Assert.Equal(1, FacialsLink.Link(snap.Facials).Linked);
        Assert.True(File.Exists(Path.Combine(mtLoc, "english-part1.pak")));
        Assert.False(File.Exists(Path.Combine(mtLoc, "Czech_xml.pak")));
        Assert.Equal("the Modding Tools' own, different", File.ReadAllText(Path.Combine(mtLoc, "english-part0.pak")));
    }}
