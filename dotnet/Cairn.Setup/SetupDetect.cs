// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
namespace Cairn.Setup;

/// <summary>Steam's install folder and every library it knows about.</summary>
public sealed record SteamInstall(string? Root, IReadOnlyList<string> Libraries)
{
    public bool Found => Root is not null;

    /// <summary>
    /// <paramref name="rootOverride"/> replaces the registry lookup (Setup's
    /// /STEAMROOT, the tests' fake libraries); a non-empty override that does
    /// not exist means "no Steam", so both "not installed" pages stay reachable
    /// from a fixture. A missing or malformed libraryfolders.vdf degrades to
    /// "just the root"; a library on a drive that is not connected is skipped.
    /// </summary>
    public static SteamInstall Locate(ISetupHost host, string? rootOverride)
    {
        string? root;
        if (rootOverride is not null)
        {
            var p = SetupPaths.Normalize(rootOverride);
            root = p.Length > 0 && Directory.Exists(p) ? p : null;
        }
        else root = host.SteamRootFromRegistry();

        if (root is null) return new SteamInstall(null, Array.Empty<string>());

        var libs = new List<string> { root };
        try
        {
            var vdf = Path.Combine(root, "steamapps", "libraryfolders.vdf");
            if (File.Exists(vdf))
            {
                foreach (var pair in Vdf.Pairs(File.ReadAllText(vdf)))
                {
                    if (!string.Equals(pair.Key, "path", StringComparison.OrdinalIgnoreCase)) continue;
                    var lib = SetupPaths.Normalize(pair.Value);
                    if (lib.Length == 0 || libs.Any(l => SetupPaths.SamePath(l, lib))) continue;
                    if (Directory.Exists(lib)) libs.Add(lib);
                }
            }
        }
        catch { /* unreadable vdf: the root alone */ }
        return new SteamInstall(root, libs);
    }

    /// <summary>The first library holding a manifest for the app.</summary>
    public AppManifest? Manifest(uint appId)
    {
        foreach (var lib in Libraries)
        {
            var m = AppManifest.TryRead(lib, appId);
            if (m is not null) return m;
        }
        return null;
    }
}
