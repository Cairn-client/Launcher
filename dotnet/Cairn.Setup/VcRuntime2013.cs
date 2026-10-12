// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
namespace Cairn.Setup;

/// <summary>
/// beta patch (2.3): Microsoft's Visual C++ 2013 runtime (x64). The Modding Tools' trace server
/// (trace_server.run.exe) needs it; without it Windows says "MSVCP120.dll was not found" when the
/// Modding Tools start (a tester's screenshot). Neither the game nor the mod needs it, so its
/// checklist step never holds Host or Join back.
/// </summary>
public static class VcRuntime2013
{
    /// <summary>The file Windows reported missing; it lives in System32 for the x64 runtime.</summary>
    public const string ProbeFile = "msvcp120.dll";

    /// <summary>Microsoft's page for the supported downloads, where Visual Studio 2013 (VC++ 12.0) is listed.</summary>
    public const string DownloadPage = "https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist";

    /// <summary>Steam ships the redistributable with many games, in every library it uses for them.</summary>
    public static readonly string[] RedistRelPath =
        ["steamapps", "common", "Steamworks Shared", "_CommonRedist", "vcredist", "2013", "vcredist_x64.exe"];

    /// <summary>Silent install, no reboot; Microsoft's installer asks Windows for permission itself (one UAC prompt).</summary>
    public const string InstallArguments = "/install /quiet /norestart";

    /// <summary>Exit codes that leave the runtime installed: done, done (restart pending), a newer one already there.</summary>
    public static bool InstalledByExitCode(int code) => code is 0 or 3010 or 1638;

    public static bool PresentIn(string systemDirectory)
    {
        try { return File.Exists(Path.Combine(systemDirectory, ProbeFile)); }
        catch (Exception) { return false; }
    }

    // Cairn: the Visual C++ 2010 runtime (x64). A fresh Windows install stopped the Modding Tools' trace_server_launcher.exe
    // with "MSVCR100.dll was not found" -- the 2013 runtime does not carry it. Steam ships it beside the 2013 one.

    /// <summary>The file Windows reported missing for the 2010 runtime; it lives in System32 for the x64 runtime.</summary>
    public const string ProbeFile2010 = "msvcr100.dll";

    public static readonly string[] RedistRelPath2010 =
        ["steamapps", "common", "Steamworks Shared", "_CommonRedist", "vcredist", "2010", "vcredist_x64.exe"];

    /// <summary>The 2010 redistributable takes the old switches: quiet, no reboot.</summary>
    public const string InstallArguments2010 = "/q /norestart";

    public static bool Present2010In(string systemDirectory)
    {
        try { return File.Exists(Path.Combine(systemDirectory, ProbeFile2010)); }
        catch (Exception) { return false; }
    }

    public static string? FindRedist2010(IEnumerable<string?> libraries) => FindIn(libraries, RedistRelPath2010);

    /// <summary>The first library (Steam's own folder first) that holds the x64 redistributable, or null.</summary>
    public static string? FindRedist(IEnumerable<string?> libraries) => FindIn(libraries, RedistRelPath);

    private static string? FindIn(IEnumerable<string?> libraries, string[] relPath)
    {
        foreach (var lib in libraries)
        {
            if (string.IsNullOrWhiteSpace(lib)) continue;
            try
            {
                var p = Path.Combine([lib, .. relPath]);
                if (File.Exists(p)) return Path.GetFullPath(p);
            }
            catch (Exception) { }
        }
        return null;
    }
}
