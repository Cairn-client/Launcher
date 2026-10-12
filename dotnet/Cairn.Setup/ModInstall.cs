// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
using System.Security.Cryptography;

namespace Cairn.Setup;

public sealed record ManifestEntry(string Kind, string Rel, long Size, string Sha256);

public sealed class ModReport
{
    /// <summary>No install-manifest.txt beside the launcher: a development build run from the repo. The mod check does not apply.</summary>
    public bool DevelopmentBuild { get; init; }
    public bool VerdictPass { get; init; }
    public string VerdictLine { get; init; } = "";
    /// <summary>Install-folder files that are missing or the wrong size (sizes only: hashing ~700 files is not a startup check).</summary>
    public List<string> AppProblems { get; init; } = new();
    /// <summary>Mod files that are not in &lt;MT&gt;\Mods\cairn at all.</summary>
    public List<string> ModMissing { get; init; } = new();
    /// <summary>Mod files that are there but are not the bytes Setup shipped (sha256: two small files) -- a hand-built pak, typically.</summary>
    public List<string> ModDifferent { get; init; } = new();
    /// <summary>The downloaded pack (the mod's files, verified when it was fetched) is on this PC and intact, so the launcher can place it.</summary>
    public bool StagedAvailable { get; init; }
    /// <summary>No mod pack has been downloaded yet: the installer no longer carries the game mod, the launcher fetches it.</summary>
    public bool PackMissing { get; init; }
    /// <summary>An earlier release's copy of our mod is still in &lt;MT&gt;\Mods\kdcmp (the folder was renamed to cairn); with both there, the game loads the mod twice.</summary>
    public bool LegacyFolder { get; init; }
    /// <summary>A cairn folder is there whose mod.manifest is not ours, and nothing says we put it there.</summary>
    public bool ForeignFolder { get; init; }
    public string TargetDir { get; init; } = "";

    public bool Placed => !DevelopmentBuild && !PackMissing && !LegacyFolder && ModMissing.Count == 0 && !ForeignFolder;

    /// <summary>
    /// What blocks Host and Join: Setup's own verdict said the install failed, the mod is not
    /// there, or the folder is somebody else's. Files that differ from the shipped bytes only
    /// warn (<see cref="Warnings"/>): the same rule as the launcher's InstallIntegrity check --
    /// a false alarm that stops a player connecting is worse than a warned mismatch, and a
    /// hand-built pak is the developer's normal state. They are never overwritten either.
    /// </summary>
    public bool Ok => DevelopmentBuild || (VerdictPass && Placed);

    public IEnumerable<string> Warnings => AppProblems.Concat(ModDifferent);
}

/// <summary>
/// The mod half of setup (checklist step 6). The installer's own files are checked against its install-manifest.txt
/// (&lt;kind&gt;|&lt;rel&gt;|&lt;size&gt;|&lt;sha256&gt;, APP entries = files in the install folder). The game mod comes from a
/// downloaded <see cref="PackStore"/> pack: its manifest lists the MOD files (relative to &lt;MT&gt;\Mods\cairn) and its
/// files are what the launcher places once the Modding Tools are set up.
/// </summary>
public static class ModInstall
{
    public const string ManifestName = "install-manifest.txt";
    public const string VerdictName = "install-verify.txt";
    /// <summary>The pack that holds the Kingdom Come: Deliverance II mod (see <see cref="PackStore"/>).</summary>
    public const string PackId = "kcd2";
    /// <summary>Built by the agent from the player's own game files; ours, kept across a re-place.</summary>
    public const string KeysPak = @"Data\cairn_keys.pak";

    public static string TargetDir(string moddingToolsRoot) => Path.Combine(moddingToolsRoot, "Mods", "cairn");

    // ---- the folder an earlier release used ------------------------------------------------------------------------------
    // 0.1.x placed the mod at Mods\kdcmp; the rename moved it to Mods\cairn. Left behind, the old copy loads beside the new one (two
    // mods, every hook twice). It is removed only when it is provably ours: our mod id and name, and nothing in it but our three files.

    /// <summary>
    /// Whether the downloaded mod is really in &lt;MT&gt;\Mods\cairn: every file the pack lists is there at the listed size, and no
    /// earlier release's copy sits beside it. Sizes only (it runs on every repaint of the game list); the full check is <see cref="Check"/>.
    /// Works for a development build too, which has no install manifest for <see cref="Check"/> to read.
    /// </summary>
    public static bool IsPlaced(string moddingToolsRoot, string? packsRoot = null)
    {
        var pack = PackStore.Read(packsRoot ?? PackStore.DefaultRoot, PackId);
        if (pack is null || HasLegacyFolder(moddingToolsRoot)) return false;
        var target = TargetDir(moddingToolsRoot);
        return pack.Files.Where(f => f.Kind == "MOD").All(f =>
        {
            var fi = new FileInfo(Path.Combine(target, f.Rel));
            return fi.Exists && fi.Length == f.Size;
        });
    }

    public static string LegacyDir(string moddingToolsRoot) => Path.Combine(moddingToolsRoot, "Mods", "kdcmp");

    private static readonly string[] LegacyFiles = { "mod.manifest", @"Data\kdcmp.pak", @"Data\kdcmp_keys.pak" };

    public static bool HasLegacyFolder(string moddingToolsRoot)
    {
        var dir = LegacyDir(moddingToolsRoot);
        var manifest = Path.Combine(dir, "mod.manifest");
        try
        {
            if (!File.Exists(manifest)) return false;
            var text = File.ReadAllText(manifest);
            if (!text.Contains("<modid>kdcmp</modid>") || !text.Contains("<name>KCD2 Multiplayer</name>")) return false;
            var ours = new HashSet<string>(LegacyFiles.Select(f => Path.GetFullPath(Path.Combine(dir, f))), StringComparer.OrdinalIgnoreCase);
            return Directory.EnumerateFiles(dir, "*", SearchOption.AllDirectories).All(f => ours.Contains(Path.GetFullPath(f)));
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return false; }
    }

    /// <summary>Removes the earlier release's folder when it is ours (see <see cref="HasLegacyFolder"/>); true when it is gone or was never there.</summary>
    public static bool RemoveLegacyFolder(string moddingToolsRoot)
    {
        if (!HasLegacyFolder(moddingToolsRoot)) return true;
        try
        {
            foreach (var f in Directory.EnumerateFiles(LegacyDir(moddingToolsRoot), "*", SearchOption.AllDirectories)) new FileInfo(f).IsReadOnly = false;
            Directory.Delete(LegacyDir(moddingToolsRoot), recursive: true);
            return true;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return false; }
    }

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

    public static ModReport Check(string appDir, string moddingToolsRoot, bool ownsModFolder, string? packsRoot = null)
    {
        packsRoot ??= PackStore.DefaultRoot;
        var manifestPath = Path.Combine(appDir, ManifestName);
        var target = TargetDir(moddingToolsRoot);
        if (!File.Exists(manifestPath))
            return new ModReport { DevelopmentBuild = true, TargetDir = target };

        List<ManifestEntry> entries;
        try { entries = ReadManifest(manifestPath); }
        catch { return new ModReport { VerdictLine = "install-manifest.txt could not be read", TargetDir = target }; }

        string verdict = "";
        try { verdict = File.ReadLines(Path.Combine(appDir, VerdictName)).FirstOrDefault() ?? ""; } catch { }

        var app = new List<string>();
        foreach (var e in entries.Where(e => e.Kind == "APP"))
        {
            var fi = new FileInfo(Path.Combine(appDir, e.Rel));
            if (!fi.Exists) app.Add($"{e.Rel} (missing)");
            else if (fi.Length != e.Size) app.Add($"{e.Rel} ({fi.Length} bytes, expected {e.Size})");
        }

        var pack = PackStore.Read(packsRoot, PackId);
        var mods = (pack?.Files ?? new List<ManifestEntry>()).Where(e => e.Kind == "MOD").ToList();
        string staging = PackStore.FilesDir(packsRoot, PackId);
        var missing = new List<string>();
        var different = new List<string>();
        foreach (var e in mods)
        {
            if (Matches(Path.Combine(target, e.Rel), e, out var why)) continue;
            if (why == "missing") missing.Add(e.Rel);
            else different.Add($"{e.Rel} ({why})");
        }

        bool staged = mods.Count > 0 && mods.All(e => Matches(Path.Combine(staging, e.Rel), e, out _));

        // Ours by content: our mod.manifest has never changed between releases, so a cairn folder whose
        // mod.manifest is not those bytes was put there by someone else -- unless the registry says it was us.
        var manifestEntry = mods.FirstOrDefault(e => string.Equals(e.Rel, "mod.manifest", StringComparison.OrdinalIgnoreCase));
        var targetManifest = Path.Combine(target, "mod.manifest");
        bool foreign = !ownsModFolder && manifestEntry is not null && File.Exists(targetManifest) &&
                       !Matches(targetManifest, manifestEntry, out _);

        return new ModReport
        {
            VerdictPass = verdict.StartsWith("PASS", StringComparison.Ordinal),
            VerdictLine = verdict,
            AppProblems = app,
            ModMissing = missing,
            ModDifferent = foreign ? new List<string>() : different,
            StagedAvailable = staged,
            PackMissing = pack is null,
            LegacyFolder = HasLegacyFolder(moddingToolsRoot),
            ForeignFolder = foreign,
            TargetDir = target,
        };
    }

    private static bool Matches(string path, ManifestEntry e, out string why)
    {
        var fi = new FileInfo(path);
        if (!fi.Exists) { why = "missing"; return false; }
        if (fi.Length != e.Size) { why = "wrong size"; return false; }
        try
        {
            using var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            var hash = Convert.ToHexString(SHA256.HashData(fs)).ToLowerInvariant();
            if (hash != e.Sha256) { why = "wrong content"; return false; }
        }
        catch { why = "could not be read"; return false; }
        why = "";
        return true;
    }

    public sealed record PlaceResult(bool Ok, string Message);

    /// <summary>
    /// Copies the staged mod into &lt;MT&gt;\Mods\cairn. Never over a foreign
    /// cairn (the caller checks <see cref="ModReport.ForeignFolder"/> first).
    /// Prunes the folder the way the installer does for its own upgrades
    ///: everything but our files goes -- loose pak sources there break
    /// the game outright ("114 tables are not loaded") -- and each of our files
    /// is written under a temporary name and moved into place, so a failure
    /// leaves the previous version rather than nothing.
    /// </summary>
    public static PlaceResult Place(string moddingToolsRoot, string? packsRoot = null)
    {
        var target = TargetDir(moddingToolsRoot);
        try
        {
            packsRoot ??= PackStore.DefaultRoot;
            var pack = PackStore.Read(packsRoot, PackId);
            var mods = (pack?.Files ?? new List<ManifestEntry>()).Where(e => e.Kind == "MOD").ToList();
            if (mods.Count == 0) return new PlaceResult(false, "the mod has not been downloaded yet");
            string staging = PackStore.FilesDir(packsRoot, PackId);
            foreach (var e in mods)
                if (!Matches(Path.Combine(staging, e.Rel), e, out var why))
                    return new PlaceResult(false, $"the downloaded copy of {e.Rel} is {why}");
            if (!RemoveLegacyFolder(moddingToolsRoot))
                return new PlaceResult(false, "an earlier copy of the mod (Mods\\kdcmp) could not be removed (is the game running?)");

            Directory.CreateDirectory(Path.Combine(target, "Data"));
            Prune(target, mods.Select(m => m.Rel).Append(KeysPak).ToList());

            foreach (var e in mods)
            {
                var dest = Path.Combine(target, e.Rel);
                Directory.CreateDirectory(Path.GetDirectoryName(dest)!);
                var tmp = dest + ".cairn-new";
                File.Copy(Path.Combine(staging, e.Rel), tmp, overwrite: true);
                if (File.Exists(dest)) new FileInfo(dest).IsReadOnly = false;
                File.Move(tmp, dest, overwrite: true);
            }

            foreach (var e in mods)
                if (!Matches(Path.Combine(target, e.Rel), e, out var why))
                    return new PlaceResult(false, $"{e.Rel} is {why} after copying");
            return new PlaceResult(true, $"placed {mods.Count} file(s)");
        }
        catch (Exception ex)
        {
            return new PlaceResult(false, ex is IOException ? "a file is in use (is the game running?)" : ex.GetType().Name);
        }
    }

    /// <summary>
    /// Takes the mod back out of &lt;MT&gt;Modsirn: only the files the pack lists (and the keys pak built from the player's
    /// own game files), then the folders that are left empty. Anything else in the folder is not ours and stays.
    /// </summary>
    public static PlaceResult Unplace(string moddingToolsRoot, string? packsRoot = null)
    {
        var target = TargetDir(moddingToolsRoot);
        try
        {
            var pack = PackStore.Read(packsRoot ?? PackStore.DefaultRoot, PackId);
            if (!Directory.Exists(target)) return new PlaceResult(true, "nothing to remove");
            int removed = 0;
            foreach (var rel in (pack?.Files.Select(f => f.Rel) ?? Enumerable.Empty<string>()).Append(KeysPak))
            {
                var path = Path.GetFullPath(Path.Combine(target, rel));
                if (!path.StartsWith(Path.GetFullPath(target) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) || !File.Exists(path)) continue;
                new FileInfo(path).IsReadOnly = false;
                File.Delete(path);
                removed++;
            }
            RemoveEmptyDirs(new DirectoryInfo(target));
            return new PlaceResult(true, $"removed {removed} file(s)");
        }
        catch (Exception ex)
        {
            return new PlaceResult(false, ex is IOException ? "a file is in use (is the game running?)" : ex.GetType().Name);
        }
    }

    private static void RemoveEmptyDirs(DirectoryInfo dir)
    {
        foreach (var sub in dir.EnumerateDirectories().ToList())
        {
            if ((sub.Attributes & FileAttributes.ReparsePoint) != 0) continue;   // never follow a link
            RemoveEmptyDirs(sub);
        }
        if (!dir.EnumerateFileSystemInfos().Any()) dir.Delete();
    }

    private static void Prune(string target, List<string> keepRels)
    {
        var keep = new HashSet<string>(keepRels.Select(r => Path.GetFullPath(Path.Combine(target, r))), StringComparer.OrdinalIgnoreCase);
        PruneDir(new DirectoryInfo(target), keep, isRoot: true);
    }

    /// <summary>Never follows a junction or a directory symlink: one is removed as a link, its target untouched.</summary>
    private static void PruneDir(DirectoryInfo dir, HashSet<string> keep, bool isRoot)
    {
        foreach (var entry in dir.EnumerateFileSystemInfos().ToList())
        {
            if (entry is DirectoryInfo sub)
            {
                if ((sub.Attributes & FileAttributes.ReparsePoint) != 0) { sub.Delete(); continue; }
                PruneDir(sub, keep, isRoot: false);
                bool isData = isRoot && string.Equals(sub.Name, "Data", StringComparison.OrdinalIgnoreCase);
                if (!isData && !sub.EnumerateFileSystemInfos().Any()) sub.Delete();
                continue;
            }
            if (keep.Contains(Path.GetFullPath(entry.FullName))) continue;
            if ((entry.Attributes & FileAttributes.ReadOnly) != 0) entry.Attributes &= ~FileAttributes.ReadOnly;
            entry.Delete();
        }
    }
}
