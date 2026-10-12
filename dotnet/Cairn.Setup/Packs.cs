// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text.Json;

namespace Cairn.Setup;

/// <summary>One downloadable mod pack, as listed in the release's packs.json.</summary>
public sealed record PackInfo(string Id, string Game, string Version, string File, string Sha256, long Size);

/// <summary>
/// The catalog a release publishes next to its installer (<c>packs.json</c>): which game mods can be downloaded for that
/// client version. A pack is built together with the client it is released with, so the catalog URL names the client's own
/// version and a pack never meets a client it was not built for.
/// </summary>
public sealed record PackIndex(int Schema, string Client, IReadOnlyList<PackInfo> Packs)
{
    public const int CurrentSchema = 1;
    public const string FileName = "packs.json";

    public PackInfo? ForGame(string gameId) =>
        Packs.FirstOrDefault(p => string.Equals(p.Game, gameId, StringComparison.OrdinalIgnoreCase));

    /// <summary>Reads a catalog; null when it is not one this client understands (bad JSON, a newer schema, a malformed entry).</summary>
    public static PackIndex? Parse(string json)
    {
        try
        {
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;
            int schema = root.GetProperty("schema").GetInt32();
            if (schema != CurrentSchema) return null;
            var packs = new List<PackInfo>();
            foreach (var p in root.GetProperty("packs").EnumerateArray())
            {
                var info = new PackInfo(
                    p.GetProperty("id").GetString() ?? "",
                    p.GetProperty("game").GetString() ?? "",
                    p.GetProperty("version").GetString() ?? "",
                    p.GetProperty("file").GetString() ?? "",
                    (p.GetProperty("sha256").GetString() ?? "").ToLowerInvariant(),
                    p.GetProperty("size").GetInt64());
                if (!IsSafeName(info.Id) || !IsSafeName(info.Game) || info.Version.Length == 0 || !IsSafeFile(info.File)
                    || info.Sha256.Length != 64 || info.Size <= 0) return null;
                packs.Add(info);
            }
            return new PackIndex(schema, root.GetProperty("client").GetString() ?? "", packs);
        }
        catch (Exception ex) when (ex is JsonException or KeyNotFoundException or InvalidOperationException or FormatException)
        {
            return null;
        }
    }

    // ids and file names come from the network: only plain names are accepted, so nothing can point outside the pack store.
    private static bool IsSafeName(string s) => s.Length is > 0 and <= 40 && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_');
    private static bool IsSafeFile(string s) => s.Length is > 0 and <= 120 && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.') && !s.Contains("..");
}

/// <summary>A pack that is downloaded, verified and extracted on this PC.</summary>
public sealed record InstalledPack(string Id, string Version, IReadOnlyList<ManifestEntry> Files);

/// <summary>
/// Where downloaded packs live: <c>&lt;root&gt;\&lt;id&gt;\manifest.txt</c> (the pack's own file list, <c>MOD|rel|size|sha256</c>,
/// first line <c># pack|id|version</c>) and <c>&lt;root&gt;\&lt;id&gt;\files\&lt;rel&gt;</c>. The root sits beside the launcher
/// (<c>%LOCALAPPDATA%\Cairn\packs</c> for an installed one), so it is writable without elevation.
/// </summary>
public static class PackStore
{
    public const string ManifestName = "manifest.txt";
    public const string FilesFolder = "files";

    public static string DefaultRoot => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "packs");

    public static string Dir(string root, string id) => Path.Combine(root, id);
    public static string FilesDir(string root, string id) => Path.Combine(root, id, FilesFolder);
    public static string ManifestPath(string root, string id) => Path.Combine(root, id, ManifestName);

    /// <summary>The downloaded pack, or null when there is none (or its manifest cannot be read).</summary>
    public static InstalledPack? Read(string root, string id)
    {
        var path = ManifestPath(root, id);
        if (!File.Exists(path)) return null;
        try
        {
            string version = "";
            foreach (var line in File.ReadLines(path))
            {
                var f = line.Trim().Split('|');
                if (f.Length >= 3 && f[0] == "# pack") { version = f[2]; break; }
            }
            var files = ModInstall.ReadManifest(path);
            return files.Count == 0 || version.Length == 0 ? null : new InstalledPack(id, version, files);
        }
        catch (IOException) { return null; }
    }

    public static bool IsDownloaded(string root, string id) => Read(root, id) is not null;

    /// <summary>Deletes the downloaded pack (not the mod already placed in a game folder).</summary>
    public static void Remove(string root, string id)
    {
        var dir = Dir(root, id);
        if (Directory.Exists(dir)) Directory.Delete(dir, recursive: true);
    }
}

public sealed record PackResult(bool Ok, string Message);

/// <summary>Downloads one pack, checks it, and puts it in the <see cref="PackStore"/>; nothing half-done is ever left behind.</summary>
public static class PackInstaller
{
    /// <summary>
    /// The zip is hashed while it downloads and refused unless size and sha256 are what the catalog said; every file in it must be
    /// listed in its manifest.txt with the listed size and hash; entries that would land outside the pack folder are refused.
    /// Only then is the previous copy replaced.
    /// </summary>
    public static async Task<PackResult> DownloadAsync(PackInfo pack, Uri url, string root, HttpClient http,
        IProgress<double>? progress = null, CancellationToken ct = default)
    {
        string work = Path.Combine(root, pack.Id + ".new");
        string zipPath = Path.Combine(root, pack.Id + ".download");
        try
        {
            Directory.CreateDirectory(root);
            CleanUp(work, zipPath);

            using (var resp = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, ct).ConfigureAwait(false))
            {
                if (!resp.IsSuccessStatusCode) return new PackResult(false, $"the download server answered {(int)resp.StatusCode}");
                long total = resp.Content.Headers.ContentLength ?? pack.Size;
                if (total != pack.Size) return new PackResult(false, "the file is not the size the catalog lists");

                using var src = await resp.Content.ReadAsStreamAsync(ct).ConfigureAwait(false);
                using var dst = new FileStream(zipPath, FileMode.Create, FileAccess.Write, FileShare.None);
                using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
                var buf = new byte[81920];
                long done = 0;
                int n;
                while ((n = await src.ReadAsync(buf, ct).ConfigureAwait(false)) > 0)
                {
                    if ((done += n) > pack.Size) return new PackResult(false, "the download is larger than the catalog lists");
                    hash.AppendData(buf, 0, n);
                    await dst.WriteAsync(buf.AsMemory(0, n), ct).ConfigureAwait(false);
                    progress?.Report((double)done / pack.Size);
                }
                if (done != pack.Size) return new PackResult(false, "the download ended early");
                if (!string.Equals(Convert.ToHexString(hash.GetHashAndReset()), pack.Sha256, StringComparison.OrdinalIgnoreCase))
                    return new PackResult(false, "the download does not match its checksum");
            }

            var extracted = Extract(zipPath, work, pack);
            if (!extracted.Ok) return extracted;

            var finalDir = PackStore.Dir(root, pack.Id);
            if (Directory.Exists(finalDir)) Directory.Delete(finalDir, recursive: true);
            Directory.Move(work, finalDir);
            return new PackResult(true, $"{pack.Id} {pack.Version} downloaded");
        }
        catch (OperationCanceledException) { throw; }
        catch (HttpRequestException ex) { return new PackResult(false, "could not reach the download server (" + ex.Message + ")"); }
        catch (IOException ex) { return new PackResult(false, "could not write the pack (" + ex.Message + ")"); }
        catch (UnauthorizedAccessException) { return new PackResult(false, "no permission to write the pack folder"); }
        catch (InvalidDataException) { return new PackResult(false, "the download is not a valid pack"); }
        finally { CleanUp(work, zipPath); }
    }

    private static PackResult Extract(string zipPath, string work, PackInfo pack)
    {
        string files = Path.Combine(work, PackStore.FilesFolder);
        Directory.CreateDirectory(files);
        string filesFull = Path.GetFullPath(files) + Path.DirectorySeparatorChar;

        using var zip = ZipFile.OpenRead(zipPath);
        string? manifestText = null;
        foreach (var e in zip.Entries)
        {
            if (e.FullName.EndsWith('/') || e.FullName.EndsWith('\\')) continue;
            if (e.FullName == PackStore.ManifestName)
            {
                using var r = new StreamReader(e.Open());
                manifestText = r.ReadToEnd();
                continue;
            }
            string dest = Path.GetFullPath(Path.Combine(files, e.FullName));
            if (!dest.StartsWith(filesFull, StringComparison.OrdinalIgnoreCase))
                return new PackResult(false, "the pack contains a file outside its folder");
            Directory.CreateDirectory(Path.GetDirectoryName(dest)!);
            e.ExtractToFile(dest, overwrite: true);
        }
        if (manifestText is null) return new PackResult(false, "the pack has no manifest");

        var manifestPath = Path.Combine(work, PackStore.ManifestName);
        File.WriteAllText(manifestPath, $"# pack|{pack.Id}|{pack.Version}\n" + manifestText);
        var entries = ModInstall.ReadManifest(manifestPath);
        if (entries.Count == 0) return new PackResult(false, "the pack lists no files");

        var listed = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var e in entries)
        {
            string path = Path.GetFullPath(Path.Combine(files, e.Rel));
            if (!path.StartsWith(filesFull, StringComparison.OrdinalIgnoreCase)) return new PackResult(false, "the pack lists a path outside its folder");
            var fi = new FileInfo(path);
            if (!fi.Exists) return new PackResult(false, $"{e.Rel} is missing from the pack");
            if (fi.Length != e.Size) return new PackResult(false, $"{e.Rel} is not the listed size");
            using var fs = fi.OpenRead();
            if (!string.Equals(Convert.ToHexString(SHA256.HashData(fs)), e.Sha256, StringComparison.OrdinalIgnoreCase))
                return new PackResult(false, $"{e.Rel} does not match its checksum");
            listed.Add(Path.GetRelativePath(files, path));
        }
        foreach (var f in Directory.EnumerateFiles(files, "*", SearchOption.AllDirectories))
            if (!listed.Contains(Path.GetRelativePath(files, f))) return new PackResult(false, $"{Path.GetRelativePath(files, f)} is not listed in the pack manifest");
        return new PackResult(true, "");
    }

    private static void CleanUp(string work, string zipPath)
    {
        try { if (Directory.Exists(work)) Directory.Delete(work, recursive: true); } catch { }
        try { if (File.Exists(zipPath)) File.Delete(zipPath); } catch { }
    }
}

/// <summary>Where a client finds its packs: the assets of the release that carries its own version, and nowhere else.</summary>
public static class PackCatalog
{
    public const string ReleaseBase = "https://github.com/Cairn-client/launcher/releases/download/";

    /// <summary>The release tag for a client version ("0.2.0+abc" gives "0.2.0"); null when it is not a plain X.Y.Z.</summary>
    public static string? ReleaseTag(string? clientVersion)
    {
        if (string.IsNullOrWhiteSpace(clientVersion)) return null;
        var m = System.Text.RegularExpressions.Regex.Match(clientVersion.Trim(), @"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?=$|[+-])");
        return m.Success ? m.Value : null;
    }

    public static Uri? IndexUrl(string? clientVersion) =>
        ReleaseTag(clientVersion) is { } tag ? new Uri(ReleaseBase + tag + "/" + PackIndex.FileName) : null;

    public static Uri? PackUrl(string? clientVersion, PackInfo pack) =>
        ReleaseTag(clientVersion) is { } tag ? new Uri(ReleaseBase + tag + "/" + pack.File) : null;

    /// <summary>The catalog of this client's release; null when there is no such release yet (a development build) or it cannot be reached.</summary>
    public static async Task<PackIndex?> FetchAsync(HttpClient http, string? clientVersion, CancellationToken ct = default)
    {
        var url = IndexUrl(clientVersion);
        if (url is null) return null;
        try
        {
            using var resp = await http.GetAsync(url, ct).ConfigureAwait(false);
            if (!resp.IsSuccessStatusCode) return null;
            return PackIndex.Parse(await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false));
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException && !ct.IsCancellationRequested) { return null; }
    }
}
