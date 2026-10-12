// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.IO.Compression;
using System.Net;
using System.Security.Cryptography;
using System.Text;

namespace Cairn.Setup.Tests;

public class PackIndexTests
{
    private const string Good = """
        { "schema": 2, "id": "kcd2", "game": "kcd2", "version": "0.2.0", "upstream": "0.48.6", "file": "Cairn-Pack-kcd2-0.2.0.zip",
          "sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "size": 1234, "launcherMin": "0.1.0" }
        """;

    [Fact]
    public void AGoodCatalogIsRead()
    {
        var index = PackIndex.Parse(Good);
        Assert.NotNull(index);
        var p = index!.ForGame("KCD2");
        Assert.Equal("Cairn-Pack-kcd2-0.2.0.zip", p!.File);
        Assert.Equal(new string('a', 64), p.Sha256);   // normalised to lower case
        Assert.Equal("0.1.0", p.LauncherMin);
        Assert.Equal("0.48.6", p.Upstream);
        Assert.Null(index.ForGame("fo4"));
    }

    [Theory]
    [InlineData("not json")]
    [InlineData("""{ "schema": 1, "id": "kcd2" }""")]                                                // a schema this launcher does not know
    [InlineData("""{ "schema": 2, "id": "kcd2", "game": "kcd2" }""")]                                // no file, no checksum
    public void ACatalogItDoesNotUnderstandIsRefused(string json) => Assert.Null(PackIndex.Parse(json));

    [Theory]
    [InlineData("../evil", "a.zip", 64, "0.2.0")]                  // a path in the id
    [InlineData("kcd2", "..\\a.zip", 64, "0.2.0")]                 // a path in the file name
    [InlineData("kcd2", "a.zip", 10, "0.2.0")]                     // not a sha256
    [InlineData("kcd2", "a.zip", 64, "latest")]                    // not a plain version (it becomes part of a download address)
    public void AMalformedEntryRefusesTheWholeCatalog(string id, string file, int shaLen, string version)
    {
        string json = "{\"schema\":2,\"id\":\"" + id.Replace("\\", "\\\\") + "\",\"game\":\"kcd2\",\"version\":\"" + version + "\"," +
                      "\"file\":\"" + file.Replace("\\", "\\\\") + "\",\"sha256\":\"" + new string('a', shaLen) + "\",\"size\":5}";
        Assert.Null(PackIndex.Parse(json));
    }
}

public class PackCatalogTests
{
    private static readonly PackInfo Info = new("kcd2", "kcd2", "0.2.0", "Cairn-Pack-kcd2-0.2.0.zip", new string('a', 64), 5);

    [Fact]
    public void ThePackAndItsCatalogLiveInTheGamesOwnRepository()
    {
        Assert.Equal("https://github.com/Cairn-client/mod-kcd2/releases/latest/download/pack-index.json", PackCatalog.IndexUrl("Cairn-client/mod-kcd2")!.ToString());
        Assert.Equal("https://github.com/Cairn-client/mod-kcd2/releases/download/0.2.0/Cairn-Pack-kcd2-0.2.0.zip", PackCatalog.PackUrl("Cairn-client/mod-kcd2", Info)!.ToString());
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("no-slash")]
    [InlineData("a/b/c")]
    [InlineData("owner/../name")]
    [InlineData("owner/na me")]
    public void ARepositoryThatIsNotPlainGivesNoAddress(string? repo)
    {
        Assert.Null(PackCatalog.IndexUrl(repo));
        Assert.Null(PackCatalog.PackUrl(repo, Info));
    }

    [Theory]
    [InlineData("0.1.0", "0.2.0", true)]
    [InlineData("0.2.0", "0.2.0", false)]
    [InlineData("0.2.1", "0.2.0", false)]     // never "update" to an older one
    [InlineData("0.9.0", "0.10.0", true)]     // versions compare as numbers, not text
    public void AnUpdateIsOfferedOnlyForANewerVersion(string installedVersion, string latestVersion, bool offered)
    {
        var installed = new InstalledPack("kcd2", installedVersion, Array.Empty<ManifestEntry>());
        var latest = Info with { Version = latestVersion };
        Assert.Equal(offered, PackCatalog.UpdateFor(installed, latest) is not null);
        Assert.Null(PackCatalog.UpdateFor(null, latest));
    }

    [Theory]
    [InlineData("", "0.1.0", true)]           // the pack names no minimum
    [InlineData("0.1.0", "0.1.0", true)]
    [InlineData("0.2.0", "0.1.9", false)]
    [InlineData("0.1.0", "0.2.3+abc", true)]  // the launcher's informational version carries a hash
    [InlineData("junk", "0.1.0", false)]      // a minimum that cannot be read does not let the pack through
    public void ALauncherThatIsTooOldIsToldSo(string min, string launcher, bool enough) =>
        Assert.Equal(enough, PackCatalog.LauncherIsEnough(min, launcher));
}

public class PackContractTests
{
    [Fact]
    public void AGoodContractIsRead()
    {
        var c = PackContract.Parse(FakeSteam.ContractJson);
        Assert.NotNull(c);
        Assert.Equal("kdcmp", c!.ModFolder);
        Assert.Equal("KCD2MP_", c.LuaPrefix);
        Assert.Equal("bin/Agent.exe", c.Agent);
        Assert.Equal(new[] { "cairn" }, c.ReplacesFolders);
        Assert.Equal("[TEST-EVT] v1 ", c.Lua!["eventPrefix"]);
        Assert.Equal("Test_Model", c.Lua["startGameModel"]);
    }

    [Theory]
    [InlineData("\"startGameModel\": \"evil(); os.exit()\"")]      // not a plain function name: it is put into a Lua call
    [InlineData("\"startGameModel\": \"\"")]
    [InlineData("\"unknownKey\": \"Test_Fn\"")]                      // only the names the launcher knows
    [InlineData("\"eventPrefix\": \"has \\\"quote\"")]               // a prefix with a quote or backslash
    [InlineData("\"startGameEvent\": \"a b\"")]
    [InlineData("\"helperFlag\": \"w159\"")]                          // a flag starts with two dashes
    [InlineData("\"helperFlag\": \"--a b\"")]
    [InlineData("\"helperTag\": \"W 159\"")]
    public void ALuaNameThatCouldBecomeCodeIsRefused(string entry)
    {
        var json = FakeSteam.ContractJson.Replace("\"lua\": {", "\"lua\": { " + entry + ",");
        Assert.Null(PackContract.Parse(json));
    }

    [Fact]
    public void TheAgentsHelperFlagAndTagAreRead()
    {
        var json = FakeSteam.ContractJson.Replace("\"lua\": {", "\"lua\": { \"helperFlag\": \"--w159\", \"helperTag\": \"W159\",");
        var c = PackContract.Parse(json);
        Assert.Equal("--w159", c!.Lua!["helperFlag"]);
        Assert.Equal("W159", c.Lua["helperTag"]);
    }

    [Fact]
    public void ThePluginsLogNameIsReadAndMustBeAPlainFileName()
    {
        var ok = PackContract.Parse(FakeSteam.ContractJson.Replace("\"lua\": {", "\"lua\": { \"nativeLog\": \"kcdmp-native.log\","));
        Assert.Equal("kcdmp-native.log", ok!.Lua!["nativeLog"]);
        Assert.Null(PackContract.Parse(FakeSteam.ContractJson.Replace("\"lua\": {", "\"lua\": { \"nativeLog\": \"..\\\\x.log\",")));
    }

    [Fact]
    public void AContractWithoutLuaNamesIsStillRead()
    {
        var json = System.Text.RegularExpressions.Regex.Replace(FakeSteam.ContractJson, @",\s*""lua"": \{[^}]*\}", "");
        var c = PackContract.Parse(json);
        Assert.NotNull(c);
        Assert.True(c!.Lua is null || c.Lua.Count == 0);
    }

    [Theory]
    [InlineData("""{ "schema": 1 }""")]
    [InlineData("not json")]
    [InlineData("""{ "schema": 2, "id": "kcd2", "version": "1.0.0", "modFolder": "..", "luaPrefix": "X_", "agent": "bin/a.exe", "relay": "bin/r.exe", "native": "bin/n.dll" }""")]          // a path as a mod folder
    [InlineData("""{ "schema": 2, "id": "kcd2", "version": "1.0.0", "modFolder": "m", "luaPrefix": "X_", "agent": "../a.exe", "relay": "bin/r.exe", "native": "bin/n.dll" }""")]        // a program outside the pack
    [InlineData("""{ "schema": 2, "id": "kcd2", "version": "1.0.0", "modFolder": "m", "luaPrefix": "X_", "agent": "C:/a.exe", "relay": "bin/r.exe", "native": "bin/n.dll" }""")]        // an absolute path
    public void AContractThatIsNotSafeOrNotKnownIsRefused(string json) => Assert.Null(PackContract.Parse(json));

    [Fact]
    public void AProgramResolvesInsideThePackFolder()
    {
        var path = PackContract.Resolve(@"C:\store\kcd2\files", "bin/Agent.exe");
        Assert.Equal(@"C:\store\kcd2\files\bin\Agent.exe", path);
    }
}

public class PackInstallerTests : IDisposable
{
    private readonly string _root = Path.Combine(AppContext.BaseDirectory, "scratch", "packs-" + Guid.NewGuid().ToString("N")[..8]);
    public void Dispose() { try { Directory.Delete(_root, recursive: true); } catch { } }

    private static string Hex(byte[] b) => Convert.ToHexString(SHA256.HashData(b)).ToLowerInvariant();

    private static readonly string[] Programs = { "Agent.exe", "Relay.exe", "Native.dll", "brand.pak" };

    /// <summary>
    /// A pack zip: pack.json, manifest.txt, mod\... and bin\... Each file is listed with its real size and hash unless told otherwise.
    /// </summary>
    private static byte[] BuildZip(Dictionary<string, string> modFiles, Func<string, string, string>? manifestLine = null, bool withManifest = true,
        Dictionary<string, string>? extraUnlisted = null, string? contract = null, bool withPrograms = true, string version = "0.2.0")
    {
        using var ms = new MemoryStream();
        using (var zip = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
        {
            void Add(string name, string text)
            {
                var e = zip.CreateEntry(name);
                using var w = new StreamWriter(e.Open(), new UTF8Encoding(false));
                w.Write(text);
            }
            string Line(string kind, string rel, string text) =>
                manifestLine?.Invoke(rel, text) ?? $"{kind}|{rel}|{Encoding.UTF8.GetByteCount(text)}|{Hex(Encoding.UTF8.GetBytes(text))}";
            var lines = modFiles.Select(kv => Line("MOD", kv.Key, kv.Value)).ToList();
            if (withPrograms) lines.AddRange(Programs.Select(p => Line("BIN", p, "stub " + p)));
            if (withManifest) Add("manifest.txt", string.Join("\n", lines) + "\n");
            if (contract is not null || withPrograms) Add("pack.json", contract ?? FakeSteam.ContractJson.Replace("\"version\": \"0.1.0\"", $"\"version\": \"{version}\""));
            foreach (var kv in modFiles) Add("mod/" + kv.Key.Replace('\\', '/'), kv.Value);
            if (withPrograms) foreach (var p in Programs) Add("bin/" + p, "stub " + p);
            if (extraUnlisted is not null) foreach (var kv in extraUnlisted) Add(kv.Key, kv.Value);
        }
        return ms.ToArray();
    }

    private static PackInfo InfoFor(byte[] zip, string id = "kcd2", string version = "0.2.0") =>
        new(id, id, version, "Cairn-Pack-" + id + "-" + version + ".zip", Hex(zip), zip.Length);

    private sealed class FakeHandler(Func<HttpResponseMessage> respond) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken ct) => Task.FromResult(respond());
    }

    private static HttpClient Serving(byte[] body, HttpStatusCode code = HttpStatusCode.OK) =>
        new(new FakeHandler(() => new HttpResponseMessage(code) { Content = new ByteArrayContent(body) }));

    private static readonly Dictionary<string, string> Files = new() { ["mod.manifest"] = "manifest text", ["Data\\kdcmp.pak"] = "pak bytes" };

    private static readonly Uri AnyUrl = new("https://example.test/p.zip");

    [Fact]
    public async Task AGoodPackIsDownloadedVerifiedAndStored()
    {
        var zip = BuildZip(Files);
        var progress = new List<double>();
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), AnyUrl, _root, Serving(zip), new Progress<double>(progress.Add));
        Assert.True(r.Ok, r.Message);

        var pack = PackStore.Read(_root, "kcd2");
        Assert.NotNull(pack);
        Assert.Equal("0.2.0", pack!.Version);
        Assert.Equal(2 + Programs.Length, pack.Files.Count);
        Assert.Equal("pak bytes", File.ReadAllText(Path.Combine(PackStore.FilesDir(_root, "kcd2"), "mod", "Data", "kdcmp.pak")));
        Assert.Equal("stub Agent.exe", File.ReadAllText(PackContract.Resolve(PackStore.FilesDir(_root, "kcd2"), pack.Contract!.Agent)));
        Assert.Equal("kdcmp", pack.Contract!.ModFolder);
        Assert.False(File.Exists(Path.Combine(_root, "kcd2.download")));   // nothing half-done is left behind
        Assert.False(Directory.Exists(Path.Combine(_root, "kcd2.new")));
    }

    [Fact]
    public async Task ADownloadingAgainReplacesThePreviousCopy()
    {
        var first = BuildZip(Files);
        await PackInstaller.DownloadAsync(InfoFor(first), AnyUrl, _root, Serving(first));
        var second = BuildZip(new Dictionary<string, string> { ["mod.manifest"] = "new manifest" }, version: "0.3.0");
        var r = await PackInstaller.DownloadAsync(InfoFor(second, version: "0.3.0"), AnyUrl, _root, Serving(second));
        Assert.True(r.Ok, r.Message);
        Assert.Equal("0.3.0", PackStore.Read(_root, "kcd2")!.Version);
        Assert.Single(PackStore.Read(_root, "kcd2")!.Files.Where(f => f.Kind == "MOD"));
        Assert.False(File.Exists(Path.Combine(PackStore.FilesDir(_root, "kcd2"), "mod", "Data", "kdcmp.pak")));   // the old file is gone
    }

    [Fact]
    public async Task AnUpdateGoesThroughWhileALogIsInUse()
    {
        var first = BuildZip(Files);
        await PackInstaller.DownloadAsync(InfoFor(first), AnyUrl, _root, Serving(first));
        var bin = Path.Combine(PackStore.FilesDir(_root, "kcd2"), PackStore.BinFolder);
        // the agent of the last session still has its log open
        using var held = new FileStream(Path.Combine(bin, "agent.log"), FileMode.Create, FileAccess.ReadWrite, FileShare.None);

        var second = BuildZip(Files, version: "0.3.0");
        var r = await PackInstaller.DownloadAsync(InfoFor(second, version: "0.3.0"), AnyUrl, _root, Serving(second));
        Assert.True(r.Ok, r.Message);
        Assert.Equal("0.3.0", PackStore.Read(_root, "kcd2")!.Version);
        Assert.Equal("stub Relay.exe", File.ReadAllText(Path.Combine(PackStore.FilesDir(_root, "kcd2"), PackStore.BinFolder, "Relay.exe")));
    }

    [Fact]
    public async Task AnUpdateThatCannotReplaceAFileInUseSaysSoPlainly()
    {
        var first = BuildZip(Files);
        await PackInstaller.DownloadAsync(InfoFor(first), AnyUrl, _root, Serving(first));
        var bin = Path.Combine(PackStore.FilesDir(_root, "kcd2"), PackStore.BinFolder);
        using var held = new FileStream(Path.Combine(bin, "Relay.exe"), FileMode.Open, FileAccess.Read, FileShare.None);   // a program of the pack, running

        var second = BuildZip(Files, version: "0.3.0");
        var r = await PackInstaller.DownloadAsync(InfoFor(second, version: "0.3.0"), AnyUrl, _root, Serving(second));
        Assert.False(r.Ok);
        Assert.Contains("still running", r.Message);
        Assert.DoesNotContain("process cannot access", r.Message);
    }

    [Fact]
    public async Task AnUpdateKeepsWhatTheProgramsWroteBesideThemSuchAsTheirSettingsAndLogs()
    {
        var first = BuildZip(Files);
        await PackInstaller.DownloadAsync(InfoFor(first), AnyUrl, _root, Serving(first));
        var bin = Path.Combine(PackStore.FilesDir(_root, "kcd2"), PackStore.BinFolder);
        File.WriteAllText(Path.Combine(bin, "agent.json"), "the player's settings");
        File.WriteAllText(Path.Combine(bin, "agent.log"), "a log");
        Directory.CreateDirectory(Path.Combine(bin, "sub"));
        File.WriteAllText(Path.Combine(bin, "sub", "more.log"), "another");
        File.WriteAllText(Path.Combine(bin, "Relay.exe"), "tampered, but listed: the new copy wins");

        var second = BuildZip(Files, version: "0.3.0");
        var r = await PackInstaller.DownloadAsync(InfoFor(second, version: "0.3.0"), AnyUrl, _root, Serving(second));
        Assert.True(r.Ok, r.Message);

        var nbin = Path.Combine(PackStore.FilesDir(_root, "kcd2"), PackStore.BinFolder);
        Assert.Equal("the player's settings", File.ReadAllText(Path.Combine(nbin, "agent.json")));
        Assert.Equal("a log", File.ReadAllText(Path.Combine(nbin, "agent.log")));
        Assert.Equal("another", File.ReadAllText(Path.Combine(nbin, "sub", "more.log")));
        Assert.Equal("stub Relay.exe", File.ReadAllText(Path.Combine(nbin, "Relay.exe")));
    }

    [Fact]
    public async Task AWrongChecksumIsRefusedAndStoresNothing()
    {
        var zip = BuildZip(Files);
        var info = InfoFor(zip) with { Sha256 = new string('0', 64) };
        var r = await PackInstaller.DownloadAsync(info, AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("checksum", r.Message);
        Assert.False(PackStore.IsDownloaded(_root, "kcd2"));
    }

    [Fact]
    public async Task AFailedReplacementKeepsThePreviousCopy()
    {
        var good = BuildZip(Files);
        await PackInstaller.DownloadAsync(InfoFor(good), AnyUrl, _root, Serving(good));
        var bad = BuildZip(Files);
        var r = await PackInstaller.DownloadAsync(InfoFor(bad) with { Sha256 = new string('1', 64) }, AnyUrl, _root, Serving(bad));
        Assert.False(r.Ok);
        Assert.True(PackStore.IsDownloaded(_root, "kcd2"));
    }

    [Fact]
    public async Task ASizeThatDiffersFromTheCatalogIsRefused()
    {
        var zip = BuildZip(Files);
        var r = await PackInstaller.DownloadAsync(InfoFor(zip) with { Size = zip.Length + 5 }, AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
    }

    [Fact]
    public async Task AServerErrorIsReportedPlainly()
    {
        var r = await PackInstaller.DownloadAsync(InfoFor(BuildZip(Files)), AnyUrl, _root, Serving([], HttpStatusCode.NotFound));
        Assert.False(r.Ok);
        Assert.Contains("404", r.Message);
    }

    [Fact]
    public async Task AFileThatEscapesThePackFolderIsRefused()
    {
        var zip = BuildZip(Files, extraUnlisted: new Dictionary<string, string> { ["../../evil.txt"] = "x" });
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.False(File.Exists(Path.Combine(_root, "evil.txt")));
        Assert.False(PackStore.IsDownloaded(_root, "kcd2"));
    }

    [Fact]
    public async Task AFileTheManifestDoesNotListIsRefused()
    {
        var zip = BuildZip(Files, extraUnlisted: new Dictionary<string, string> { ["mod/Data/extra.pak"] = "sneaked in" });
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("not listed", r.Message);
    }

    [Fact]
    public async Task AFileWhoseContentDiffersFromItsManifestLineIsRefused()
    {
        var zip = BuildZip(Files, manifestLine: (rel, text) => $"MOD|{rel}|{Encoding.UTF8.GetByteCount(text)}|{new string('2', 64)}");
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("checksum", r.Message);
    }

    [Fact]
    public async Task APackWithoutAManifestIsRefused()
    {
        var zip = BuildZip(Files, withManifest: false);
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
    }

    [Fact]
    public async Task APackWithoutAContractIsRefused()
    {
        var zip = BuildZip(Files, withPrograms: false);
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("contract", r.Message);
    }

    [Fact]
    public async Task AContractThatNamesAProgramThePackDoesNotCarryIsRefused()
    {
        var zip = BuildZip(Files, contract: FakeSteam.ContractJson.Replace("bin/Relay.exe", "bin/Missing.exe").Replace("\"version\": \"0.1.0\"", "\"version\": \"0.2.0\""));
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("Missing.exe", r.Message);
        Assert.False(PackStore.IsDownloaded(_root, "kcd2"));
    }

    [Fact]
    public async Task AContractThatDoesNotMatchItsCatalogEntryIsRefused()
    {
        var zip = BuildZip(Files, version: "0.9.0");
        var r = await PackInstaller.DownloadAsync(InfoFor(zip, version: "0.2.0"), AnyUrl, _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("does not match", r.Message);
    }

    [Fact]
    public async Task NotAZipIsRefused()
    {
        var junk = Encoding.UTF8.GetBytes("this is not a zip file at all");
        var r = await PackInstaller.DownloadAsync(InfoFor(junk), AnyUrl, _root, Serving(junk));
        Assert.False(r.Ok);
    }

    [Fact]
    public void RemoveDeletesTheDownloadedPack()
    {
        using var f = new FakeSteam("rm").WithPack();
        Assert.True(PackStore.IsDownloaded(f.PacksRoot, ModInstall.PackId));
        PackStore.Remove(f.PacksRoot, ModInstall.PackId);
        Assert.False(PackStore.IsDownloaded(f.PacksRoot, ModInstall.PackId));
    }
}

public class PackChecklistTests
{
    [Fact]
    public void WithoutADownloadedPackTheModStepAsksForTheDownload()
    {
        using var f = new FakeSteam("nopack").WithGame().WithModdingTools().Linked().WithInstall(staged: false);
        var snap = f.Snapshot();
        Assert.True(snap.Mod!.PackMissing);
        Assert.False(snap.Mod.Ok);
        var step = SetupChecklist.Evaluate(snap, SetupActivity.Idle).Single(s => s.Id == StepId.Mod);
        Assert.Equal(StepStatus.NeedsYou, step.Status);
        Assert.Equal(StepAction.DownloadPack, step.Action);
    }

    [Fact]
    public void WhileTheDownloadRunsTheStepShowsProgress()
    {
        using var f = new FakeSteam("dl").WithGame().WithModdingTools().Linked().WithInstall(staged: false);
        var step = SetupChecklist.Evaluate(f.Snapshot(), SetupActivity.Idle with { DownloadingPack = true, PackProgress = 0.42 }).Single(s => s.Id == StepId.Mod);
        Assert.Equal(StepStatus.Working, step.Status);
        Assert.Contains("42%", step.Detail);
    }

    [Fact]
    public void ADownloadedPackIsPlacedByTheLauncherThenTheStepIsDone()
    {
        using var f = new FakeSteam("placepack").WithGame().WithModdingTools().Linked().WithInstall(staged: true);
        var before = f.Snapshot();
        Assert.False(before.Mod!.PackMissing);
        Assert.Equal(StepAction.PlaceMod, SetupChecklist.Evaluate(before, SetupActivity.Idle).Single(s => s.Id == StepId.Mod).Action);
        f.WithModPlaced();
        Assert.Equal(StepStatus.Done, SetupChecklist.Evaluate(f.Snapshot(), SetupActivity.Idle).Single(s => s.Id == StepId.Mod).Status);
    }
}

public class ModRemovalTests
{
    [Fact]
    public void RemovingTheModTakesOutOnlyTheFilesThePackListedAndTheKeysPak()
    {
        using var f = new FakeSteam("unplace").WithModdingTools().WithInstall().WithModPlaced();
        var target = ModInstall.TargetDir(f.MtRoot, f.PacksRoot);
        File.WriteAllText(Path.Combine(target, "Data", "kdcmp_keys.pak"), "built from the player's own game files");
        File.WriteAllText(Path.Combine(target, "player-notes.txt"), "not ours");

        var r = ModInstall.Unplace(f.MtRoot, f.PacksRoot);

        Assert.True(r.Ok, r.Message);
        Assert.False(File.Exists(Path.Combine(target, "mod.manifest")));
        Assert.False(File.Exists(Path.Combine(target, "Data", "kdcmp.pak")));
        Assert.False(File.Exists(Path.Combine(target, "Data", "kdcmp_keys.pak")));
        Assert.True(File.Exists(Path.Combine(target, "player-notes.txt")));   // somebody else's file stays
    }

    [Fact]
    public void ThePrograms_of_the_pack_are_never_put_into_the_game_folder()
    {
        using var f = new FakeSteam("unplace-bin").WithModdingTools().WithInstall().WithModPlaced();
        var target = ModInstall.TargetDir(f.MtRoot, f.PacksRoot);
        Assert.False(File.Exists(Path.Combine(target, "Agent.exe")));
        Assert.False(File.Exists(Path.Combine(target, "Native.dll")));
        Assert.False(Directory.Exists(Path.Combine(target, "bin")));
    }

    [Fact]
    public void RemovingTheLastFileAlsoRemovesTheEmptyFolders()
    {
        using var f = new FakeSteam("unplace-empty").WithModdingTools().WithInstall().WithModPlaced();
        Assert.True(ModInstall.Unplace(f.MtRoot, f.PacksRoot).Ok);
        Assert.False(Directory.Exists(ModInstall.TargetDir(f.MtRoot, f.PacksRoot)));
    }

    [Fact]
    public void RemovingWhenNothingIsPlacedIsHarmless()
    {
        using var f = new FakeSteam("unplace-none").WithModdingTools();
        Assert.True(ModInstall.Unplace(f.MtRoot, f.PacksRoot).Ok);
    }

    [Fact]
    public void RemovingNeverFollowsALinkOutOfTheModFolder()
    {
        using var f = new FakeSteam("unplace-link").WithModdingTools().WithInstall().WithModPlaced();
        var outside = Path.Combine(f.Root, "outside");
        Directory.CreateDirectory(outside);
        File.WriteAllText(Path.Combine(outside, "keep.txt"), "do not touch");
        var link = Path.Combine(ModInstall.TargetDir(f.MtRoot, f.PacksRoot), "linked");
        try { Directory.CreateSymbolicLink(link, outside); }
        catch (Exception) { return; }   // creating a symlink needs a privilege some machines do not give a test run
        ModInstall.Unplace(f.MtRoot, f.PacksRoot);
        Assert.True(File.Exists(Path.Combine(outside, "keep.txt")));
    }
}

public class ScriptBuiltPackTests : IDisposable
{
    private readonly string _root = Path.Combine(AppContext.BaseDirectory, "scratch", "scriptpack-" + Guid.NewGuid().ToString("N")[..8]);
    public void Dispose() { try { Directory.Delete(_root, recursive: true); } catch { } }

    private sealed class FileHandler(string path) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken ct) =>
            Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK) { Content = new ByteArrayContent(File.ReadAllBytes(path)) });
    }

    /// <summary>
    /// A pack built by a mod repository's own script (its release folder: pack-index.json and the zip) goes through the launcher's real
    /// download code, when a path to that folder is given in CAIRN_MOD_RELEASE_DIR. Quietly passes when it is not.
    /// </summary>
    [Fact]
    public async Task ThePackAModRepositoryBuildsInstallsThroughTheLaunchersOwnCode()
    {
        var packs = Environment.GetEnvironmentVariable("CAIRN_MOD_RELEASE_DIR");
        if (string.IsNullOrWhiteSpace(packs) || !File.Exists(Path.Combine(packs, PackIndex.FileName))) return;

        var index = PackIndex.Parse(File.ReadAllText(Path.Combine(packs, PackIndex.FileName)));
        Assert.NotNull(index);
        var info = index!.ForGame(ModInstall.PackId);
        Assert.NotNull(info);

        var http = new HttpClient(new FileHandler(Path.Combine(packs, info!.File)));
        var r = await PackInstaller.DownloadAsync(info, new Uri("https://example.test/" + info.File), _root, http);
        Assert.True(r.Ok, r.Message);

        var installed = PackStore.Read(_root, ModInstall.PackId);
        Assert.NotNull(installed);
        Assert.NotNull(installed!.Contract);
        Assert.True(File.Exists(PackContract.Resolve(PackStore.FilesDir(_root, ModInstall.PackId), installed.Contract!.Agent)));
        Assert.Contains(installed.Files, f => f.Kind == "MOD" && f.Rel == "mod.manifest");
        Assert.NotNull(PackCatalog.PackUrl("Cairn-client/mod-kcd2", info));
    }
}

public class LegacyModFolderTests
{
    private const string OldManifest = "<kcd_mod>\n<info>\n<name>KCD2 Multiplayer</name>\n<modid>cairn</modid>\n</info>\n</kcd_mod>\n";

    private static string MakeLegacy(FakeSteam f, bool extraFile = false, string manifest = OldManifest)
    {
        var dir = ModInstall.LegacyDir(f.MtRoot, "cairn");
        Directory.CreateDirectory(Path.Combine(dir, "Data"));
        File.WriteAllText(Path.Combine(dir, "mod.manifest"), manifest);
        File.WriteAllText(Path.Combine(dir, "Data", "cairn.pak"), "old pak");
        File.WriteAllText(Path.Combine(dir, "Data", "cairn_keys.pak"), "old keys");
        if (extraFile) File.WriteAllText(Path.Combine(dir, "notes.txt"), "somebody else's");
        return dir;
    }

    [Fact]
    public void An_earlier_mods_folder_is_recognised_and_stops_the_mod_counting_as_in_place()
    {
        using var f = new FakeSteam("legacy").WithModdingTools().WithInstall().WithModPlaced();
        MakeLegacy(f);
        var r = ModInstall.Check(f.AppDir, f.MtRoot, ownsModFolder: true, packsRoot: f.PacksRoot);
        Assert.True(r.LegacyFolder);
        Assert.False(r.Ok);   // the current mod is there, but the game would load two
    }

    [Fact]
    public void Placing_the_mod_removes_the_earlier_folder_and_then_everything_is_fine()
    {
        using var f = new FakeSteam("legacy-place").WithModdingTools().WithInstall().WithModPlaced();
        var dir = MakeLegacy(f);
        Assert.True(ModInstall.Place(f.MtRoot, f.PacksRoot).Ok);
        Assert.False(Directory.Exists(dir));
        var r = ModInstall.Check(f.AppDir, f.MtRoot, ownsModFolder: true, packsRoot: f.PacksRoot);
        Assert.False(r.LegacyFolder);
        Assert.True(r.Ok);
    }

    [Fact]
    public void A_folder_with_anything_else_in_it_is_not_ours_and_stays()
    {
        using var f = new FakeSteam("legacy-foreign").WithModdingTools().WithInstall();
        var dir = MakeLegacy(f, extraFile: true);
        Assert.False(ModInstall.HasLegacyFolder(f.MtRoot, f.PacksRoot));
        Assert.True(ModInstall.RemoveLegacyFolder(f.MtRoot, f.PacksRoot));
        Assert.True(File.Exists(Path.Combine(dir, "notes.txt")));
    }

    [Fact]
    public void A_different_mod_that_happens_to_be_called_cairn_stays()
    {
        using var f = new FakeSteam("legacy-other").WithModdingTools().WithInstall();
        var dir = MakeLegacy(f, manifest: "<kcd_mod><info><name>Another Mod</name><modid>cairn</modid></info></kcd_mod>");
        Assert.False(ModInstall.HasLegacyFolder(f.MtRoot, f.PacksRoot));
        Assert.True(Directory.Exists(dir));
    }

    [Fact]
    public void A_folder_the_contract_does_not_name_is_never_touched()
    {
        using var f = new FakeSteam("legacy-unnamed").WithModdingTools().WithInstall();
        var other = Path.Combine(f.MtRoot, "Mods", "some-other-mod");
        Directory.CreateDirectory(Path.Combine(other, "Data"));
        File.WriteAllText(Path.Combine(other, "mod.manifest"), OldManifest.Replace("cairn", "some-other-mod"));
        Assert.False(ModInstall.HasLegacyFolder(f.MtRoot, f.PacksRoot));
        Assert.True(Directory.Exists(other));
    }

    [Fact]
    public void No_earlier_folder_is_nothing_to_do()
    {
        using var f = new FakeSteam("legacy-none").WithModdingTools();
        Assert.False(ModInstall.HasLegacyFolder(f.MtRoot, f.PacksRoot));
        Assert.True(ModInstall.RemoveLegacyFolder(f.MtRoot, f.PacksRoot));
    }
}

public class ModPlacedTests
{
    [Fact]
    public void A_downloaded_pack_that_was_never_placed_is_not_installed()
    {
        using var f = new FakeSteam("placed-no").WithModdingTools().WithPack();
        Assert.False(ModInstall.IsPlaced(f.MtRoot, f.PacksRoot));
    }

    [Fact]
    public void A_placed_mod_is_installed_even_without_an_install_manifest()
    {
        using var f = new FakeSteam("placed-yes").WithModdingTools().WithPack();
        Assert.True(ModInstall.Place(f.MtRoot, f.PacksRoot).Ok);
        Assert.True(ModInstall.IsPlaced(f.MtRoot, f.PacksRoot));   // no AppDir / manifest involved
    }

    [Fact]
    public void A_file_of_the_wrong_size_or_a_missing_one_is_not_installed()
    {
        using var f = new FakeSteam("placed-bad").WithModdingTools().WithPack().WithModPlaced();
        var pak = Path.Combine(ModInstall.TargetDir(f.MtRoot, f.PacksRoot), "Data", "kdcmp.pak");
        File.WriteAllText(pak, "short");
        Assert.False(ModInstall.IsPlaced(f.MtRoot, f.PacksRoot));
        File.Delete(pak);
        Assert.False(ModInstall.IsPlaced(f.MtRoot, f.PacksRoot));
    }

    [Fact]
    public void No_pack_at_all_is_not_installed()
    {
        using var f = new FakeSteam("placed-nopack").WithModdingTools();
        Assert.False(ModInstall.IsPlaced(f.MtRoot, f.PacksRoot));
    }

    [Fact]
    public void The_mod_folder_is_the_one_the_contract_names()
    {
        using var f = new FakeSteam("folder").WithModdingTools().WithPack();
        Assert.Equal(Path.Combine(f.MtRoot, "Mods", "kdcmp"), ModInstall.TargetDir(f.MtRoot, f.PacksRoot));
        Assert.Equal(@"Data\kdcmp_keys.pak", ModInstall.KeysPak(f.PacksRoot));
    }
}
