// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Net;

namespace Cairn.Setup.Tests;

public class GameListTests
{
    private const string Good = """
        { "schema": 1,
          "games": [ { "id": "kcd2", "repo": "Cairn-client/mod-kcd2" }, { "id": "cp77" } ],
          "packs": [ { "id": "core", "repo": "Cairn-client/core" } ],
          "components": {
            "kcd2-game": { "kind": "steam-app", "name": "Kingdom Come: Deliverance II", "appId": 1771300 },
            "kcd2-tools": { "kind": "steam-app", "name": "Modding Tools", "appId": 2429020, "storePage": false } },
          "blocked": [ { "pack": "kcd2", "version": "0.1.0", "reason": "Saves are lost." }, { "pack": "kcd2", "max": "0.0.5", "reason": "Too old." } ],
          "notices": [ { "id": "n1", "title": "Sleep", "text": "Sleep needs both players.", "kind": "known-issue", "games": ["kcd2"] },
                       { "id": "n2", "title": "News", "text": "Hello", "until": "2026-01-01" } ] }
        """;

    [Fact]
    public void AGoodListIsRead()
    {
        var l = GameList.Parse(Good);
        Assert.NotNull(l);
        Assert.Equal(new[] { "kcd2", "cp77" }, l!.Games.Select(g => g.Id));
        Assert.Equal("Cairn-client/mod-kcd2", l.Games[0].Repo);
        Assert.Equal("", l.Games[1].Repo);
        Assert.Equal("Cairn-client/core", l.RepoOfPack("core"));
        Assert.Equal("Cairn-client/mod-kcd2", l.RepoOfPack("kcd2"));
        Assert.Equal(2429020u, l.Components["kcd2-tools"].AppId);
        Assert.False(l.Components["kcd2-tools"].HasStorePage);
        Assert.True(l.Components["kcd2-game"].HasStorePage);
    }

    [Fact]
    public void AGameCanSayItsExecutableIsASetting()
    {
        var l = GameList.Parse("""{ "schema": 1, "games": [ { "id": "a", "repo": "o/a", "exeSetting": true }, { "id": "b", "repo": "o/b" } ] }""")!;
        Assert.True(l.Games[0].ExeSetting);
        Assert.False(l.Games[1].ExeSetting);
    }

    [Theory]
    [InlineData("""{ "schema": 2, "games": [] }""")]
    [InlineData("""{ "schema": 1, "games": [ { "id": "../x" } ] }""")]
    [InlineData("""{ "schema": 1, "games": [ { "id": "a" }, { "id": "a" } ] }""")]
    [InlineData("""{ "schema": 1, "games": [ { "id": "a", "repo": "https://evil.test/x" } ] }""")]
    [InlineData("""{ "schema": 1, "packs": [ { "id": "core", "repo": "no-slash" } ] }""")]
    [InlineData("""{ "schema": 1, "components": { "x": { "kind": "steam-app", "name": "X" } } }""")]
    [InlineData("""{ "schema": 1, "blocked": [ { "pack": "kcd2", "reason": "neither version nor max" } ] }""")]
    [InlineData("""{ "schema": 1, "blocked": [ { "pack": "kcd2", "version": "0.1.0", "max": "0.2.0" } ] }""")]
    [InlineData("""{ "schema": 1, "notices": [ { "id": "n", "title": "" } ] }""")]
    [InlineData("""{ "schema": 1, "notices": [ { "id": "n", "title": "t", "kind": "advert" } ] }""")]
    [InlineData("not json")]
    public void ABadListIsRefused(string json) => Assert.Null(GameList.Parse(json));

    [Fact]
    public void AListThatIsTooBigIsRefused() =>
        Assert.Null(GameList.Parse("""{ "schema": 1, "notices": [] }""" + new string(' ', CatalogLimits.MaxFile)));

    [Fact]
    public void BlockedVersionsAreMatchedByVersionOrByRange()
    {
        var l = GameList.Parse(Good)!;
        Assert.Equal("Saves are lost.", l.BlockOf("kcd2", "0.1.0")!.Reason);
        Assert.Null(l.BlockOf("kcd2", "0.1.1"));
        Assert.Equal("Too old.", l.BlockOf("kcd2", "0.0.4")!.Reason);
        Assert.Equal("Too old.", l.BlockOf("KCD2", "0.0.5")!.Reason);
        Assert.Null(l.BlockOf("kcd2", "0.0.6"));
        Assert.Null(l.BlockOf("other", "0.1.0"));
        Assert.Null(l.BlockOf("kcd2", "not a version"));
    }

    [Fact]
    public void NoticesAreCurrentUntilTheirEndAndForTheGamesThePlayerHas()
    {
        var l = GameList.Parse(Good)!;
        var now = new DateTime(2026, 6, 1, 0, 0, 0, DateTimeKind.Utc);
        var issue = l.Notices.Single(n => n.Id == "n1");
        var news = l.Notices.Single(n => n.Id == "n2");
        Assert.True(issue.IsCurrent("0.2.0", now, g => g == "kcd2"));
        Assert.False(issue.IsCurrent("0.2.0", now, g => g == "cp77"));   // not for a player who does not have that game
        Assert.True(issue.IsCurrent("0.2.0", now));
        Assert.False(news.IsCurrent("0.2.0", now));                       // it ended on 1 January
        Assert.True(news.IsCurrent("0.2.0", new DateTime(2025, 12, 1, 0, 0, 0, DateTimeKind.Utc)));
    }

    [Fact]
    public void ANoticeCanBeLimitedToLauncherVersions()
    {
        var n = new Notice("n", "t", "x", Notice.News, Array.Empty<string>(), "0.3.0", "0.4.0", "");
        var now = DateTime.UtcNow;
        Assert.False(n.IsCurrent("0.2.9", now));
        Assert.True(n.IsCurrent("0.3.0", now));
        Assert.True(n.IsCurrent("0.4.0+abc", now));
        Assert.False(n.IsCurrent("0.4.1", now));
    }

    [Fact]
    public void NoticeTextHasNoMarkupOrControlCharacters()
    {
        var l = GameList.Parse("""{ "schema": 1, "notices": [ { "id": "n", "title": "<b>Hi</b>\u0007", "text": "a\nb" } ] }""")!;
        Assert.Equal("bHi/b", l.Notices[0].Title);
        Assert.Equal("ab", l.Notices[0].Text);
    }
}

public class GameManifestTests
{
    private const string Good = """
        { "schema": 1, "id": "kcd2", "name": "Kingdom Come: Deliverance II", "developer": "Warhorse Studios", "franchise": "Kingdom Come",
          "steamAppId": 2429020, "storeAppId": 1771300, "exe": "KingdomCome.exe", "description": "Explore Bohemia.", "accent": "#E8C872",
          "images": { "background": "kcd2.jpg", "logo": "kcd2-logo.png", "hero": "kcd2-hero.jpg", "poster": "kcd2-poster.jpg", "screenshots": ["shots/kcd2-1.jpg", "shots/kcd2-2.jpg"] },
          "requires": [ "kcd2-game", { "id": "extra", "kind": "steam-app", "name": "Extra", "appId": 5 } ],
          "features": [ { "title": "Sessions", "items": [ { "name": "Host", "detail": "Join by code." }, { "name": "Map", "detail": "On the map.", "status": "inTest" }, { "name": "Later", "status": "planned" } ] } ],
          "content": [ "kcd2-start-saves" ] }
        """;

    [Fact]
    public void AGoodGameIsRead()
    {
        var m = GameManifest.Parse(Good);
        Assert.NotNull(m);
        Assert.Equal("kcd2", m!.Id);
        Assert.Equal(2429020u, m.SteamAppId);
        Assert.Equal(1771300u, m.StoreAppId);
        Assert.True(m.Supported);
        Assert.Equal("#E8C872", m.Accent);
        Assert.Equal(2, m.Images.Screenshots.Count);
        var f = m.Features.Single().Items;
        Assert.Equal(new[] { FeatureState.Ready, FeatureState.InTest, FeatureState.Planned }, f.Select(x => x.State));
        Assert.Equal(new[] { "kcd2-start-saves" }, m.Content);
    }

    [Fact]
    public void ARequirementIsAShortNameOrAnInlineDefinition()
    {
        var list = GameList.Parse("""{ "schema": 1, "components": { "kcd2-game": { "kind": "steam-app", "name": "KCD2", "appId": 1771300 } } }""")!;
        var req = GameManifest.Parse(Good)!.ResolveRequirements(list);
        Assert.Equal(new[] { "KCD2", "Extra" }, req.Select(c => c.Name));
        // a name no definition has is left out, not invented
        Assert.Single(GameManifest.Parse(Good)!.ResolveRequirements(GameList.Empty));
    }

    [Fact]
    public void TheHowToLinesAreReadAndBounded()
    {
        var m = GameManifest.Parse("""{ "schema": 1, "id": "a", "name": "A", "hostSteps": ["Press Host.", "  Load a save. "], "joinSteps": ["Press Join."] }""")!;
        Assert.Equal(new[] { "Press Host.", "Load a save." }, m.HostSteps);
        Assert.Equal(new[] { "Press Join." }, m.JoinSteps);
        Assert.Empty(GameManifest.Parse("""{ "schema": 1, "id": "a", "name": "A" }""")!.HostSteps!);
        Assert.Null(GameManifest.Parse("""{ "schema": 1, "id": "a", "name": "A", "hostSteps": [1] }"""));
        Assert.Null(GameManifest.Parse("""{ "schema": 1, "id": "a", "name": "A", "joinSteps": ["1","2","3","4","5","6","7","8","9"] }"""));
    }

    [Fact]
    public void ASharedRequirementIsDefinedOnceForSeveralGames()
    {
        var list = GameList.Parse("""{ "schema": 1, "components": { "vcredist": { "kind": "steam-app", "name": "Runtime", "appId": 7 } } }""")!;
        var a = GameManifest.Parse("""{ "schema": 1, "id": "a", "name": "A", "requires": ["vcredist"] }""")!;
        var b = GameManifest.Parse("""{ "schema": 1, "id": "b", "name": "B", "requires": ["vcredist"] }""")!;
        Assert.Same(a.ResolveRequirements(list).Single(), b.ResolveRequirements(list).Single());
    }

    [Theory]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "images": { "logo": "https://evil.test/x.png" } }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "images": { "logo": "../x.png" } }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "images": { "logo": "/etc/x.png" } }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "images": { "logo": "x.exe" } }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "images": { "screenshots": ["a:b.jpg"] } }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "accent": "red" }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "exe": "..\\evil.exe" }""")]
    [InlineData("""{ "schema": 1, "id": "a b", "name": "A" }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "" }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "features": [ { "title": "T", "items": [ { "name": "x", "status": "great" } ] } ] }""")]
    [InlineData("""{ "schema": 1, "id": "a", "name": "A", "requires": [ 5 ] }""")]
    [InlineData("""{ "schema": 2, "id": "a", "name": "A" }""")]
    public void ABadGameIsRefused(string json) => Assert.Null(GameManifest.Parse(json));

    [Fact]
    public void AGameCanBeListedAsNotSupportedYet()
    {
        var m = GameManifest.Parse("""{ "schema": 1, "id": "a", "name": "A", "supported": false }""");
        Assert.False(m!.Supported);
    }
}

public class CatalogClientTests : IDisposable
{
    private readonly string cache = Path.Combine(Path.GetTempPath(), "cairn-catalog-" + Guid.NewGuid().ToString("N")[..8]);
    public void Dispose() { try { Directory.Delete(cache, true); } catch { } }

    private const string Games = """{ "schema": 1, "games": [ { "id": "a", "repo": "Cairn-client/mod-a" }, { "id": "b" } ] }""";
    private static string Game(string id, string name) => $$"""{ "schema": 1, "id": "{{id}}", "name": "{{name}}", "images": { "logo": "{{id}}.png" } }""";

    private sealed class Fake(Func<string, string?> answer) : HttpMessageHandler
    {
        public readonly List<string> Asked = new();
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken ct)
        {
            string url = request.RequestUri!.ToString();
            Asked.Add(url);
            var body = answer(url);
            return Task.FromResult(body is null
                ? new HttpResponseMessage(HttpStatusCode.NotFound)
                : new HttpResponseMessage(HttpStatusCode.OK) { Content = new StringContent(body) });
        }
    }

    private CatalogClient Client(Fake f, string? gamesBase = null, bool? repos = null) =>
        new(new HttpClient(f), cache, gamesBase, repos);

    [Fact]
    public async Task AGameWithARepositoryReadsItsGameJsonFromItsLatestRelease()
    {
        var f = new Fake(u => u switch
        {
            CatalogClient.DefaultGamesBase + "games.json" => Games,
            "https://github.com/Cairn-client/mod-a/releases/latest/download/game.json" => Game("a", "Game A"),
            CatalogClient.DefaultGamesBase + "games/b/game.json" => Game("b", "Game B"),
            _ => null,
        });
        var snap = await Client(f).LoadAsync();
        Assert.NotNull(snap);
        Assert.False(snap!.FromCache);
        Assert.Equal(new[] { "Game A", "Game B" }, snap.Games.Select(g => g.Manifest.Name));
        // its pictures are looked up beside the file that names them
        Assert.Equal("https://github.com/Cairn-client/mod-a/releases/latest/download/a.png", snap.Games[0].Picture("a.png"));
        Assert.Equal(CatalogClient.DefaultGamesBase + "games/b/b.png", snap.Games[1].Picture("b.png"));
        Assert.Equal("", snap.Games[1].Picture(""));
    }

    [Fact]
    public async Task AGameWhoseRepositoryHasNoGameJsonYetFallsBackToTheGamesRepository()
    {
        var f = new Fake(u => u switch
        {
            CatalogClient.DefaultGamesBase + "games.json" => Games,
            CatalogClient.DefaultGamesBase + "games/a/game.json" => Game("a", "A from the list repo"),
            CatalogClient.DefaultGamesBase + "games/b/game.json" => Game("b", "B"),
            _ => null,
        });
        var snap = await Client(f).LoadAsync();
        Assert.Equal("A from the list repo", snap!.Games[0].Manifest.Name);
    }

    [Fact]
    public async Task AGameJsonThatIsAnotherGameOrBadIsNotUsed()
    {
        var f = new Fake(u => u switch
        {
            CatalogClient.DefaultGamesBase + "games.json" => Games,
            "https://github.com/Cairn-client/mod-a/releases/latest/download/game.json" => Game("zzz", "Impostor"),
            CatalogClient.DefaultGamesBase + "games/a/game.json" => "{ nope",
            CatalogClient.DefaultGamesBase + "games/b/game.json" => Game("b", "B"),
            _ => null,
        });
        var snap = await Client(f).LoadAsync();
        Assert.Equal(new[] { "B" }, snap!.Games.Select(g => g.Manifest.Name));
        Assert.True(snap.FromCache);   // one game could not be had: what is shown is not the whole answer
    }

    [Fact]
    public async Task OfflineTheLastCopyOnDiskIsUsed()
    {
        var online = new Fake(u => u switch
        {
            CatalogClient.DefaultGamesBase + "games.json" => Games,
            "https://github.com/Cairn-client/mod-a/releases/latest/download/game.json" => Game("a", "Game A"),
            CatalogClient.DefaultGamesBase + "games/b/game.json" => Game("b", "Game B"),
            _ => null,
        });
        Assert.NotNull(await Client(online).LoadAsync());

        var snap = await Client(new Fake(_ => null)).LoadAsync();
        Assert.NotNull(snap);
        Assert.True(snap!.FromCache);
        Assert.Equal(new[] { "Game A", "Game B" }, snap.Games.Select(g => g.Manifest.Name));
        Assert.Equal("https://github.com/Cairn-client/mod-a/releases/latest/download/a.png", snap.Games[0].Picture("a.png"));
    }

    [Fact]
    public async Task WithNeitherNetworkNorCopyThereIsNoCatalog() =>
        Assert.Null(await Client(new Fake(_ => null)).LoadAsync());

    [Fact]
    public async Task ABrokenListOnTheNetworkDoesNotReplaceTheCopyOnDisk()
    {
        var good = new Fake(u => u == CatalogClient.DefaultGamesBase + "games.json" ? Games : u.Contains("game.json") ? Game(u.Contains("/a/") || u.Contains("mod-a") ? "a" : "b", "X") : null);
        Assert.NotNull(await Client(good).LoadAsync());
        var broken = new Fake(u => u == CatalogClient.DefaultGamesBase + "games.json" ? "{ broken" : null);
        var snap = await Client(broken).LoadAsync();
        Assert.True(snap!.FromCache);
        Assert.Equal(2, snap.List.Games.Count);
    }

    [Fact]
    public async Task ALocalCopyOfTheCatalogServesEveryGameItself()
    {
        var f = new Fake(u => u switch
        {
            "http://localhost:9/games.json" => Games,
            "http://localhost:9/games/a/game.json" => Game("a", "A"),
            "http://localhost:9/games/b/game.json" => Game("b", "B"),
            _ => null,
        });
        var snap = await Client(f, "http://localhost:9").LoadAsync();
        Assert.Equal(2, snap!.Games.Count);
        Assert.DoesNotContain(f.Asked, u => u.StartsWith("https://github.com"));
    }
}

public class PackPlannerTests
{
    private static PackInfo Pack(string id, string version, params (string id, string min)[] deps) =>
        new(id, "g", version, id + ".zip", new string('a', 64), 10, DependsOn: deps.Select(d => new PackDependency(d.id, d.min)).ToList());

    private static InstalledPack Have(string id, string version) => new(id, version, Array.Empty<ManifestEntry>());

    [Fact]
    public void APackWithNoDependenciesIsAlone()
    {
        var plan = PackPlanner.Plan(Pack("mod", "1.0.0"), _ => null, _ => null);
        Assert.True(plan.Ok);
        Assert.Equal(new[] { "mod" }, plan.Order.Select(p => p.Id));
    }

    [Fact]
    public void WhatItDependsOnComesFirstAndOnlyWhenMissingOrTooOld()
    {
        var mod = Pack("mod", "1.0.0", ("core", "1.2.0"), ("saves", "1.0.0"));
        var offered = new Dictionary<string, PackInfo> { ["core"] = Pack("core", "1.3.0"), ["saves"] = Pack("saves", "1.0.0") };

        var none = PackPlanner.Plan(mod, _ => null, id => offered.GetValueOrDefault(id));
        Assert.Equal(new[] { "core", "saves", "mod" }, none.Order.Select(p => p.Id));

        var coreOld = PackPlanner.Plan(mod, id => id == "core" ? Have("core", "1.1.0") : Have("saves", "1.0.0"), id => offered.GetValueOrDefault(id));
        Assert.Equal(new[] { "core", "mod" }, coreOld.Order.Select(p => p.Id));

        var enough = PackPlanner.Plan(mod, id => id == "core" ? Have("core", "1.2.0") : Have("saves", "2.0.0"), id => offered.GetValueOrDefault(id));
        Assert.Equal(new[] { "mod" }, enough.Order.Select(p => p.Id));
    }

    [Fact]
    public void ADependencyOfADependencyIsFollowedAndSharedOnesAreOnce()
    {
        var offered = new Dictionary<string, PackInfo>
        {
            ["a"] = Pack("a", "1.0.0", ("base", "1.0.0")),
            ["b"] = Pack("b", "1.0.0", ("base", "1.0.0")),
            ["base"] = Pack("base", "1.0.0"),
        };
        var plan = PackPlanner.Plan(Pack("mod", "1.0.0", ("a", "1.0.0"), ("b", "1.0.0")), _ => null, id => offered.GetValueOrDefault(id));
        Assert.Equal(new[] { "base", "a", "b", "mod" }, plan.Order.Select(p => p.Id));
    }

    [Fact]
    public void ADependencyThatCannotBeHadIsAProblem()
    {
        var missing = PackPlanner.Plan(Pack("mod", "1.0.0", ("core", "1.0.0")), _ => null, _ => null);
        Assert.False(missing.Ok);
        Assert.Contains("core", missing.Problem);
        Assert.Empty(missing.Order);

        var tooOld = PackPlanner.Plan(Pack("mod", "1.0.0", ("core", "2.0.0")), _ => null, id => Pack(id, "1.5.0"));
        Assert.False(tooOld.Ok);
        Assert.Contains("1.5.0", tooOld.Problem);
    }

    [Fact]
    public void ALoopOfDependenciesIsAProblemNotAHang()
    {
        var offered = new Dictionary<string, PackInfo> { ["a"] = Pack("a", "1.0.0", ("b", "1.0.0")), ["b"] = Pack("b", "1.0.0", ("a", "1.0.0")) };
        var plan = PackPlanner.Plan(Pack("a", "1.0.0", ("b", "1.0.0")), _ => null, id => offered.GetValueOrDefault(id));
        Assert.False(plan.Ok);
        Assert.Contains("itself", plan.Problem);
    }
}

public class PackFormatTests
{
    [Fact]
    public void ACatalogEntryCanNameWhatItDependsOnAndItsKind()
    {
        var idx = PackIndex.Parse("""
            { "schema": 2, "id": "kcd2-saves", "game": "kcd2", "version": "1.0.0", "file": "s.zip", "kind": "content",
              "sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "size": 5,
              "dependsOn": [ { "id": "core", "min": "1.2.0" } ] }
            """);
        var p = idx!.Packs.Single();
        Assert.Equal(PackKinds.Content, p.Kind);
        Assert.Equal("core", p.Dependencies.Single().Id);
        Assert.Equal("1.2.0", p.Dependencies.Single().Min);
    }

    [Fact]
    public void ACatalogEntryWithoutKindIsAMod()
    {
        var idx = PackIndex.Parse("""{ "schema": 2, "id": "m", "game": "m", "version": "1.0.0", "file": "m.zip", "sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "size": 5 }""");
        Assert.Equal(PackKinds.Mod, idx!.Packs.Single().Kind);
        Assert.Empty(idx.Packs.Single().Dependencies);
    }

    [Theory]
    [InlineData("\"kind\": \"program\"")]
    [InlineData("\"dependsOn\": [ { \"id\": \"../x\", \"min\": \"1.0.0\" } ]")]
    [InlineData("\"dependsOn\": [ { \"id\": \"core\", \"min\": \"latest\" } ]")]
    [InlineData("\"dependsOn\": \"core\"")]
    public void ABadKindOrDependencyIsRefused(string extra)
    {
        var json = "{ \"schema\": 2, \"id\": \"m\", \"game\": \"m\", \"version\": \"1.0.0\", \"file\": \"m.zip\", \"sha256\": \"" + new string('a', 64) + "\", \"size\": 5, " + extra + " }";
        Assert.Null(PackIndex.Parse(json));
    }

    [Fact]
    public void AContractCanBeContentWithNoProgramsAndAKnownTarget()
    {
        var c = PackContract.Parse("""{ "schema": 2, "id": "saves", "version": "1.0.0", "kind": "content", "installTo": "start-save" }""");
        Assert.NotNull(c);
        Assert.Equal(PackKinds.Content, c!.Kind);
        Assert.Equal(PackContract.StartSaveTarget, c.InstallTo);
    }

    [Theory]
    [InlineData("""{ "schema": 2, "id": "saves", "version": "1.0.0", "kind": "content", "installTo": "C:/Windows" }""")]
    [InlineData("""{ "schema": 2, "id": "saves", "version": "1.0.0", "kind": "content", "agent": "bin/a.exe" }""")]
    [InlineData("""{ "schema": 2, "id": "saves", "version": "1.0.0", "kind": "weird" }""")]
    [InlineData("""{ "schema": 2, "id": "saves", "version": "1.0.0", "kind": "content", "menuProtocol": 0 }""")]
    public void ABadContentContractIsRefused(string json) => Assert.Null(PackContract.Parse(json));

    [Fact]
    public void TheMenuProtocolDefaultsToOneAndIsRead()
    {
        var c = PackContract.Parse(FakeSteam.ContractJson);
        Assert.Equal(1, c!.MenuProtocol);
        var two = PackContract.Parse(FakeSteam.ContractJson.Replace("\"schema\": 2,", "\"schema\": 2, \"menuProtocol\": 2,"));
        Assert.Equal(2, two!.MenuProtocol);
        Assert.True(two.MenuProtocol > PackContract.SupportedMenuProtocol);
    }

    [Fact]
    public void ContentIsCopiedButNeverOverwritesWhatThePlayerHas()
    {
        string root = Path.Combine(Path.GetTempPath(), "cairn-content-" + Guid.NewGuid().ToString("N")[..8]);
        try
        {
            string src = Path.Combine(root, "src"), dst = Path.Combine(root, "dst");
            Directory.CreateDirectory(Path.Combine(src, "knight")); Directory.CreateDirectory(Path.Combine(dst, "knight"));
            File.WriteAllText(Path.Combine(src, "knight", "a.whs"), "new");
            File.WriteAllText(Path.Combine(src, "knight", "b.whs"), "new");
            File.WriteAllText(Path.Combine(dst, "knight", "a.whs"), "mine");
            Assert.Equal(1, ContentInstall.CopyMissing(src, dst));
            Assert.Equal("mine", File.ReadAllText(Path.Combine(dst, "knight", "a.whs")));
            Assert.Equal("new", File.ReadAllText(Path.Combine(dst, "knight", "b.whs")));
            Assert.Equal(0, ContentInstall.CopyMissing(src, dst));
            Assert.Equal(0, ContentInstall.CopyMissing(Path.Combine(root, "nothing"), dst));
        }
        finally { try { Directory.Delete(root, true); } catch { } }
    }

    [Fact]
    public void TheBetaChannelFollowsTheNewestReleaseAndStableOnlyTheStableOne()
    {
        PackInfo P(string v) => new("m", "m", v, "m.zip", new string('a', 64), 5);
        Assert.Equal("0.2.0", PackChannels.Choose(PackChannels.Beta, P("0.1.0"), P("0.2.0"))!.Version);
        Assert.Equal("0.1.0", PackChannels.Choose(PackChannels.Stable, P("0.1.0"), P("0.2.0"))!.Version);
        Assert.Equal("0.1.0", PackChannels.Choose(PackChannels.Beta, P("0.1.0"), P("0.1.0"))!.Version);
        Assert.Equal("0.3.0", PackChannels.Choose(PackChannels.Beta, P("0.3.0"), P("0.2.0"))!.Version);   // a beta that is older than stable is never offered
        Assert.Equal("0.2.0", PackChannels.Choose(PackChannels.Beta, null, P("0.2.0"))!.Version);
        Assert.Null(PackChannels.Choose(PackChannels.Stable, null, P("0.2.0")));
        Assert.True(PackChannels.IsKnown("beta") && PackChannels.IsKnown("stable") && !PackChannels.IsKnown("nightly"));
    }
}

public class OwnSessionTests
{
    [Fact]
    public void AGameCanSayItsModRunsTheSessionItself()
    {
        string Json(string extra) => "{ \"schema\": 1, \"id\": \"noita\", \"name\": \"Noita\", \"exe\": \"noita.exe\", \"accent\": \"#E8A35A\"" + extra + " }";
        Assert.True(GameManifest.Parse(Json(", \"ownSession\": true"))!.OwnSession);
        Assert.False(GameManifest.Parse(Json(""))!.OwnSession);
        Assert.False(GameManifest.Parse(Json(", \"ownSession\": \"yes\""))!.OwnSession);   // only a real true counts
    }
}
