// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
namespace Cairn.Setup;

/// <summary>
/// The one part of the machine the launcher reads that is not a plain file: where Steam is installed (the registry). Behind an interface so the
/// unit tests can look for games in a fake Steam library in a scratch folder without the real machine leaking in.
/// </summary>
public interface ISetupHost
{
    /// <summary>Steam's install folder from the registry (HKCU SteamPath, then HKLM InstallPath), or null.</summary>
    string? SteamRootFromRegistry();
}

public sealed class RealSetupHost : ISetupHost
{
    public static readonly RealSetupHost Instance = new();

    public string? SteamRootFromRegistry()
    {
        if (!OperatingSystem.IsWindows()) return null;
        try
        {
            using (var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(@"Software\Valve\Steam"))
                if (key?.GetValue("SteamPath") is string sp && Usable(sp, out var p)) return p;
            foreach (var view in new[] { Microsoft.Win32.RegistryView.Registry32, Microsoft.Win32.RegistryView.Registry64 })
            {
                using var hklm = Microsoft.Win32.RegistryKey.OpenBaseKey(Microsoft.Win32.RegistryHive.LocalMachine, view);
                using var key = hklm.OpenSubKey(@"SOFTWARE\Valve\Steam");
                if (key?.GetValue("InstallPath") is string ip && Usable(ip, out var p)) return p;
            }
        }
        catch { }
        return null;
    }

    private static bool Usable(string raw, out string path)
    {
        path = SetupPaths.Normalize(raw);
        return path.Length > 0 && Directory.Exists(path);
    }
}

public static class SetupPaths
{
    /// <summary>Backslashes, no trailing separator (Steam writes "c:/program files (x86)/steam").</summary>
    public static string Normalize(string? path)
    {
        if (string.IsNullOrWhiteSpace(path)) return "";
        var p = path.Trim().Replace('/', '\\');
        while (p.Length > 3 && p.EndsWith('\\')) p = p[..^1];
        return p;
    }

    public static bool SamePath(string a, string b)
    {
        try { return string.Equals(Path.GetFullPath(a).TrimEnd('\\'), Path.GetFullPath(b).TrimEnd('\\'), StringComparison.OrdinalIgnoreCase); }
        catch { return string.Equals(a, b, StringComparison.OrdinalIgnoreCase); }
    }
}
