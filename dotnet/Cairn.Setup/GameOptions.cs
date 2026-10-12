// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Diagnostics;
using System.Text;
using System.Text.Json;

namespace Cairn.Setup;

/// <summary>
/// A toggle a game offers on its settings page (DLSS 5 for Kingdom Come II is one): listed in the game's <c>game.json</c> (<c>options</c>), done by the
/// pack's <c>optionsProgram</c>. <c>Requires</c> names a PC need the launcher can see (<c>nvidia-rtx</c>), or is empty.
/// </summary>
public sealed record GameOption(string Id, string Title, string Hint, string Requires = "");

/// <summary>What the pack's program says about one option on this PC.</summary>
public sealed record OptionStatus(bool Available, bool On, string Note);

/// <summary>How a run of the pack's program went: <c>Ok</c>, and what to tell the player (its last words, or why it did not run).</summary>
public sealed record OptionRun(bool Ok, string Message);

/// <summary>
/// Talks to a pack's options program (<c>pack.json</c>'s <c>optionsProgram</c>), a program the launcher runs and never builds a command for beyond these two:
/// <c>&lt;program&gt; options check &lt;id&gt; --game-dir &lt;dir&gt;</c> prints one JSON line <c>{"available":true,"on":false,"note":"..."}</c>, and
/// <c>&lt;program&gt; options apply &lt;id&gt; on|off --game-dir &lt;dir&gt;</c> prints progress as <c>{"say":"..."}</c> lines and ends with <c>{"ok":true,"message":"..."}</c>.
/// Ids are plain, the directory is the game's, nothing else reaches the program's command line.
/// </summary>
public static class OptionRunner
{
    public const int MaxOptions = 8;
    public static readonly TimeSpan CheckTimeout = TimeSpan.FromSeconds(30);
    public static readonly TimeSpan ApplyTimeout = TimeSpan.FromMinutes(15);

    public static readonly IReadOnlySet<string> Requirements = new HashSet<string>(StringComparer.Ordinal) { "", "nvidia-rtx" };

    internal static List<GameOption>? Parse(JsonElement root)
    {
        var list = new List<GameOption>();
        foreach (var e in GameList.Items(root, "options"))
        {
            if (e.ValueKind != JsonValueKind.Object || list.Count >= MaxOptions) return null;
            string id = GameList.Str(e, "id"), requires = GameList.Str(e, "requires"), title = CatalogLimits.Text(GameList.Str(e, "title"), 60);
            if (!CatalogLimits.IsId(id) || title.Length == 0 || !Requirements.Contains(requires) || list.Any(o => o.Id == id)) return null;
            list.Add(new GameOption(id, title, CatalogLimits.Text(GameList.Str(e, "hint"), 300), requires));
        }
        return list;
    }

    public static async Task<OptionStatus?> CheckAsync(string program, string optionId, string gameDir, CancellationToken ct = default, TimeSpan? timeout = null)
    {
        var run = await RunAsync(program, new[] { "options", "check", optionId, "--game-dir", gameDir }, timeout ?? CheckTimeout, _ => { }, ct).ConfigureAwait(false);
        if (run.Last is null) return null;
        try
        {
            using var doc = JsonDocument.Parse(run.Last);
            var r = doc.RootElement;
            bool Flag(string n) => r.TryGetProperty(n, out var v) && v.ValueKind == JsonValueKind.True;
            string note = r.TryGetProperty("note", out var nv) && nv.ValueKind == JsonValueKind.String ? CatalogLimits.Text(nv.GetString() ?? "", 300) : "";
            return new OptionStatus(Flag("available"), Flag("on"), note);
        }
        catch (JsonException) { return null; }
    }

    public static async Task<OptionRun> ApplyAsync(string program, string optionId, bool on, string gameDir, Action<string> say, CancellationToken ct = default, TimeSpan? timeout = null)
    {
        var run = await RunAsync(program, new[] { "options", "apply", optionId, on ? "on" : "off", "--game-dir", gameDir }, timeout ?? ApplyTimeout, say, ct).ConfigureAwait(false);
        if (run.TimedOut) return new OptionRun(false, "It took too long and was stopped.");
        if (run.Last is not null)
        {
            try
            {
                using var doc = JsonDocument.Parse(run.Last);
                var r = doc.RootElement;
                if (r.TryGetProperty("ok", out var ok) && ok.ValueKind is JsonValueKind.True or JsonValueKind.False)
                    return new OptionRun(ok.ValueKind == JsonValueKind.True, r.TryGetProperty("message", out var m) && m.ValueKind == JsonValueKind.String ? CatalogLimits.Text(m.GetString() ?? "", 600) : "");
            }
            catch (JsonException) { }
        }
        return new OptionRun(false, run.StartError ?? "The program did not answer.");
    }

    internal sealed record Run(string? Last, bool TimedOut, string? StartError, IReadOnlyList<string>? Lines = null);

    // starts the program with its arguments as a list (never a command line to be split), reads its output as lines, stops it at the timeout
    internal static async Task<Run> RunAsync(string program, IReadOnlyList<string> args, TimeSpan timeout, Action<string> say, CancellationToken ct)
    {
        if (!File.Exists(program)) return new Run(null, false, "The program of the mod is not there.");
        if (args.Skip(2).Take(1).Any(a => !CatalogLimits.IsId(a))) return new Run(null, false, "Not a valid option.");
        var psi = new ProcessStartInfo(program)
        {
            WorkingDirectory = Path.GetDirectoryName(program)!,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            StandardOutputEncoding = Encoding.UTF8,
        };
        foreach (var a in args) psi.ArgumentList.Add(a);
        Process? p;
        try { p = Process.Start(psi); }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { return new Run(null, false, "The program of the mod did not start (" + ex.Message + ").") ; }
        if (p is null) return new Run(null, false, "The program of the mod did not start.");
        using (p)
        {
            string? last = null;
            var all = new List<string>();
            using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
            cts.CancelAfter(timeout);
            try
            {
                string? line;
                while ((line = await p.StandardOutput.ReadLineAsync(cts.Token).ConfigureAwait(false)) is not null)
                {
                    if (line.Length is 0 or > 4096) continue;
                    last = line;
                    if (all.Count < 50) all.Add(line);
                    try
                    {
                        using var doc = JsonDocument.Parse(line);
                        if (doc.RootElement.ValueKind == JsonValueKind.Object && doc.RootElement.TryGetProperty("say", out var s) && s.ValueKind == JsonValueKind.String)
                            say(CatalogLimits.Text(s.GetString() ?? "", 300));
                    }
                    catch (JsonException) { }
                }
                await p.WaitForExitAsync(cts.Token).ConfigureAwait(false);
                return new Run(last, false, null, all);
            }
            catch (OperationCanceledException)
            {
                try { if (!p.HasExited) p.Kill(entireProcessTree: true); } catch (Exception ex) when (ex is InvalidOperationException or System.ComponentModel.Win32Exception) { }
                if (ct.IsCancellationRequested) throw;
                return new Run(last, true, null, all);
            }
        }
    }
}
