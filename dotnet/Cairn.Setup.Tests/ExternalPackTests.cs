// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.IO.Compression;
using System.Net;
using System.Security.Cryptography;
using System.Text;

namespace Cairn.Setup.Tests;

public class ExternalContractTests
{
    private static string Json(string installDir = "mods/quant.ew", string extra = "", string kind = "external") =>
        "{ \"schema\": 2, \"id\": \"noita\", \"version\": \"1.0.0\", \"kind\": \"" + kind + "\", \"installDir\": \"" + installDir + "\"" + extra + " }";

    [Fact]
    public void AnExternalContractNeedsOnlyAnIdAVersionAndAFolderInTheGame()
    {
        var c = PackContract.Parse(Json());
        Assert.NotNull(c);
        Assert.Equal(PackKinds.External, c!.Kind);
        Assert.Equal("mods/quant.ew", c.InstallDir);
        Assert.Equal("", c.Companion);
    }

    [Fact]
    public void TheGamesOwnFolderCanBeTheInstallFolder() => Assert.Equal(".", PackContract.Parse(Json("."))!.InstallDir);

    [Fact]
    public void ACompanionProgramInsideThePackIsAccepted() =>
        Assert.Equal("bin/proxy.exe", PackContract.Parse(Json(extra: ", \"companion\": \"bin/proxy.exe\""))!.Companion);

    [Theory]
    [InlineData("../evil")]               // leaves the game's folder
    [InlineData("mods/../../evil")]
    [InlineData("/mods/x")]               // rooted
    [InlineData("C:/mods/x")]             // a drive
    [InlineData("mods\\\\x")]             // backslashes are not how a contract writes a path
    [InlineData("mods/")]                 // a trailing slash
    [InlineData("")]                      // an external mod must say where it goes
    [InlineData("a/b/c/d/e")]             // deeper than a mod folder needs
    [InlineData("mods/.hidden")]
    public void AFolderThatCouldLeaveTheGameIsRefused(string dir) => Assert.Null(PackContract.Parse(Json(dir)));

    [Theory]
    [InlineData(", \"agent\": \"bin/Agent.exe\"")]                  // an external mod does not use Cairn's session programs
    [InlineData(", \"installTo\": \"start-save\"")]
    [InlineData(", \"companion\": \"../evil.exe\"")]
    [InlineData(", \"companion\": \"C:/evil.exe\"")]
    public void AnExternalContractThatMixesInOtherThingsIsRefused(string extra) => Assert.Null(PackContract.Parse(Json(extra: extra)));

    [Theory]
    [InlineData("mod")]
    [InlineData("content")]
    public void OnlyAnExternalModHasAnInstallFolder(string kind)
    {
        Assert.Null(PackContract.Parse(Json(kind: kind)));
    }

    [Fact]
    public void AnExternalModIsAGameMod()
    {
        Assert.True(PackKinds.IsGameMod(PackKinds.External));
        Assert.True(PackKinds.IsGameMod(PackKinds.Mod));
        Assert.False(PackKinds.IsGameMod(PackKinds.Content));
    }

    [Fact]
    public void TheCatalogOffersAnExternalModAsTheGamesMod()
    {
        var index = PackIndex.Parse("{ \"schema\": 2, \"id\": \"noita\", \"game\": \"noita\", \"version\": \"1.0.0\", \"kind\": \"external\", \"file\": \"p.zip\", \"sha256\": \""
                                    + new string('a', 64) + "\", \"size\": 5 }");
        Assert.Equal("noita", index!.ForGame("noita")!.Id);
    }
}

public class ExternalInstallTests : IDisposable
{
    private readonly string _dir = Path.Combine(AppContext.BaseDirectory, "scratch", "ext-" + Guid.NewGuid().ToString("N")[..8]);
    private string Packs => Path.Combine(_dir, "packs");
    private string Game => Path.Combine(_dir, "game");

    public ExternalInstallTests() { Directory.CreateDirectory(Packs); Directory.CreateDirectory(Game); }
    public void Dispose() { try { Directory.Delete(_dir, recursive: true); } catch { } }

    private static string Hex(string t) => Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(t))).ToLowerInvariant();

    private static byte[] Zip(Dictionary<string, string> modFiles, string version = "1.0.0", string installDir = "mods/quant.ew", string? companion = null, bool carryCompanion = true, string? installRoot = null)
    {
        using var ms = new MemoryStream();
        using (var zip = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
        {
            void Add(string name, string text) { using var w = new StreamWriter(zip.CreateEntry(name).Open(), new UTF8Encoding(false)); w.Write(text); }
            var lines = modFiles.Select(kv => $"MOD|{kv.Key}|{Encoding.UTF8.GetByteCount(kv.Value)}|{Hex(kv.Value)}").ToList();
            if (companion is not null && carryCompanion) lines.Add($"BIN|{Path.GetFileName(companion)}|7|{Hex("proxy!!")}");
            Add("manifest.txt", string.Join("\n", lines) + "\n");
            Add("pack.json", "{ \"schema\": 2, \"id\": \"noita\", \"version\": \"" + version + "\", \"kind\": \"external\", \"installDir\": \"" + installDir + "\""
                             + (installRoot is null ? "" : ", \"installRoot\": \"" + installRoot + "\"")
                             + (companion is null ? "" : ", \"companion\": \"" + companion + "\"") + " }");
            foreach (var kv in modFiles) Add("mod/" + kv.Key, kv.Value);
            if (companion is not null && carryCompanion) Add("bin/" + Path.GetFileName(companion), "proxy!!");
        }
        return ms.ToArray();
    }

    private sealed class Serve(byte[] body) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken ct) =>
            Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK) { Content = new ByteArrayContent(body) });
    }

    private async Task<PackResult> Download(byte[] zip, string version = "1.0.0") =>
        await PackInstaller.DownloadAsync(new PackInfo("noita", "noita", version, "p.zip", Convert.ToHexString(SHA256.HashData(zip)).ToLowerInvariant(), zip.Length, Kind: PackKinds.External),
            new Uri("https://example.test/p.zip"), Packs, new HttpClient(new Serve(zip)));

    private static readonly Dictionary<string, string> Files = new() { ["init.lua"] = "print('a')", ["data/x.xml"] = "<x/>" };

    [Fact]
    public async Task ADownloadedExternalModIsPlacedInTheGamesFolderAndNowhereElse()
    {
        Assert.True((await Download(Zip(Files))).Ok);
        Assert.False(ExternalInstall.IsPlaced(Game, Packs, "noita"));

        var r = ExternalInstall.Place(Game, Packs, "noita");
        Assert.True(r.Ok, r.Message);
        Assert.True(ExternalInstall.IsPlaced(Game, Packs, "noita"));
        Assert.Equal("print('a')", File.ReadAllText(Path.Combine(Game, "mods", "quant.ew", "init.lua")));
        Assert.Equal("<x/>", File.ReadAllText(Path.Combine(Game, "mods", "quant.ew", "data", "x.xml")));
        Assert.Single(Directory.GetDirectories(Game));   // only mods\
    }

    [Fact]
    public async Task AnUpdateReplacesTheFilesAndDropsTheOnesTheNewVersionNoLongerHas()
    {
        await Download(Zip(Files));
        ExternalInstall.Place(Game, Packs, "noita");
        Assert.True((await Download(Zip(new() { ["init.lua"] = "print('b')" }, version: "1.1.0"), "1.1.0")).Ok);
        Assert.False(ExternalInstall.IsPlaced(Game, Packs, "noita"));   // the placed copy is the old version

        Assert.True(ExternalInstall.Place(Game, Packs, "noita").Ok);
        Assert.True(ExternalInstall.IsPlaced(Game, Packs, "noita"));
        Assert.Equal("print('b')", File.ReadAllText(Path.Combine(Game, "mods", "quant.ew", "init.lua")));
        Assert.False(File.Exists(Path.Combine(Game, "mods", "quant.ew", "data", "x.xml")));
    }

    [Fact]
    public async Task AFolderThePlayerMadeIsNeverOverwrittenOrRemoved()
    {
        var theirs = Path.Combine(Game, "mods", "quant.ew");
        Directory.CreateDirectory(theirs);
        File.WriteAllText(Path.Combine(theirs, "init.lua"), "the player's own");
        await Download(Zip(Files));

        var r = ExternalInstall.Place(Game, Packs, "noita");
        Assert.False(r.Ok);
        Assert.Contains("already in the game", r.Message);
        Assert.Equal("the player's own", File.ReadAllText(Path.Combine(theirs, "init.lua")));

        Assert.True(ExternalInstall.Unplace(Game, Packs, "noita").Ok);
        Assert.Equal("the player's own", File.ReadAllText(Path.Combine(theirs, "init.lua")));
    }

    [Fact]
    public async Task RemovingTakesOutOnlyWhatCairnPlacedAndTheFoldersItLeavesEmpty()
    {
        await Download(Zip(Files));
        ExternalInstall.Place(Game, Packs, "noita");
        var extra = Path.Combine(Game, "mods", "quant.ew", "player-settings.txt");
        File.WriteAllText(extra, "kept");

        Assert.True(ExternalInstall.Unplace(Game, Packs, "noita").Ok);
        Assert.False(File.Exists(Path.Combine(Game, "mods", "quant.ew", "init.lua")));
        Assert.False(File.Exists(Path.Combine(Game, "mods", "quant.ew", ExternalInstall.MarkerName)));
        Assert.Equal("kept", File.ReadAllText(extra));   // something the mod wrote itself stays

        File.Delete(extra);
        ExternalInstall.Place(Game, Packs, "noita");
        Assert.True(ExternalInstall.Unplace(Game, Packs, "noita").Ok);
        Assert.False(Directory.Exists(Path.Combine(Game, "mods", "quant.ew")));
    }

    [Fact]
    public async Task APackThatIsNotAnExternalModIsNotPlacedByThis()
    {
        Assert.False(ExternalInstall.Place(Game, Packs, "nothing").Ok);
        Assert.True(ExternalInstall.Unplace(Game, Packs, "nothing").Ok);
        Assert.False(ExternalInstall.IsPlaced(Game, Packs, "nothing"));
        await Task.CompletedTask;
    }

    [Fact]
    public async Task ACompanionProgramMustBeInThePackAndIsFoundInThePackStore()
    {
        Assert.False((await Download(Zip(Files, companion: "bin/proxy.exe", carryCompanion: false))).Ok);   // named but not carried

        Assert.True((await Download(Zip(Files, companion: "bin/proxy.exe"))).Ok);
        var path = ExternalInstall.CompanionPath(Packs, "noita");
        Assert.NotNull(path);
        Assert.StartsWith(Path.GetFullPath(PackStore.FilesDir(Packs, "noita")), path);
        Assert.Equal("proxy!!", File.ReadAllText(path!));
    }

    [Fact]
    public void NoCompanionMeansNoPath()
    {
        Assert.Null(ExternalInstall.CompanionPath(Packs, "noita"));
    }

    // ---- a mod that installs into the game's own folder (a loader such as BepInEx) --------------------------------------------

    private static readonly Dictionary<string, string> LoaderFiles = new() { ["winhttp.dll"] = "loader", ["BepInEx/plugins/mp/mp.dll"] = "plugin" };

    [Fact]
    public async Task AModForTheGamesOwnFolderIsPlacedBesideTheGamesFiles()
    {
        File.WriteAllText(Path.Combine(Game, "game.exe"), "the game");
        await Download(Zip(LoaderFiles, installDir: "."));
        var r = ExternalInstall.Place(Game, Packs, "noita");
        Assert.True(r.Ok, r.Message);
        Assert.True(ExternalInstall.IsPlaced(Game, Packs, "noita"));
        Assert.Equal("loader", File.ReadAllText(Path.Combine(Game, "winhttp.dll")));
        Assert.Equal("plugin", File.ReadAllText(Path.Combine(Game, "BepInEx", "plugins", "mp", "mp.dll")));
        Assert.Equal("the game", File.ReadAllText(Path.Combine(Game, "game.exe")));
    }

    // ---- a mod that installs into the player's local application data (a game that looks for its mods there) ----------------

    private const string AppDataDir = "Colossal Order/Cities_Skylines/Addons/Mods/CSM";

    [Fact]
    public void AContractCanNameTheLocalApplicationDataAsTheRootOfItsFolder()
    {
        string Json(string root, string dir) => "{ \"schema\": 2, \"id\": \"cs\", \"version\": \"1.0.0\", \"kind\": \"external\", \"installRoot\": \"" + root + "\", \"installDir\": \"" + dir + "\" }";
        var c = PackContract.Parse(Json("local-appdata", AppDataDir));
        Assert.NotNull(c);
        Assert.Equal("local-appdata", c!.InstallRoot);
        Assert.Null(PackContract.Parse(Json("local-appdata", ".")));              // the root itself is never a mod folder
        Assert.Null(PackContract.Parse(Json("local-appdata", "../x")));
        Assert.Null(PackContract.Parse(Json("system32", "x")));                     // only the roots the launcher knows
        Assert.Null(PackContract.Parse(Json("local-appdata", "a/b/c/d/e/f/g")));    // deeper than a mod folder needs
        Assert.Null(PackContract.Parse("{ \"schema\": 2, \"id\": \"cs\", \"version\": \"1.0.0\", \"kind\": \"mod\", \"installRoot\": \"local-appdata\" }"));   // only an external mod has an install root
    }

    [Fact]
    public async Task AModForTheLocalApplicationDataIsPlacedThereAndTakenOutAgain()
    {
        var appData = Path.Combine(_dir, "appdata");
        await Download(Zip(new Dictionary<string, string> { ["CSM.dll"] = "the mod" }, installDir: AppDataDir, installRoot: "local-appdata"));
        var r = ExternalInstall.Place(Game, Packs, "noita", appData);
        Assert.True(r.Ok, r.Message);
        Assert.Equal("the mod", File.ReadAllText(Path.Combine(appData, "Colossal Order", "Cities_Skylines", "Addons", "Mods", "CSM", "CSM.dll")));
        Assert.True(ExternalInstall.IsPlaced(Game, Packs, "noita", appData));
        Assert.False(File.Exists(Path.Combine(Game, "CSM.dll")));   // not in the game's folder
        Assert.True(ExternalInstall.Unplace(Game, Packs, "noita", appData).Ok);
        Assert.False(File.Exists(Path.Combine(appData, "Colossal Order", "Cities_Skylines", "Addons", "Mods", "CSM", "CSM.dll")));
    }

    [Fact]
    public async Task AFolderOfTheSameNameInTheLocalApplicationDataThatCairnDidNotMakeIsLeftAlone()
    {
        var appData = Path.Combine(_dir, "appdata");
        var theirs = Path.Combine(appData, "Colossal Order", "Cities_Skylines", "Addons", "Mods", "CSM");
        Directory.CreateDirectory(theirs);
        File.WriteAllText(Path.Combine(theirs, "CSM.dll"), "the player's own copy");
        await Download(Zip(new Dictionary<string, string> { ["CSM.dll"] = "the mod" }, installDir: AppDataDir, installRoot: "local-appdata"));
        Assert.False(ExternalInstall.Place(Game, Packs, "noita", appData).Ok);
        Assert.Equal("the player's own copy", File.ReadAllText(Path.Combine(theirs, "CSM.dll")));
    }

    [Fact]
    public async Task AGameWithoutAFolderStillPlacesNothingWhenTheModNeedsTheLocalApplicationData()
    {
        await Download(Zip(new Dictionary<string, string> { ["CSM.dll"] = "x" }, installDir: AppDataDir, installRoot: "local-appdata"));
        Assert.False(ExternalInstall.Place(Path.Combine(_dir, "no-such-game"), Packs, "noita", Path.Combine(_dir, "appdata")).Ok);
    }
    [Fact]
    public async Task AFileOfTheGameWithTheSameNameAndOtherBytesIsNeverOverwritten()
    {
        File.WriteAllText(Path.Combine(Game, "winhttp.dll"), "the player's own");
        await Download(Zip(LoaderFiles, installDir: "."));
        var r = ExternalInstall.Place(Game, Packs, "noita");
        Assert.False(r.Ok);
        Assert.Contains("winhttp.dll", r.Message);
        Assert.Equal("the player's own", File.ReadAllText(Path.Combine(Game, "winhttp.dll")));
        Assert.False(File.Exists(Path.Combine(Game, "BepInEx", "plugins", "mp", "mp.dll")));   // nothing was placed
    }

    [Fact]
    public async Task AFileThatIsAlreadyExactlyOursIsAdopted()
    {
        File.WriteAllText(Path.Combine(Game, "winhttp.dll"), "loader");
        await Download(Zip(LoaderFiles, installDir: "."));
        Assert.True(ExternalInstall.Place(Game, Packs, "noita").Ok);
    }

    [Fact]
    public async Task RemovingFromTheGamesFolderTakesOutOnlyOurFilesAndNeverTheGameFolder()
    {
        File.WriteAllText(Path.Combine(Game, "game.exe"), "the game");
        Directory.CreateDirectory(Path.Combine(Game, "BepInEx", "config"));
        File.WriteAllText(Path.Combine(Game, "BepInEx", "config", "theirs.cfg"), "kept");
        await Download(Zip(LoaderFiles, installDir: "."));
        ExternalInstall.Place(Game, Packs, "noita");

        Assert.True(ExternalInstall.Unplace(Game, Packs, "noita").Ok);
        Assert.True(Directory.Exists(Game));
        Assert.Equal("the game", File.ReadAllText(Path.Combine(Game, "game.exe")));
        Assert.False(File.Exists(Path.Combine(Game, "winhttp.dll")));
        Assert.False(File.Exists(Path.Combine(Game, ExternalInstall.MarkerName)));
        Assert.False(Directory.Exists(Path.Combine(Game, "BepInEx", "plugins")));   // the folders we emptied go
        Assert.Equal("kept", File.ReadAllText(Path.Combine(Game, "BepInEx", "config", "theirs.cfg")));   // the ones with something else stay
    }

    [Fact]
    public async Task AnUpdateInTheGamesFolderDropsOnlyWhatTheOldVersionPlaced()
    {
        File.WriteAllText(Path.Combine(Game, "game.exe"), "the game");
        await Download(Zip(LoaderFiles, installDir: "."));
        ExternalInstall.Place(Game, Packs, "noita");
        await Download(Zip(new() { ["winhttp.dll"] = "loader2" }, version: "1.1.0", installDir: "."), "1.1.0");
        Assert.True(ExternalInstall.Place(Game, Packs, "noita").Ok);
        Assert.Equal("loader2", File.ReadAllText(Path.Combine(Game, "winhttp.dll")));
        Assert.False(File.Exists(Path.Combine(Game, "BepInEx", "plugins", "mp", "mp.dll")));
        Assert.True(File.Exists(Path.Combine(Game, "game.exe")));
    }
}


public class ModCompanionTests
{
    [Fact]
    public void AGamesModMayCarryItsBridgeAsACompanion()
    {
        var c = PackContract.Parse(FakeSteam.ContractJson.Replace("\"lua\": {", "\"companion\": \"bin/Bridge.exe\", \"lua\": {"));
        Assert.NotNull(c);
        Assert.Equal("bin/Bridge.exe", c!.Companion);
        Assert.Equal("", PackContract.Parse(FakeSteam.ContractJson)!.Companion);   // a mod without one is as before
    }

    [Theory]
    [InlineData("../Bridge.exe")]
    [InlineData("C:/Bridge.exe")]
    public void ACompanionThatLeavesThePackIsRefused(string path) =>
        Assert.Null(PackContract.Parse(FakeSteam.ContractJson.Replace("\"lua\": {", "\"companion\": \"" + path + "\", \"lua\": {")));

    [Fact]
    public void ContentHasNoCompanion() =>
        Assert.Null(PackContract.Parse("{ \"schema\": 2, \"id\": \"saves\", \"version\": \"1.0.0\", \"kind\": \"content\", \"companion\": \"bin/x.exe\" }"));
}

public class CompanionStartsGameTests
{
    private static string Json(string extra) =>
        "{ \"schema\": 2, \"id\": \"noita\", \"version\": \"1.0.0\", \"kind\": \"external\", \"installDir\": \"mods/quant.ew\"" + extra + " }";

    [Fact]
    public void AnExternalModCanSayItsProgramStartsTheGame()
    {
        var c = PackContract.Parse(Json(", \"companion\": \"bin/noita_proxy.exe\", \"companionStartsGame\": true"));
        Assert.True(c!.CompanionStartsGame);
        Assert.False(PackContract.Parse(Json(", \"companion\": \"bin/noita_proxy.exe\""))!.CompanionStartsGame);
    }

    [Fact]
    public void ItNeedsAProgramToStartIt() => Assert.Null(PackContract.Parse(Json(", \"companionStartsGame\": true")));

    [Fact]
    public void OnlyAnExternalModCanSayIt() =>
        Assert.Null(PackContract.Parse(FakeSteam.ContractJson.Replace("\"lua\": {", "\"companion\": \"bin/B.exe\", \"companionStartsGame\": true, \"lua\": {")));
}
