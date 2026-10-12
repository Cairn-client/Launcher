// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
using System.Security.Cryptography;

namespace Cairn.Setup.Tests;

/// <summary>
/// A fake Steam install on a scratch folder: real folders, real appmanifests,
/// tiny fake paks laid out the way the real game's are (Data, Localization,
/// Data\Levels\&lt;level&gt;). Everything lives under the test's own output
/// folder in the repo; nothing outside it is touched.
/// </summary>
public sealed class FakeSteam : IDisposable
{
    public string Root { get; }
    public string SteamRoot => Path.Combine(Root, "Steam");
    public string Library => Path.Combine(Root, "Lib");
    public string GameRoot => Path.Combine(Library, "steamapps", "common", "KingdomComeDeliverance2");
    public string MtRoot => Path.Combine(Library, "steamapps", "common", "KCD2Mod");
    public string MtExe => Path.Combine(MtRoot, "Bin", "Win64ReleaseSteamLTO_DLL", "KingdomCome.exe");
    public string AppDir => Path.Combine(Root, "App");
    public string PacksRoot => Path.Combine(Root, "Packs");

    public static readonly string[] GamePaks =
    [
        @"Data\Tables.pak", @"Data\Scripts.pak", @"Data\Animations.pak",
        @"Localization\English_xml.pak", @"Localization\english-part0.pak",
        @"Data\Levels\klaster\level.pak", @"Data\Levels\trosecko\terrain.pak", @"Data\Levels\trosecko\svo-part0.pak",
    ];

    public FakeSteam(string name)
    {
        Root = Path.Combine(AppContext.BaseDirectory, "scratch", name + "-" + Guid.NewGuid().ToString("N")[..8]);
        Directory.CreateDirectory(Path.Combine(SteamRoot, "steamapps"));
        Directory.CreateDirectory(Path.Combine(Library, "steamapps"));
        File.WriteAllText(Path.Combine(SteamRoot, "steamapps", "libraryfolders.vdf"),
            "\"libraryfolders\"\n{\n" +
            $"\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{Esc(SteamRoot)}\"\n\t}}\n" +
            $"\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{Esc(Library)}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"1771300\"\t\t\"1\"\n\t\t}}\n\t}}\n}}\n");
    }

    private static string Esc(string p) => p.Replace("\\", "\\\\");

    public FakeSteam WithGame(long stateFlags = 4)
    {
        WriteManifest(SteamApp.Game, "Kingdom Come: Deliverance II", "KingdomComeDeliverance2", stateFlags);
        var t = new DateTime(2026, 7, 27, 11, 0, 28, DateTimeKind.Utc);
        foreach (var rel in GamePaks)
        {
            var p = Path.Combine(GameRoot, rel);
            Directory.CreateDirectory(Path.GetDirectoryName(p)!);
            File.WriteAllText(p, "pak " + rel);
            File.SetLastWriteTimeUtc(p, t);
        }
        Directory.CreateDirectory(Path.Combine(GameRoot, "Bin", "Win64MasterMasterSteamPGO"));
        File.WriteAllText(Path.Combine(GameRoot, "Bin", "Win64MasterMasterSteamPGO", "KingdomCome.exe"), "");
        return this;
    }

    public FakeSteam WithManifestOnly(uint appId, string installDir, long stateFlags, long toDownload = 0, long downloaded = 0)
    {
        WriteManifest(appId, "x", installDir, stateFlags, toDownload, downloaded);
        return this;
    }

    /// <summary>A fresh Modding Tools install: no Data folder at all until the workspace is linked.</summary>
    public FakeSteam WithModdingTools(long stateFlags = 4, bool withToolFolder = true)
    {
        WriteManifest(SteamApp.ModdingTools, "Kingdom Come: Deliverance II Modding tools", "KCD2Mod", stateFlags);
        var bin = Path.GetDirectoryName(MtExe)!;
        Directory.CreateDirectory(bin);
        foreach (var f in new[] { "KingdomCome.exe", "Framework.dll", "CrySystem.dll", "WHGame.dll" })
            File.WriteAllText(Path.Combine(bin, f), "");
        Directory.CreateDirectory(Path.Combine(MtRoot, "Engine"));
        if (withToolFolder) Directory.CreateDirectory(Path.Combine(MtRoot, "Tools", "ModdingWorkspaceSetup"));
        return this;
    }

    /// <summary>The installed launcher's folder (install-manifest.txt, the verdict) and, when <paramref name="staged"/>, the downloaded mod pack.</summary>
    public FakeSteam WithInstall(bool verdictPass = true, bool staged = true)
    {
        Directory.CreateDirectory(AppDir);
        File.WriteAllText(Path.Combine(AppDir, "Cairn.Launcher.exe"), "launcher");
        var lines = new List<string> { "# Cairn install manifest v2" };
        lines.Add($"APP|Cairn.Launcher.exe|{"launcher".Length}|{Sha("launcher")}");
        File.WriteAllLines(Path.Combine(AppDir, ModInstall.ManifestName), lines);
        File.WriteAllText(Path.Combine(AppDir, ModInstall.VerdictName), verdictPass ? "PASS  4 component(s) verified\n" : "FAIL  1 component(s)\n");
        if (staged) WithPack();
        return this;
    }

    public const string ContractJson = """
        { "schema": 2, "id": "kcd2", "game": "kcd2", "version": "0.1.0", "upstream": "0.48.6", "modFolder": "kdcmp", "luaPrefix": "KCD2MP_",
          "agent": "bin/Agent.exe", "relay": "bin/Relay.exe", "native": "bin/Native.dll", "brandPak": "bin/brand.pak",
          "agentConfig": "agent.json", "launcherMin": "0.1.0", "replacesFolders": ["cairn"],
          "lua": { "eventPrefix": "[TEST-EVT] v1 ", "startGameEvent": "sg", "startGameModel": "Test_Model", "startGameTick": "Test_Tick",
                   "startGameRecapStart": "Test_Recap", "startGameDisarm": "Test_Disarm", "joinFlowLoad": "Test_Load", "menuRequest": "Test_Menu" } }
        """;

    /// <summary>A downloaded, verified mod pack for the game, laid out the way <see cref="PackInstaller"/> leaves it.</summary>
    public FakeSteam WithPack(string version = "0.1.0")
    {
        var modManifest = "mod.manifest contents";
        var modPak = "kdcmp.pak contents";
        var files = PackStore.FilesDir(PacksRoot, "kcd2");
        var mod = Path.Combine(files, PackStore.ModFolder);
        var bin = Path.Combine(files, PackStore.BinFolder);
        Directory.CreateDirectory(Path.Combine(mod, "Data"));
        Directory.CreateDirectory(bin);
        File.WriteAllText(Path.Combine(mod, "mod.manifest"), modManifest);
        File.WriteAllText(Path.Combine(mod, "Data", "kdcmp.pak"), modPak);
        var lines = new List<string> { $"# pack|{"kcd2"}|{version}",
            $"MOD|Data\\kdcmp.pak|{modPak.Length}|{Sha(modPak).ToLowerInvariant()}",
            $"MOD|mod.manifest|{modManifest.Length}|{Sha(modManifest).ToLowerInvariant()}" };
        foreach (var program in new[] { "Agent.exe", "Relay.exe", "Native.dll", "brand.pak" })
        {
            File.WriteAllText(Path.Combine(bin, program), "stub " + program);
            lines.Add($"BIN|{program}|{("stub " + program).Length}|{Sha("stub " + program).ToLowerInvariant()}");
        }
        File.WriteAllLines(PackStore.ManifestPath(PacksRoot, "kcd2"), lines);
        File.WriteAllText(PackStore.ContractPath(PacksRoot, "kcd2"), ContractJson.Replace("\"version\": \"0.1.0\"", $"\"version\": \"{version}\""));
        return this;
    }
    private static string Sha(string s) => Convert.ToHexString(SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(s)));

    private void WriteManifest(uint appId, string name, string installDir, long flags, long toDownload = 0, long downloaded = 0)
    {
        File.WriteAllText(AppManifest.PathFor(Library, appId),
            $"\"AppState\"\n{{\n\t\"appid\"\t\t\"{appId}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n" +
            $"\t\"installdir\"\t\t\"{installDir}\"\n\t\"LastOwner\"\t\t\"76561190000000001\"\n" +
            $"\t\"BytesToDownload\"\t\t\"{toDownload}\"\n\t\"BytesDownloaded\"\t\t\"{downloaded}\"\n}}\n");
    }

    public void Dispose()
    {
        try { Directory.Delete(Root, recursive: true); } catch { }
    }
}

public sealed class FakeHost : ISetupHost
{
    public string? SteamRootFromRegistry() => null;
}
