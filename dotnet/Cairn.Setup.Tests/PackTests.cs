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
        { "schema": 1, "client": "0.2.0", "packs": [
            { "id": "kcd2", "game": "kcd2", "version": "0.2.0", "file": "Cairn-Pack-kcd2-0.2.0.zip",
              "sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "size": 1234 } ] }
        """;

    [Fact]
    public void AGoodCatalogIsRead()
    {
        var index = PackIndex.Parse(Good);
        Assert.NotNull(index);
        var p = index!.ForGame("KCD2");
        Assert.Equal("Cairn-Pack-kcd2-0.2.0.zip", p!.File);
        Assert.Equal(new string('a', 64), p.Sha256);   // normalised to lower case
        Assert.Null(index.ForGame("fo4"));
    }

    [Theory]
    [InlineData("not json")]
    [InlineData("""{ "schema": 2, "client": "9", "packs": [] }""")]                                  // a schema this client does not know
    [InlineData("""{ "schema": 1, "client": "0.2.0" }""")]                                           // no packs
    public void ACatalogItDoesNotUnderstandIsRefused(string json) => Assert.Null(PackIndex.Parse(json));

    [Theory]
    [InlineData("../evil", "a.zip", 64)]                  // a path in the id
    [InlineData("kcd2", "..\\a.zip", 64)]                 // a path in the file name
    [InlineData("kcd2", "a.zip", 10)]                     // not a sha256
    public void AMalformedEntryRefusesTheWholeCatalog(string id, string file, int shaLen)
    {
        string json = "{\"schema\":1,\"client\":\"0.2.0\",\"packs\":[{\"id\":\"" + id.Replace("\\", "\\\\") + "\",\"game\":\"kcd2\",\"version\":\"1\"," +
                      "\"file\":\"" + file.Replace("\\", "\\\\") + "\",\"sha256\":\"" + new string('a', shaLen) + "\",\"size\":5}]}";
        Assert.Null(PackIndex.Parse(json));
    }
}

public class PackInstallerTests : IDisposable
{
    private readonly string _root = Path.Combine(AppContext.BaseDirectory, "scratch", "packs-" + Guid.NewGuid().ToString("N")[..8]);
    public void Dispose() { try { Directory.Delete(_root, recursive: true); } catch { } }

    private static string Hex(byte[] b) => Convert.ToHexString(SHA256.HashData(b)).ToLowerInvariant();

    /// <summary>A pack zip: manifest.txt plus the files, each listed with its real size and hash unless told otherwise.</summary>
    private static byte[] BuildZip(Dictionary<string, string> files, Func<string, string, string>? manifestLine = null, bool withManifest = true,
        Dictionary<string, string>? extraUnlisted = null)
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
            if (withManifest)
                Add("manifest.txt", string.Join("\n", files.Select(kv =>
                    manifestLine?.Invoke(kv.Key, kv.Value) ?? $"MOD|{kv.Key}|{Encoding.UTF8.GetByteCount(kv.Value)}|{Hex(Encoding.UTF8.GetBytes(kv.Value))}")) + "\n");
            foreach (var kv in files) Add(kv.Key.Replace('\\', '/'), kv.Value);
            if (extraUnlisted is not null) foreach (var kv in extraUnlisted) Add(kv.Key, kv.Value);
        }
        return ms.ToArray();
    }

    private static PackInfo InfoFor(byte[] zip, string id = "kcd2") =>
        new(id, id, "0.2.0", "Cairn-Pack-" + id + "-0.2.0.zip", Hex(zip), zip.Length);

    private sealed class FakeHandler(Func<HttpResponseMessage> respond) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken ct) => Task.FromResult(respond());
    }

    private static HttpClient Serving(byte[] body, HttpStatusCode code = HttpStatusCode.OK) =>
        new(new FakeHandler(() => new HttpResponseMessage(code) { Content = new ByteArrayContent(body) }));

    private static readonly Dictionary<string, string> Files = new() { ["mod.manifest"] = "manifest text", ["Data\\cairn.pak"] = "pak bytes" };

    [Fact]
    public async Task AGoodPackIsDownloadedVerifiedAndStored()
    {
        var zip = BuildZip(Files);
        var progress = new List<double>();
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), new Uri("https://example.test/p.zip"), _root, Serving(zip), new Progress<double>(progress.Add));
        Assert.True(r.Ok, r.Message);

        var pack = PackStore.Read(_root, "kcd2");
        Assert.NotNull(pack);
        Assert.Equal("0.2.0", pack!.Version);
        Assert.Equal(2, pack.Files.Count);
        Assert.Equal("pak bytes", File.ReadAllText(Path.Combine(PackStore.FilesDir(_root, "kcd2"), "Data", "cairn.pak")));
        Assert.False(File.Exists(Path.Combine(_root, "kcd2.download")));   // nothing half-done is left behind
        Assert.False(Directory.Exists(Path.Combine(_root, "kcd2.new")));
    }

    [Fact]
    public async Task ADownloadingAgainReplacesThePreviousCopy()
    {
        var first = BuildZip(Files);
        await PackInstaller.DownloadAsync(InfoFor(first), new Uri("https://example.test/p.zip"), _root, Serving(first));
        var second = BuildZip(new Dictionary<string, string> { ["mod.manifest"] = "new manifest" });
        var r = await PackInstaller.DownloadAsync(InfoFor(second), new Uri("https://example.test/p.zip"), _root, Serving(second));
        Assert.True(r.Ok, r.Message);
        Assert.Single(PackStore.Read(_root, "kcd2")!.Files);
        Assert.False(File.Exists(Path.Combine(PackStore.FilesDir(_root, "kcd2"), "Data", "cairn.pak")));   // the old file is gone
    }

    [Fact]
    public async Task AWrongChecksumIsRefusedAndStoresNothing()
    {
        var zip = BuildZip(Files);
        var info = InfoFor(zip) with { Sha256 = new string('0', 64) };
        var r = await PackInstaller.DownloadAsync(info, new Uri("https://example.test/p.zip"), _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("checksum", r.Message);
        Assert.False(PackStore.IsDownloaded(_root, "kcd2"));
    }

    [Fact]
    public async Task AFailedReplacementKeepsThePreviousCopy()
    {
        var good = BuildZip(Files);
        await PackInstaller.DownloadAsync(InfoFor(good), new Uri("https://example.test/p.zip"), _root, Serving(good));
        var bad = BuildZip(Files);
        var r = await PackInstaller.DownloadAsync(InfoFor(bad) with { Sha256 = new string('1', 64) }, new Uri("https://example.test/p.zip"), _root, Serving(bad));
        Assert.False(r.Ok);
        Assert.True(PackStore.IsDownloaded(_root, "kcd2"));
    }

    [Fact]
    public async Task ASizeThatDiffersFromTheCatalogIsRefused()
    {
        var zip = BuildZip(Files);
        var r = await PackInstaller.DownloadAsync(InfoFor(zip) with { Size = zip.Length + 5 }, new Uri("https://example.test/p.zip"), _root, Serving(zip));
        Assert.False(r.Ok);
    }

    [Fact]
    public async Task AServerErrorIsReportedPlainly()
    {
        var r = await PackInstaller.DownloadAsync(InfoFor(BuildZip(Files)), new Uri("https://example.test/p.zip"), _root, Serving([], HttpStatusCode.NotFound));
        Assert.False(r.Ok);
        Assert.Contains("404", r.Message);
    }

    [Fact]
    public async Task AFileThatEscapesThePackFolderIsRefused()
    {
        var zip = BuildZip(Files, extraUnlisted: new Dictionary<string, string> { ["../../evil.txt"] = "x" });
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), new Uri("https://example.test/p.zip"), _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.False(File.Exists(Path.Combine(_root, "evil.txt")));
        Assert.False(PackStore.IsDownloaded(_root, "kcd2"));
    }

    [Fact]
    public async Task AFileTheManifestDoesNotListIsRefused()
    {
        var zip = BuildZip(Files, extraUnlisted: new Dictionary<string, string> { ["Data/extra.pak"] = "sneaked in" });
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), new Uri("https://example.test/p.zip"), _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("not listed", r.Message);
    }

    [Fact]
    public async Task AFileWhoseContentDiffersFromItsManifestLineIsRefused()
    {
        var zip = BuildZip(Files, manifestLine: (rel, text) => $"MOD|{rel}|{Encoding.UTF8.GetByteCount(text)}|{new string('2', 64)}");
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), new Uri("https://example.test/p.zip"), _root, Serving(zip));
        Assert.False(r.Ok);
        Assert.Contains("checksum", r.Message);
    }

    [Fact]
    public async Task APackWithoutAManifestIsRefused()
    {
        var zip = BuildZip(Files, withManifest: false);
        var r = await PackInstaller.DownloadAsync(InfoFor(zip), new Uri("https://example.test/p.zip"), _root, Serving(zip));
        Assert.False(r.Ok);
    }

    [Fact]
    public async Task NotAZipIsRefused()
    {
        var junk = Encoding.UTF8.GetBytes("this is not a zip file at all");
        var r = await PackInstaller.DownloadAsync(InfoFor(junk), new Uri("https://example.test/p.zip"), _root, Serving(junk));
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
        var target = ModInstall.TargetDir(f.MtRoot);
        File.WriteAllText(Path.Combine(target, "Data", "cairn_keys.pak"), "built from the player's own game files");
        File.WriteAllText(Path.Combine(target, "player-notes.txt"), "not ours");

        var r = ModInstall.Unplace(f.MtRoot, f.PacksRoot);

        Assert.True(r.Ok, r.Message);
        Assert.False(File.Exists(Path.Combine(target, "mod.manifest")));
        Assert.False(File.Exists(Path.Combine(target, "Data", "cairn.pak")));
        Assert.False(File.Exists(Path.Combine(target, "Data", "cairn_keys.pak")));
        Assert.True(File.Exists(Path.Combine(target, "player-notes.txt")));   // somebody else's file stays
    }

    [Fact]
    public void RemovingTheLastFileAlsoRemovesTheEmptyFolders()
    {
        using var f = new FakeSteam("unplace-empty").WithModdingTools().WithInstall().WithModPlaced();
        Assert.True(ModInstall.Unplace(f.MtRoot, f.PacksRoot).Ok);
        Assert.False(Directory.Exists(ModInstall.TargetDir(f.MtRoot)));
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
        var link = Path.Combine(ModInstall.TargetDir(f.MtRoot), "linked");
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
    /// What tools\Build-Packs.ps1 really writes (release\packs, built by CI and by Build-Installer.ps1 before the tests run)
    /// goes through the launcher's real download code: the script's zip layout and catalog must be what the reader accepts.
    /// Quietly passes when the packs have not been built on this machine.
    /// </summary>
    [Fact]
    public async Task ThePackTheBuildScriptWritesInstallsThroughTheLaunchersOwnCode()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null && !File.Exists(Path.Combine(dir.FullName, "VERSION"))) dir = dir.Parent;
        var packs = dir is null ? null : Path.Combine(dir.FullName, "release", "packs");
        if (packs is null || !File.Exists(Path.Combine(packs, PackIndex.FileName))) return;

        var index = PackIndex.Parse(File.ReadAllText(Path.Combine(packs, PackIndex.FileName)));
        Assert.NotNull(index);
        var info = index!.ForGame(ModInstall.PackId);
        Assert.NotNull(info);

        var http = new HttpClient(new FileHandler(Path.Combine(packs, info!.File)));
        var r = await PackInstaller.DownloadAsync(info, new Uri("https://example.test/" + info.File), _root, http);
        Assert.True(r.Ok, r.Message);

        var installed = PackStore.Read(_root, ModInstall.PackId);
        Assert.NotNull(installed);
        Assert.Contains(installed!.Files, f => f.Rel.Equals(@"Data\cairn.pak", StringComparison.OrdinalIgnoreCase));
        Assert.Contains(installed.Files, f => f.Rel == "mod.manifest");
        Assert.NotNull(PackCatalog.PackUrl(index.Client, info));
    }
}

public class LegacyModFolderTests
{
    private const string OldManifest = "<kcd_mod>\n<info>\n<name>KCD2 Multiplayer</name>\n<modid>kdcmp</modid>\n</info>\n</kcd_mod>\n";

    private static string MakeLegacy(FakeSteam f, bool extraFile = false, string manifest = OldManifest)
    {
        var dir = ModInstall.LegacyDir(f.MtRoot);
        Directory.CreateDirectory(Path.Combine(dir, "Data"));
        File.WriteAllText(Path.Combine(dir, "mod.manifest"), manifest);
        File.WriteAllText(Path.Combine(dir, "Data", "kdcmp.pak"), "old pak");
        File.WriteAllText(Path.Combine(dir, "Data", "kdcmp_keys.pak"), "old keys");
        if (extraFile) File.WriteAllText(Path.Combine(dir, "notes.txt"), "somebody else's");
        return dir;
    }

    [Fact]
    public void The_earlier_releases_folder_is_recognised_and_stops_the_mod_counting_as_in_place()
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
        Assert.False(ModInstall.HasLegacyFolder(f.MtRoot));
        Assert.True(ModInstall.RemoveLegacyFolder(f.MtRoot));
        Assert.True(File.Exists(Path.Combine(dir, "notes.txt")));
    }

    [Fact]
    public void A_different_mod_that_happens_to_be_called_kdcmp_stays()
    {
        using var f = new FakeSteam("legacy-other").WithModdingTools().WithInstall();
        var dir = MakeLegacy(f, manifest: "<kcd_mod><info><name>Another Mod</name><modid>kdcmp</modid></info></kcd_mod>");
        Assert.False(ModInstall.HasLegacyFolder(f.MtRoot));
        Assert.True(Directory.Exists(dir));
    }

    [Fact]
    public void No_earlier_folder_is_nothing_to_do()
    {
        using var f = new FakeSteam("legacy-none").WithModdingTools();
        Assert.False(ModInstall.HasLegacyFolder(f.MtRoot));
        Assert.True(ModInstall.RemoveLegacyFolder(f.MtRoot));
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
        var pak = Path.Combine(ModInstall.TargetDir(f.MtRoot), "Data", "cairn.pak");
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
}
