// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
namespace Cairn.Setup.Tests;

public class GameSetupTests
{
    private sealed class Host : IGameSetupHost
    {
        public bool Steam = true, Game = true, Mod = true;
        public HashSet<uint> Apps = new();
        public HashSet<string> Files = new(StringComparer.OrdinalIgnoreCase);
        public long? Disk = 50_000;
        public bool SteamRunning() => Steam;
        public bool GameFound() => Game;
        public bool ModInstalled() => Mod;
        public bool SteamAppInstalled(uint appId) => Apps.Contains(appId);
        public bool GameFileExists(string relativePath) => Files.Contains(relativePath);
        public long? FreeDiskMb() => Disk;
    }

    private static string Manifest(string setup) =>
        "{ \"schema\": 1, \"id\": \"noita\", \"name\": \"Noita\", \"exe\": \"noita.exe\", \"accent\": \"#E8A35A\"" + (setup.Length == 0 ? "" : ", \"setup\": " + setup) + " }";

    [Fact]
    public void WhenAllIsInPlaceTheDefaultsAreAllGreen()
    {
        var r = GameSetup.Evaluate(GameSetup.Defaults, new Host());
        Assert.Equal(3, r.Count);
        Assert.All(r, x => Assert.Equal(SetupState.Ok, x.State));
        Assert.Equal(new[] { "Steam", "Game", "Mod" }, r.Select(x => x.Title));
    }

    [Fact]
    public void AMissingThingIsRedWithWhatToDoAboutIt()
    {
        var r = GameSetup.Evaluate(GameSetup.Defaults, new Host { Steam = false, Mod = false });
        Assert.Equal(new[] { SetupState.Fail, SetupState.Ok, SetupState.Fail }, r.Select(x => x.State));
        Assert.Contains("Start Steam", r[0].Detail);
        Assert.Contains("Download the mod", r[2].Detail);
    }

    [Fact]
    public void AnOptionalLineIsOnlyAWarning()
    {
        var rules = new[] { new SetupRule(SetupKinds.SteamApp, "Noita", "Install Noita from Steam.", AppId: 881100, Optional: true) };
        var r = GameSetup.Evaluate(rules, new Host());
        Assert.Equal(SetupState.Warn, r[0].State);
        Assert.Equal(SetupState.Ok, GameSetup.Evaluate(rules, new Host { Apps = { 881100 } })[0].State);
    }

    [Fact]
    public void AFileInTheGamesFolderIsCheckedOnlyWhenTheGameIsFound()
    {
        var rules = new[] { new SetupRule(SetupKinds.GameFile, "Loader", "The loader is missing.", Path: "winhttp.dll") };
        Assert.Equal(SetupState.Fail, GameSetup.Evaluate(rules, new Host())[0].State);
        Assert.Equal(SetupState.Ok, GameSetup.Evaluate(rules, new Host { Files = { "winhttp.dll" } })[0].State);
        Assert.Equal(SetupState.Fail, GameSetup.Evaluate(rules, new Host { Game = false, Files = { "winhttp.dll" } })[0].State);
    }

    [Fact]
    public void FreeDiskIsCheckedAndAnUnknownSizeIsNotAFailure()
    {
        var rules = new[] { new SetupRule(SetupKinds.FreeDisk, "", "Free some space.", Mb: 2000) };
        Assert.Equal(SetupState.Fail, GameSetup.Evaluate(rules, new Host { Disk = 500 })[0].State);
        Assert.Equal(SetupState.Ok, GameSetup.Evaluate(rules, new Host { Disk = 5000 })[0].State);
        Assert.Equal(SetupState.Ok, GameSetup.Evaluate(rules, new Host { Disk = null })[0].State);
    }

    [Fact]
    public void AGamesOwnSetupListIsReadFromItsGameJson()
    {
        var m = GameManifest.Parse(Manifest("""
            [ { "kind": "steam-running" },
              { "kind": "steam-app", "appId": 881100, "title": "Noita", "detail": "Install Noita from your Steam library." },
              { "kind": "game-file", "path": "mods/quant.ew/init.lua", "title": "Mod file", "optional": true },
              { "kind": "free-disk", "mb": 500 } ]
            """))!;
        Assert.Equal(4, m.Setup!.Count);
        Assert.Equal(881100u, m.Setup[1].AppId);
        Assert.True(m.Setup[2].Optional);
        Assert.Equal("mods/quant.ew/init.lua", m.Setup[2].Path);
        Assert.Equal(500, m.Setup[3].Mb);
        Assert.Empty(GameManifest.Parse(Manifest(""))!.Setup!);
    }

    [Theory]
    [InlineData("""[ { "kind": "run-program" } ]""")]                                            // only the checks the launcher knows
    [InlineData("""[ { "kind": "steam-app" } ]""")]                                                // a Steam app needs its id
    [InlineData("""[ { "kind": "game-file", "path": "../outside.txt" } ]""")]                      // a file stays inside the game's folder
    [InlineData("""[ { "kind": "game-file", "path": "C:/Windows/notepad.exe" } ]""")]
    [InlineData("""[ { "kind": "game-file", "path": "a\b" } ]""")]
    [InlineData("""[ { "kind": "game-file", "path": ".hidden/x" } ]""")]
    [InlineData("""[ { "kind": "free-disk", "mb": 0 } ]""")]
    [InlineData("""[ "steam-running" ]""")]
    public void ASetupListThatCouldDoMoreThanCheckRefusesTheWholeFile(string setup) => Assert.Null(GameManifest.Parse(Manifest(setup)));

    [Fact]
    public void ThereAreAtMostTwelveLines()
    {
        string many = "[" + string.Join(",", Enumerable.Repeat("{ \"kind\": \"steam-running\" }", 13)) + "]";
        Assert.Null(GameManifest.Parse(Manifest(many)));
    }
}
