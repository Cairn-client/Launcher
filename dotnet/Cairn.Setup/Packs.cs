// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text.Json;

namespace Cairn.Setup;

/// <summary>One downloadable mod pack, as listed in the mod repository's latest release (<c>pack-index.json</c>).</summary>
public sealed record PackInfo(string Id, string Game, string Version, string File, string Sha256, long Size, string LauncherMin = "", string Upstream = "");

/// <summary>
/// What the launcher needs to know to drive a mod's programs (the pack's <c>pack.json</c>). The mod repository decides these names;
/// the launcher never hard-codes a game's agent, relay or plugin.
/// </summary>
public sealed record PackContract(
    string Id, string Version, string Upstream, string ModFolder, string LuaPrefix,
    string Agent, string Relay, string Native, string BrandPak, string AgentConfig, string LauncherMin,
    IReadOnlyList<string> ReplacesFolders)
{
    public const string FileName = "pack.json";
    public const int CurrentSchema = 2;

    /// <summary>Reads a contract; null when it is not one this launcher understands (bad JSON, another schema, an unsafe path).</summary>
    public static PackContract? Parse(string json)
    {
        try
        {
            using var doc = JsonDocument.Parse(json);
            var r = doc.RootElement;
            if (r.GetProperty("schema").GetInt32() != CurrentSchema) return null;
            string S(string name, string fallback = "") => r.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() ?? fallback : fallback;
            var replaces = new List<string>();
            if (r.TryGetProperty("replacesFolders", out var rf) && rf.ValueKind == JsonValueKind.Array)
                foreach (var e in rf.EnumerateArray()) if (e.GetString() is { } f) replaces.Add(f);
            var c = new PackContract(S("id"), S("version"), S("upstream"), S("modFolder"), S("luaPrefix"),
                S("agent"), S("relay"), S("native"), S("brandPak"), S("agentConfig"), S("launcherMin"), replaces);
            bool ok = IsPlainName(c.Id) && c.Version.Length > 0 && IsPlainName(c.ModFolder) && c.LuaPrefix.Length is > 0 and <= 40
                      && IsPackPath(c.Agent) && IsPackPath(c.Relay) && IsPackPath(c.Native)
                      && (c.BrandPak.Length == 0 || IsPackPath(c.BrandPak)) && (c.AgentConfig.Length == 0 || IsFileName(c.AgentConfig))
                      && replaces.All(IsPlainName);
            return ok ? c : null;
        }
        catch (Exception ex) when (ex is JsonException or KeyNotFoundException or InvalidOperationException or FormatException)
        {
            return null;
        }
    }

    // everything here comes from the network: only plain names and paths that stay inside the pack are accepted
    private static bool IsPlainName(string s) => s.Length is > 0 and <= 40 && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_');
    private static bool IsFileName(string s) => s.Length is > 0 and <= 80 && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.') && !s.Contains("..");
    private static bool IsPackPath(string s) => s.Length is > 0 and <= 120 && !s.Contains("..") && !Path.IsPathRooted(s) && !s.Contains(':')
        && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.' or '/');

    /// <summary>A program of the pack as a path on this PC (the pack's files folder + the contract's relative path).</summary>
    public static string Resolve(string filesDir, string relative) => Path.GetFullPath(Path.Combine(filesDir, relative.Replace('/', Path.DirectorySeparatorChar)));
}

/// <summary>
/// The catalog a mod repository publishes with each release (<c>pack-index.json</c>): the one pack that release carries, with the
/// zip's size and sha256. A mod is released on its own, so its catalog says nothing about the launcher's version except the
/// minimum the pack needs.
/// </summary>
public sealed record PackIndex(int Schema, IReadOnlyList<PackInfo> Packs)
{
    public const int CurrentSchema = 2;
    public const string FileName = "pack-index.json";

    public PackInfo? ForGame(string gameId) =>
        Packs.FirstOrDefault(p => string.Equals(p.Game, gameId, StringComparison.OrdinalIgnoreCase));

    /// <summary>Reads a catalog; null when it is not one this launcher understands (bad JSON, another schema, a malformed entry).</summary>
    public static PackIndex? Parse(string json)
    {
        try
        {
            using var doc = JsonDocument.Parse(json);
            var p = doc.RootElement;
            if (p.GetProperty("schema").GetInt32() != CurrentSchema) return null;
            string Opt(string name) => p.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() ?? "" : "";
            var info = new PackInfo(
                p.GetProperty("id").GetString() ?? "",
                p.GetProperty("game").GetString() ?? "",
                p.GetProperty("version").GetString() ?? "",
                p.GetProperty("file").GetString() ?? "",
                (p.GetProperty("sha256").GetString() ?? "").ToLowerInvariant(),
                p.GetProperty("size").GetInt64(),
                Opt("launcherMin"), Opt("upstream"));
            if (!IsSafeName(info.Id) || !IsSafeName(info.Game) || !IsPlainVersion(info.Version) || !IsSafeFile(info.File)
                || info.Sha256.Length != 64 || info.Size <= 0) return null;
            return new PackIndex(CurrentSchema, new[] { info });
        }
        catch (Exception ex) when (ex is JsonException or KeyNotFoundException or InvalidOperationException or FormatException)
        {
            return null;
        }
    }

    // ids, versions and file names come from the network: only plain names are accepted, so nothing can point outside the pack store
    private static bool IsSafeName(string s) => s.Length is > 0 and <= 40 && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_');
    private static bool IsSafeFile(string s) => s.Length is > 0 and <= 120 && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.') && !s.Contains("..");
    private static bool IsPlainVersion(string s) => System.Text.RegularExpressions.Regex.IsMatch(s, @"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$");
}

/// <summary>A pack that is downloaded, verified and extracted on this PC.</summary>
public sealed record InstalledPack(string Id, string Version, IReadOnlyList<ManifestEntry> Files, PackContract? Contract = null);

/// <summary>
/// Where downloaded packs live: <c>&lt;root&gt;\&lt;id&gt;\manifest.txt</c> (the pack's own file list, <c>MOD|rel|size|sha256</c> and
/// <c>BIN|rel|size|sha256</c>, first line <c># pack|id|version</c>), <c>&lt;root&gt;\&lt;id&gt;\pack.json</c> (the contract) and
/// <c>&lt;root&gt;\&lt;id&gt;\files\mod\&lt;rel&gt;</c> / <c>files\bin\&lt;rel&gt;</c>. MOD files are put into the game's mod folder; BIN files
/// (the programs that run a session) stay here and are started from here. The root sits beside the launcher
/// (<c>%LOCALAPPDATA%\Cairn\packs</c> for an installed one), so it is writable without elevation.
/// </summary>
public static class PackStore
{
    public const string ManifestName = "manifest.txt";
    public const string FilesFolder = "files";
    public const string ModFolder = "mod";
    public const string BinFolder = "bin";

    public static string DefaultRoot => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "packs");

    public static string Dir(string root, string id) => Path.Combine(root, id);
    public static string FilesDir(string root, string id) => Path.Combine(root, id, FilesFolder);
    public static string ManifestPath(string root, string id) => Path.Combine(root, id, ManifestName);
    public static string ContractPath(string root, string id) => Path.Combine(root, id, PackContract.FileName);

    /// <summary>Where a manifest entry's file lies in the store: <c>files\mod\rel</c> for MOD, <c>files\bin\rel</c> for BIN.</summary>
    public static string EntryPath(string root, string id, ManifestEntry e) =>
        Path.Combine(FilesDir(root, id), KindFolder(e.Kind), e.Rel);

    public static string KindFolder(string kind) => string.Equals(kind, "BIN", StringComparison.OrdinalIgnoreCase) ? BinFolder : ModFolder;

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
            return files.Count == 0 || version.Length == 0 ? null : new InstalledPack(id, version, files, ReadContract(root, id));
        }
        catch (IOException) { return null; }
    }

    /// <summary>The pack's contract; null when the pack has none (a pack built for an older launcher) or it cannot be read.</summary>
    public static PackContract? ReadContract(string root, string id)
    {
        var path = ContractPath(root, id);
        try { return File.Exists(path) ? PackContract.Parse(File.ReadAllText(path)) : null; }
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
    /// listed in its manifest.txt with the listed size and hash; entries that would land outside the pack folder are refused; the
    /// contract must be one this launcher understands and name programs the pack really carries.
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
            // what the programs wrote beside themselves (their settings and logs) survives an update; the new pack's own files win
            CarryOver(Path.Combine(finalDir, PackStore.FilesFolder, PackStore.BinFolder), Path.Combine(work, PackStore.FilesFolder, PackStore.BinFolder));
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
        string? manifestText = null, contractText = null;
        foreach (var e in zip.Entries)
        {
            if (e.FullName.EndsWith('/') || e.FullName.EndsWith('\\')) continue;
            if (e.FullName == PackStore.ManifestName || e.FullName == PackContract.FileName)
            {
                using var r = new StreamReader(e.Open());
                if (e.FullName == PackStore.ManifestName) manifestText = r.ReadToEnd(); else contractText = r.ReadToEnd();
                continue;
            }
            string dest = Path.GetFullPath(Path.Combine(files, e.FullName));
            if (!dest.StartsWith(filesFull, StringComparison.OrdinalIgnoreCase))
                return new PackResult(false, "the pack contains a file outside its folder");
            Directory.CreateDirectory(Path.GetDirectoryName(dest)!);
            e.ExtractToFile(dest, overwrite: true);
        }
        if (manifestText is null) return new PackResult(false, "the pack has no manifest");
        if (contractText is null) return new PackResult(false, "the pack has no contract (pack.json)");
        var contract = PackContract.Parse(contractText);
        if (contract is null) return new PackResult(false, "the pack's contract is not one this launcher understands");
        if (!string.Equals(contract.Id, pack.Id, StringComparison.OrdinalIgnoreCase) || contract.Version != pack.Version)
            return new PackResult(false, "the pack's contract does not match its catalog entry");

        var manifestPath = Path.Combine(work, PackStore.ManifestName);
        File.WriteAllText(manifestPath, $"# pack|{pack.Id}|{pack.Version}\n" + manifestText);
        File.WriteAllText(Path.Combine(work, PackContract.FileName), contractText);
        var entries = ModInstall.ReadManifest(manifestPath);
        if (entries.Count == 0) return new PackResult(false, "the pack lists no files");
        if (!entries.Any(e => e.Kind == "MOD")) return new PackResult(false, "the pack lists no mod files");

        var listed = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var e in entries)
        {
            string path = Path.GetFullPath(Path.Combine(files, PackStore.KindFolder(e.Kind), e.Rel));
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

        // the contract may only name programs the pack carries
        foreach (var program in new[] { contract.Agent, contract.Relay, contract.Native })
            if (!listed.Contains(program.Replace('/', Path.DirectorySeparatorChar)))
                return new PackResult(false, $"the contract names {program}, which the pack does not carry");
        return new PackResult(true, "");
    }

    /// <summary>Copies the files the old pack's bin folder gained after it was installed (not in the new one) into the new one.</summary>
    private static void CarryOver(string oldBin, string newBin)
    {
        if (!Directory.Exists(oldBin) || !Directory.Exists(newBin)) return;
        try
        {
            foreach (var f in Directory.EnumerateFiles(oldBin, "*", SearchOption.AllDirectories))
            {
                var rel = Path.GetRelativePath(oldBin, f);
                var dest = Path.Combine(newBin, rel);
                if (File.Exists(dest)) continue;
                Directory.CreateDirectory(Path.GetDirectoryName(dest)!);
                File.Copy(f, dest);
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { /* a log that is in use stays behind; the update goes on */ }
    }

    private static void CleanUp(string work, string zipPath)
    {
        try { if (Directory.Exists(work)) Directory.Delete(work, recursive: true); } catch { }
        try { if (File.Exists(zipPath)) File.Delete(zipPath); } catch { }
    }
}

/// <summary>Where a launcher finds a game's mod: the latest release of the game's own mod repository, and nowhere else.</summary>
public static class PackCatalog
{
    /// <summary>The repository ("owner/name") must be plain: it comes from the game profile, never from the network.</summary>
    public static bool IsRepo(string? repo) =>
        repo is not null && System.Text.RegularExpressions.Regex.IsMatch(repo, @"^[A-Za-z0-9][A-Za-z0-9-]{0,38}/[A-Za-z0-9._-]{1,100}$");

    public static Uri? IndexUrl(string? repo) =>
        IsRepo(repo) ? new Uri($"https://github.com/{repo}/releases/latest/download/{PackIndex.FileName}") : null;

    public static Uri? PackUrl(string? repo, PackInfo pack) =>
        IsRepo(repo) ? new Uri($"https://github.com/{repo}/releases/download/{pack.Version}/{pack.File}") : null;

    /// <summary>The newest catalog of the repository; null when it has no release yet or cannot be reached.</summary>
    public static async Task<PackIndex?> FetchAsync(HttpClient http, string? repo, CancellationToken ct = default)
    {
        var url = IndexUrl(repo);
        if (url is null) return null;
        try
        {
            using var resp = await http.GetAsync(url, ct).ConfigureAwait(false);
            if (!resp.IsSuccessStatusCode) return null;
            return PackIndex.Parse(await resp.Content.ReadAsStringAsync(ct).ConfigureAwait(false));
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException && !ct.IsCancellationRequested) { return null; }
    }

    /// <summary>The newer pack of the same game, when the downloaded one is older (both plain X.Y.Z); null when there is nothing newer.</summary>
    public static PackInfo? UpdateFor(InstalledPack? installed, PackInfo? latest) =>
        installed is not null && latest is not null && Version.TryParse(installed.Version, out var a) && Version.TryParse(latest.Version, out var b) && b > a
            ? latest : null;

    /// <summary>Whether this launcher is new enough for the pack (its contract's / catalog's <c>launcherMin</c>); true when the pack names none.</summary>
    public static bool LauncherIsEnough(string? launcherMin, string? launcherVersion)
    {
        if (string.IsNullOrWhiteSpace(launcherMin)) return true;
        return Version.TryParse(launcherMin, out var need) && Version.TryParse((launcherVersion ?? "").Split('+', '-')[0], out var have) && have >= need;
    }
}
