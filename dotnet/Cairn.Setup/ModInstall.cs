// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
namespace Cairn.Setup;

public sealed record ManifestEntry(string Kind, string Rel, long Size, string Sha256);

/// <summary>The manifest of a pack's files (<c>KIND|rel|size|sha256</c> per line) and the result of placing a pack into a game's folder.</summary>
public static class ModInstall
{
    public const string ManifestName = "install-manifest.txt";
    public const string VerdictName = "install-verify.txt";

    public sealed record PlaceResult(bool Ok, string Message);

    public static List<ManifestEntry> ReadManifest(string path)
    {
        var list = new List<ManifestEntry>();
        foreach (var raw in File.ReadAllLines(path))
        {
            var line = raw.Trim();
            if (line.Length == 0 || line[0] == '#') continue;
            var f = line.Split('|');
            if (f.Length < 4 || f[1].Length == 0) continue;
            if (!long.TryParse(f[2], out long size)) continue;
            list.Add(new ManifestEntry(f[0].ToUpperInvariant(), f[1], size, f[3].ToLowerInvariant()));
        }
        return list;
    }
}
