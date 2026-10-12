// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Cairn.Launcher.Models;
using Microsoft.Win32;
using Serilog;

namespace Cairn.Launcher.Services
{
    /// <summary>The outcome of one run of the tool: its exit code and what it printed.</summary>
    public sealed record Dlss5Run(bool Ok, string Output);

    /// <summary>
    /// DLSS 5 in a game, through DLSS 5 Autopilot (MIT, https://github.com/Kizzuwatnaa/DLSS5-Autopilot). The launcher downloads the tool's release the
    /// first time it is wanted, checks it against the SHA256 the release lists, and runs its command line on the game's folder; the tool itself fetches every
    /// part from its own publisher and keeps a record of what it wrote, so it can take all of it out again. Nothing of it ships with Cairn.
    /// </summary>
    internal static class Dlss5Service
    {
        private static readonly HttpClient Http = CreateHttp();
        private static HttpClient CreateHttp()
        {
            var http = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("Cairn-launcher");
            http.DefaultRequestHeaders.Accept.ParseAdd("application/vnd.github+json");
            return http;
        }

        private const long MaxZipBytes = 200L * 1024 * 1024;
        private const string ExeName = "dlss5-autopilot.exe";

        public static string Root => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "tools", "dlss5-autopilot");

        private static string MarkerOf(string gameId) => Path.Combine(Root, "installed-" + new string(gameId.Where(char.IsAsciiLetterOrDigit).ToArray()) + ".txt");

        /// <summary>The graphics adapters' names, from the registry's display class (no extra package needed).</summary>
        public static IReadOnlyList<string> GpuNames()
        {
            var names = new List<string>();
            try
            {
                using var cls = Registry.LocalMachine.OpenSubKey(@"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11cd-bfc1-08002be10318}");
                foreach (string sub in (cls?.GetSubKeyNames() ?? Array.Empty<string>()).Where(s => s.Length == 4 && s.All(char.IsAsciiDigit)))
                {
                    using var k = cls!.OpenSubKey(sub);
                    if (k?.GetValue("DriverDesc") is string d && d.Length > 0) names.Add(d);
                }
            }
            catch (Exception ex) when (ex is System.Security.SecurityException or UnauthorizedAccessException or IOException)
            {
                Log.Debug(ex, "dlss5: the adapters could not be listed from the registry");
            }
            if (names.Count == 0) names.AddRange(GpuNamesFromWindows());
            Log.Information("dlss5: graphics adapters: {Names}", names.Count == 0 ? "none found" : string.Join("; ", names));
            return names;
        }

        /// <summary>What Windows itself lists as video controllers (the registry's display class can come back empty): one short PowerShell call, no extra package.</summary>
        private static IReadOnlyList<string> GpuNamesFromWindows()
        {
            try
            {
                var psi = new ProcessStartInfo("powershell.exe")
                {
                    UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true,
                    StandardOutputEncoding = Encoding.UTF8,
                };
                foreach (string a in new[] { "-NoProfile", "-NonInteractive", "-Command", "(Get-CimInstance Win32_VideoController).Name" }) psi.ArgumentList.Add(a);
                using var p = Process.Start(psi);
                if (p is null) return Array.Empty<string>();
                string text = p.StandardOutput.ReadToEnd();
                if (!p.WaitForExit(8000)) { try { p.Kill(true); } catch (InvalidOperationException) { } }
                return text.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).Where(n => n.Length is > 0 and < 200).ToList();
            }
            catch (Exception ex) when (ex is InvalidOperationException or System.ComponentModel.Win32Exception or IOException)
            {
                Log.Debug(ex, "dlss5: the adapters could not be listed from Windows");
                return Array.Empty<string>();
            }
        }

        /// <summary>The game's folder has had DLSS 5 put in by this launcher (and not taken out since).</summary>
        public static bool IsInstalled(string gameId) => File.Exists(MarkerOf(gameId));

        /// <summary>The newest release of the tool on this PC, downloaded and verified when it is not there yet. Null (with the reason) when it cannot be had.</summary>
        public static async Task<(string? Exe, string Why)> EnsureToolAsync(Action<string> say, CancellationToken ct)
        {
            try
            {
                say("Asking GitHub for the latest DLSS 5 Autopilot...");
                using var doc = JsonDocument.Parse(await Http.GetStringAsync($"https://api.github.com/repos/{Dlss5.Repo}/releases/latest", ct));
                string tag = doc.RootElement.GetProperty("tag_name").GetString() ?? "";
                if (!Dlss5.IsSafeTag(tag)) return (null, "The release has an unexpected version name.");
                string dir = Path.Combine(Root, tag);
                if (FindExe(dir) is { } have) return (have, "");

                var assets = doc.RootElement.GetProperty("assets").EnumerateArray()
                    .Select(a => (Name: a.GetProperty("name").GetString() ?? "", Url: a.GetProperty("browser_download_url").GetString() ?? "", Size: a.GetProperty("size").GetInt64()))
                    .ToList();
                string? zipName = Dlss5.PickZip(assets.Select(a => a.Name));
                var zip = assets.FirstOrDefault(a => a.Name == zipName);
                var sums = assets.FirstOrDefault(a => a.Name.Equals("SHA256SUMS.txt", StringComparison.OrdinalIgnoreCase));
                if (zipName is null || sums.Name is null) return (null, "The release has no Windows build or no checksum file.");
                string prefix = $"https://github.com/{Dlss5.Repo}/";
                if (!zip.Url.StartsWith(prefix, StringComparison.Ordinal) || !sums.Url.StartsWith(prefix, StringComparison.Ordinal)) return (null, "The release points outside its own repository.");
                if (zip.Size <= 0 || zip.Size > MaxZipBytes) return (null, "The release's size is not what was expected.");

                string expected = Dlss5.ParseSums(await Http.GetStringAsync(sums.Url, ct)).GetValueOrDefault(zipName, "");
                if (expected.Length != 64) return (null, "The release's checksum file does not list its Windows build.");

                Directory.CreateDirectory(Root);
                string tmp = Path.Combine(Root, "download-" + Guid.NewGuid().ToString("N") + ".zip");
                try
                {
                    say($"Downloading DLSS 5 Autopilot {tag} ({zip.Size / 1048576.0:0.#} MB)...");
                    await using (var src = await Http.GetStreamAsync(zip.Url, ct))
                    await using (var dst = File.Create(tmp))
                        await src.CopyToAsync(dst, ct);
                    string actual;
                    await using (var f = File.OpenRead(tmp)) actual = Convert.ToHexString(await SHA256.HashDataAsync(f, ct));
                    if (!Dlss5.HashMatches(expected, actual)) return (null, "The download does not match the checksum the release publishes, so it was not used.");
                    say("Checked. Unpacking...");
                    string part = dir + ".part";
                    if (Directory.Exists(part)) Directory.Delete(part, true);
                    ZipFile.ExtractToDirectory(tmp, part);   // refuses an entry that points outside the folder
                    if (Directory.Exists(dir)) Directory.Delete(dir, true);
                    Directory.Move(part, dir);
                }
                finally { try { File.Delete(tmp); } catch (IOException) { } }

                return FindExe(dir) is { } exe ? (exe, "") : (null, "The release holds no " + ExeName + ".");
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or IOException or InvalidDataException or JsonException or KeyNotFoundException or UnauthorizedAccessException)
            {
                Log.Warning(ex, "dlss5: the tool could not be fetched");
                return (null, ex is HttpRequestException or TaskCanceledException ? "GitHub could not be reached." : "The tool could not be unpacked: " + ex.Message);
            }
        }

        private static string? FindExe(string dir)
        {
            try { return Directory.Exists(dir) ? Directory.EnumerateFiles(dir, ExeName, SearchOption.AllDirectories).FirstOrDefault() : null; }
            catch (IOException) { return null; }
        }

        /// <summary>
        /// Runs the tool on a game's folder, out of sight, and returns what it printed. The tool is a windowed program that prints to a console: started
        /// with its output on a pipe it crashed on the first write, so PowerShell starts it with its output in files (the way that was seen to work), and the
        /// launcher reads them back. The exit code is the tool's: 0 = done.
        /// </summary>
        public static async Task<Dlss5Run> RunAsync(string exe, string gameDir, Dlss5Mode mode, Action<string> say, CancellationToken ct)
        {
            string work = Path.Combine(Path.GetTempPath(), "cairn-dlss5-" + Guid.NewGuid().ToString("N"));
            try
            {
                Directory.CreateDirectory(work);
                string outFile = Path.Combine(work, "out.txt"), errFile = Path.Combine(work, "err.txt");
                var psi = new ProcessStartInfo("powershell.exe") { UseShellExecute = false, CreateNoWindow = true, WorkingDirectory = work };
                // an encoded command: no quote in a path or an argument has to survive PowerShell's own command-line reading
                string script = Dlss5.StartScript(exe, Dlss5.ToolArgs(gameDir, mode), outFile, errFile);
                foreach (string a in new[] { "-NoProfile", "-NonInteractive", "-EncodedCommand", Convert.ToBase64String(Encoding.Unicode.GetBytes(script)) }) psi.ArgumentList.Add(a);
                using var p = Process.Start(psi) ?? throw new InvalidOperationException("PowerShell did not start");
                using var limit = CancellationTokenSource.CreateLinkedTokenSource(ct);
                limit.CancelAfter(TimeSpan.FromMinutes(30));
                try { await p.WaitForExitAsync(limit.Token); }
                catch (OperationCanceledException) { try { p.Kill(true); } catch (InvalidOperationException) { } return new Dlss5Run(false, "Stopped: it took too long."); }

                string text = (await ReadAsync(outFile) + await ReadAsync(errFile)).Trim();
                foreach (string line in text.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)) say(line);
                Log.Information("dlss5: {Mode} on {Dir} exited {Code}\n{Text}", mode, gameDir, p.ExitCode, text);
                return new Dlss5Run(p.ExitCode == 0, text.Length > 0 ? text : p.ExitCode == 0 ? "" : $"The tool stopped with code {p.ExitCode} and printed nothing.");
            }
            catch (Exception ex) when (ex is InvalidOperationException or System.ComponentModel.Win32Exception or IOException or UnauthorizedAccessException)
            {
                Log.Warning(ex, "dlss5: the tool could not be run");
                return new Dlss5Run(false, "The tool could not be started: " + ex.Message);
            }
            finally { try { Directory.Delete(work, true); } catch (IOException) { } catch (UnauthorizedAccessException) { } }
        }

        private static async Task<string> ReadAsync(string file)
        {
            try { return File.Exists(file) ? await File.ReadAllTextAsync(file) : ""; }
            catch (IOException) { return ""; }
        }

        /// <summary>Puts DLSS 5 into a game's folder (the tool checks the game and the card itself before it writes anything), and records it.</summary>
        public static async Task<Dlss5Run> InstallAsync(string gameId, string gameDir, Action<string> say, CancellationToken ct)
        {
            var (exe, why) = await EnsureToolAsync(say, ct);
            if (exe is null) return new Dlss5Run(false, why);
            var done = await RunAsync(exe, gameDir, Dlss5Mode.Install, say, ct);
            if (done.Ok)
            {
                Directory.CreateDirectory(Root);
                File.WriteAllText(MarkerOf(gameId), $"{gameDir}\n{DateTime.UtcNow:O}\n");
                if (Dlss5.FindOptiScalerIni(gameDir) is string ini)
                {
                    Dlss5.EnsureShortcutKeyDisabled(ini);
                }
            }
            return done;
        }

        /// <summary>Takes out everything the tool wrote (it put the game's own files back), and the record.</summary>
        public static async Task<Dlss5Run> RemoveAsync(string gameId, string gameDir, Action<string> say, CancellationToken ct)
        {
            var (exe, why) = await EnsureToolAsync(say, ct);
            if (exe is null)
            {
                try { File.Delete(MarkerOf(gameId)); } catch { }
                CleanOptiScalerFiles(gameDir);
                return new Dlss5Run(true, "OptiScaler removed.");
            }
            say("Taking DLSS 5 out...");
            var done = await RunAsync(exe, gameDir, Dlss5Mode.Remove, say, ct);
            if (done.Ok)
            {
                try { File.Delete(MarkerOf(gameId)); } catch (IOException) { }
                CleanOptiScalerFiles(gameDir);
            }
            return done;
        }

        private static void CleanOptiScalerFiles(string gameDir)
        {
            try
            {
                if (Dlss5.FindOptiScalerIni(gameDir) is string ini && File.Exists(ini))
                {
                    File.Delete(ini);
                    Log.Information("dlss5: deleted {File}", ini);
                }
            }
            catch (Exception ex) { Log.Debug(ex, "dlss5: could not delete OptiScaler.ini"); }
        }
    }
}
