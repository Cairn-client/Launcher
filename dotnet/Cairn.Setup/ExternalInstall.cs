// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Security.Cryptography;

namespace Cairn.Setup;

/// <summary>
/// Places an external mod (<see cref="PackKinds.External"/>) in the game's own folder, in the folder its contract names
/// (<c>installDir</c>, relative to the game's folder). Cairn writes a marker file in that folder listing what it put there, so it
/// only ever updates or removes its own files, and never touches a folder of the same name that the player (or the mod's own
/// installer) made.
/// </summary>
public static class ExternalInstall
{
    public const string MarkerName = ".cairn-pack";

    /// <summary>The pack's contract when it is an external one; null for anything else (no pack, or a mod that uses the Modding Tools).</summary>
    public static PackContract? ContractOf(string packsRoot, string packId) =>
        PackStore.ReadContract(packsRoot, packId) is { Kind: PackKinds.External } c ? c : null;

    /// <summary>The folder the mod goes to inside the game's folder; null when the contract's folder would leave it.</summary>
    public static string? TargetDir(string gameRoot, PackContract contract)
    {
        if (!PackContract.IsGameRelativeDir(contract.InstallDir)) return null;
        string root = Path.GetFullPath(gameRoot).TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar;
        if (contract.InstallDir == PackContract.GameRoot) return root.TrimEnd(Path.DirectorySeparatorChar);   // the game's own folder
        string target = Path.GetFullPath(Path.Combine(root, contract.InstallDir.Replace('/', Path.DirectorySeparatorChar)));
        return target.StartsWith(root, StringComparison.OrdinalIgnoreCase) && target.Length > root.Length ? target : null;
    }

    /// <summary>What the marker says is placed: the pack's id and version and the files, or null when there is no marker.</summary>
    public static (string Id, string Version, IReadOnlyList<string> Files)? ReadMarker(string target)
    {
        var path = Path.Combine(target, MarkerName);
        try
        {
            if (!File.Exists(path)) return null;
            var lines = File.ReadAllLines(path);
            var head = lines.Length > 0 ? lines[0].Split('|') : Array.Empty<string>();
            if (head.Length < 3 || head[0] != "# pack") return null;
            return (head[1], head[2], lines.Skip(1).Where(l => l.Length > 0).ToList());
        }
        catch (IOException) { return null; }
        catch (UnauthorizedAccessException) { return null; }
    }

    /// <summary>
    /// Whether the downloaded mod is really in the game's folder: placed by Cairn at this pack's version, and every file is there at the
    /// listed size. Sizes only (it runs on every repaint of the game list).
    /// </summary>
    public static bool IsPlaced(string gameRoot, string packsRoot, string packId)
    {
        if (ContractOf(packsRoot, packId) is not { } c || PackStore.Read(packsRoot, packId) is not { } pack || TargetDir(gameRoot, c) is not { } target) return false;
        if (ReadMarker(target) is not { } m || m.Id != packId || m.Version != pack.Version) return false;
        return pack.Files.Where(f => f.Kind == "MOD").All(f =>
        {
            var fi = new FileInfo(Path.Combine(target, f.Rel.Replace('/', Path.DirectorySeparatorChar)));
            return fi.Exists && fi.Length == f.Size;
        });
    }

    /// <summary>Puts the downloaded mod in the game's folder (a first install or an update).</summary>
    public static ModInstall.PlaceResult Place(string gameRoot, string packsRoot, string packId)
    {
        try
        {
            if (ContractOf(packsRoot, packId) is not { } c) return new ModInstall.PlaceResult(false, "this mod does not install into the game's folder");
            if (PackStore.Read(packsRoot, packId) is not { } pack) return new ModInstall.PlaceResult(false, "the mod has not been downloaded yet");
            if (!Directory.Exists(gameRoot)) return new ModInstall.PlaceResult(false, "the game's folder was not found");
            if (TargetDir(gameRoot, c) is not { } target) return new ModInstall.PlaceResult(false, "the mod names a folder outside the game");
            var mods = pack.Files.Where(e => e.Kind == "MOD").ToList();
            if (mods.Count == 0) return new ModInstall.PlaceResult(false, "the mod has not been downloaded yet");

            foreach (var e in mods)
                if (!Matches(PackStore.EntryPath(packsRoot, packId, e), e, out var why))
                    return new ModInstall.PlaceResult(false, $"the downloaded copy of {e.Rel} is {why}");

            var marker = ReadMarker(target);
            bool atRoot = c.InstallDir == PackContract.GameRoot;
            if (marker is { } old && old.Id != packId)
                return new ModInstall.PlaceResult(false, atRoot ? "the game's folder already holds another mod's files" : $"{c.InstallDir} belongs to another mod");
            if (!atRoot && Directory.Exists(target) && marker is null && Directory.EnumerateFileSystemEntries(target).Any())
                return new ModInstall.PlaceResult(false, $"a folder named {c.InstallDir} is already in the game and Cairn did not put it there; remove it first");
            if (atRoot)
            {
                // in the game's own folder there is no folder to call foreign: a file of the game, or of the player, with the same name and other bytes is theirs and stays
                var ours = new HashSet<string>((marker?.Files ?? Array.Empty<string>()).Select(Norm), StringComparer.OrdinalIgnoreCase);
                foreach (var e in mods)
                {
                    var existing = Path.Combine(target, Norm(e.Rel));
                    if (File.Exists(existing) && !ours.Contains(Norm(e.Rel)) && !Matches(existing, e, out _))
                        return new ModInstall.PlaceResult(false, $"{e.Rel} is already in the game and is not Cairn's; remove it first");
                }
            }

            Directory.CreateDirectory(target);
            // the files an earlier version placed and this one no longer has
            var keep = new HashSet<string>(mods.Select(m => Norm(m.Rel)), StringComparer.OrdinalIgnoreCase);
            if (marker is { } prev) foreach (var rel in prev.Files.Where(f => !keep.Contains(Norm(f)))) DeleteInside(target, rel);

            foreach (var e in mods)
            {
                var dest = Path.GetFullPath(Path.Combine(target, Norm(e.Rel)));
                if (!dest.StartsWith(target + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
                    return new ModInstall.PlaceResult(false, $"{e.Rel} would land outside the mod's folder");
                Directory.CreateDirectory(Path.GetDirectoryName(dest)!);
                var tmp = dest + ".cairn-new";
                File.Copy(PackStore.EntryPath(packsRoot, packId, e), tmp, overwrite: true);
                if (File.Exists(dest)) new FileInfo(dest).IsReadOnly = false;
                File.Move(tmp, dest, overwrite: true);
            }
            foreach (var e in mods)
                if (!Matches(Path.Combine(target, Norm(e.Rel)), e, out var why))
                    return new ModInstall.PlaceResult(false, $"{e.Rel} is {why} after copying");

            // the marker is written last: a folder with a marker is a complete one
            File.WriteAllText(Path.Combine(target, MarkerName), $"# pack|{packId}|{pack.Version}\n" + string.Join("\n", mods.Select(m => Norm(m.Rel))) + "\n");
            return new ModInstall.PlaceResult(true, $"placed {mods.Count} file(s)");
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return new ModInstall.PlaceResult(false, ex is IOException ? "a file is in use (is the game running?)" : "no permission to write the game's folder");
        }
    }

    /// <summary>Takes the mod back out: only the files the marker lists, then the folders that are left empty. A folder with no marker is not ours and stays.</summary>
    public static ModInstall.PlaceResult Unplace(string gameRoot, string packsRoot, string packId)
    {
        try
        {
            if (ContractOf(packsRoot, packId) is not { } c || TargetDir(gameRoot, c) is not { } target) return new ModInstall.PlaceResult(true, "nothing to remove");
            if (!Directory.Exists(target)) return new ModInstall.PlaceResult(true, "nothing to remove");
            if (ReadMarker(target) is not { } m || m.Id != packId) return new ModInstall.PlaceResult(true, "the folder is not Cairn's, so it stays");
            foreach (var rel in m.Files) DeleteInside(target, rel);
            File.Delete(Path.Combine(target, MarkerName));
            if (c.InstallDir == PackContract.GameRoot)
                foreach (var rel in m.Files) PruneEmptyParents(target, rel);   // the game's own folder is never removed, nor a folder that holds anything else
            else
                RemoveEmptyDirs(new DirectoryInfo(target));
            return new ModInstall.PlaceResult(true, $"removed {m.Files.Count} file(s)");
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return new ModInstall.PlaceResult(false, ex is IOException ? "a file is in use (is the game running?)" : "no permission to change the game's folder");
        }
    }

    /// <summary>The companion program (the mod's bridge) of a downloaded game mod of either kind, as a path in the pack store; null when it has none or the file is not there.</summary>
    public static string? CompanionPath(string packsRoot, string packId)
    {
        if (PackStore.ReadContract(packsRoot, packId) is not { Companion.Length: > 0, Kind: not PackKinds.Content } c) return null;
        var path = PackContract.Resolve(PackStore.FilesDir(packsRoot, packId), c.Companion);
        return File.Exists(path) ? path : null;
    }

    private static string Norm(string rel) => rel.Replace('/', Path.DirectorySeparatorChar);

    private static void DeleteInside(string target, string rel)
    {
        var path = Path.GetFullPath(Path.Combine(target, Norm(rel)));
        if (!path.StartsWith(target + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) || !File.Exists(path)) return;
        new FileInfo(path).IsReadOnly = false;
        File.Delete(path);
    }

    // from the folder a removed file was in, up to (not including) the mod's folder: each one that is now empty goes
    private static void PruneEmptyParents(string target, string rel)
    {
        var dir = Path.GetDirectoryName(Path.GetFullPath(Path.Combine(target, Norm(rel))));
        while (!string.IsNullOrEmpty(dir) && dir.Length > target.Length && dir.StartsWith(target + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)
               && Directory.Exists(dir) && !Directory.EnumerateFileSystemEntries(dir).Any())
        {
            Directory.Delete(dir);
            dir = Path.GetDirectoryName(dir);
        }
    }

    private static bool RemoveEmptyDirs(DirectoryInfo dir)
    {
        foreach (var sub in dir.EnumerateDirectories()) RemoveEmptyDirs(sub);
        if (dir.EnumerateFileSystemInfos().Any()) return false;
        dir.Delete();
        return true;
    }

    private static bool Matches(string path, ManifestEntry e, out string why)
    {
        why = "";
        var fi = new FileInfo(path);
        if (!fi.Exists) { why = "missing"; return false; }
        if (fi.Length != e.Size) { why = "not the listed size"; return false; }
        using var fs = fi.OpenRead();
        if (!string.Equals(Convert.ToHexString(SHA256.HashData(fs)), e.Sha256, StringComparison.OrdinalIgnoreCase)) { why = "not the listed bytes"; return false; }
        return true;
    }
}
