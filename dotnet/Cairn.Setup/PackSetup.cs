// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Diagnostics;
using System.Text;
using System.Text.Json;

namespace Cairn.Setup;

public enum PackStepStatus { Done, Working, NeedsYou, Waiting }

/// <summary>
/// One line of a game's setup checklist, as the pack's program reported it. <c>Action</c> is a word the program understands (the launcher only passes it
/// back); <c>Auto</c> says the program wants the action started without a click: <c>once</c> per launcher session, or <c>while</c> the step is the next
/// thing to do. <c>System</c> marks a step that concerns the whole PC rather than this game (shown in "System setup").
/// </summary>
public sealed record PackStep(string Id, string Title, PackStepStatus Status, string Detail, string? Action = null, string? ActionLabel = null,
                              double? Progress = null, bool Optional = false, bool System = false, string? Auto = null);

public sealed record PackSetupReport(IReadOnlyList<PackStep> Steps, bool Ready, bool OptionalNeedsYou, string? GameExe = null, string? GameDir = null, string? GameExeNote = null);

public sealed record PackActionResult(bool Ok, string Message, int PendingSeconds = 0);

/// <summary>
/// A game's setup done by the pack's program (<c>pack.json</c> <c>"setup": true</c>, run as the contract's <c>optionsProgram</c>; docs/PACKS.md): <c>setup check</c>
/// prints one <c>{"step":{...}}</c> line per step then <c>{"done":true,"ready":...}</c>; <c>setup do ACTION</c> works and ends with <c>{"ok":...,"message":...}</c>.
/// The launcher draws the list and passes the actions back; what the steps are is the game's.
/// </summary>
public static class PackSetup
{
    public const int MaxSteps = 16;
    public static readonly TimeSpan CheckTimeout = TimeSpan.FromSeconds(40);
    public static readonly TimeSpan DoTimeout = TimeSpan.FromMinutes(20);

    /// <summary>A step line; null when it is not one this launcher understands (damaged, unknown status, a bad id).</summary>
    public static PackStep? ParseStep(string line)
    {
        try
        {
            using var doc = JsonDocument.Parse(line);
            if (doc.RootElement.ValueKind != JsonValueKind.Object || !doc.RootElement.TryGetProperty("step", out var s) || s.ValueKind != JsonValueKind.Object) return null;
            string Str(string n) => s.TryGetProperty(n, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() ?? "" : "";
            bool Flag(string n) => s.TryGetProperty(n, out var v) && v.ValueKind == JsonValueKind.True;
            string id = Str("id"), title = CatalogLimits.Text(Str("title"), 120), action = Str("action");
            PackStepStatus? status = Str("status") switch
            {
                "done" => PackStepStatus.Done, "working" => PackStepStatus.Working, "needs-you" => PackStepStatus.NeedsYou, "waiting" => PackStepStatus.Waiting, _ => null,
            };
            if (!CatalogLimits.IsId(id) || title.Length == 0 || status is null || (action.Length > 0 && !CatalogLimits.IsId(action))) return null;
            string auto = Str("auto");
            if (auto.Length > 0 && auto is not ("once" or "while")) return null;
            double? progress = s.TryGetProperty("progress", out var p) && p.ValueKind == JsonValueKind.Number && p.TryGetDouble(out var pv) ? Math.Clamp(pv, 0, 1) : null;
            string label = CatalogLimits.Text(Str("actionLabel"), 40);
            return new PackStep(id, title, status.Value, CatalogLimits.Text(Str("detail"), 700), action.Length > 0 ? action : null,
                                action.Length > 0 && label.Length > 0 ? label : null, progress, Flag("optional"), Flag("system"), auto.Length > 0 ? auto : null);
        }
        catch (JsonException) { return null; }
    }

    /// <summary>The checklist, or null when the program is not there or did not give one.</summary>
    public static async Task<PackSetupReport?> CheckAsync(string program, string? gameExe, CancellationToken ct = default, TimeSpan? timeout = null)
    {
        var steps = new List<PackStep>();
        bool done = false, ready = false, optionalNeedsYou = false;
        string? mtExe = null, gameDir = null, exeNote = null;
        var run = await RunAsync(program, Args("check", null, gameExe), timeout ?? CheckTimeout, line =>
        {
            if (steps.Count < MaxSteps && ParseStep(line) is { } step && steps.All(x => x.Id != step.Id)) { steps.Add(step); return; }
            try
            {
                using var doc = JsonDocument.Parse(line);
                var r = doc.RootElement;
                if (r.ValueKind != JsonValueKind.Object) return;
                if (r.TryGetProperty("done", out var d) && d.ValueKind == JsonValueKind.True)
                {
                    done = true;
                    ready = r.TryGetProperty("ready", out var rd) && rd.ValueKind == JsonValueKind.True;
                    optionalNeedsYou = r.TryGetProperty("optionalNeedsYou", out var on) && on.ValueKind == JsonValueKind.True;
                }
                else if (r.TryGetProperty("gameExeNote", out var gn) && gn.ValueKind == JsonValueKind.String)
                    exeNote = CatalogLimits.Text(gn.GetString() ?? "", 400);
                else if (r.TryGetProperty("found", out var f) && f.ValueKind == JsonValueKind.Object && (f.TryGetProperty("gameExe", out var e) || f.TryGetProperty("moddingToolsExe", out e)) && e.ValueKind == JsonValueKind.String)
                {
                    mtExe = e.GetString();
                    if (f.TryGetProperty("gameDir", out var gd) && gd.ValueKind == JsonValueKind.String) gameDir = gd.GetString();
                }
            }
            catch (JsonException) { }
        }, ct).ConfigureAwait(false);
        if (run.TimedOut || !done || steps.Count == 0) return null;
        // the launcher decides readiness itself from the steps: a program cannot say "ready" over a step that is not done
        bool really = steps.All(x => x.Optional || x.Status == PackStepStatus.Done);
        return new PackSetupReport(steps, ready && really, optionalNeedsYou && really, mtExe, gameDir, string.IsNullOrEmpty(exeNote) ? null : exeNote);
    }

    /// <summary>Does one action the program named in a step. <paramref name="say"/> gets the program's progress lines, <paramref name="progress"/> its 0..1 numbers.</summary>
    public static async Task<PackActionResult> DoAsync(string program, string action, string? gameExe, Action<string> say, Action<double> progress,
                                                       CancellationToken ct = default, TimeSpan? timeout = null)
    {
        if (!CatalogLimits.IsId(action)) return new PackActionResult(false, "Not a valid action.");
        string? last = null;
        var run = await RunAsync(program, Args("do", action, gameExe), timeout ?? DoTimeout, line =>
        {
            try
            {
                using var doc = JsonDocument.Parse(line);
                var r = doc.RootElement;
                if (r.ValueKind != JsonValueKind.Object) return;
                if (r.TryGetProperty("say", out var s) && s.ValueKind == JsonValueKind.String && s.GetString() is { Length: > 0 } text) say(CatalogLimits.Text(text, 300));
                if (r.TryGetProperty("progress", out var p) && p.ValueKind == JsonValueKind.Number && p.TryGetDouble(out var pv)) progress(Math.Clamp(pv, 0, 1));
                if (r.TryGetProperty("ok", out _)) last = line;
            }
            catch (JsonException) { }
        }, ct).ConfigureAwait(false);
        if (run.TimedOut) return new PackActionResult(false, "It took too long and was stopped.");
        if (last is not null)
        {
            try
            {
                using var doc = JsonDocument.Parse(last);
                var r = doc.RootElement;
                bool ok = r.TryGetProperty("ok", out var o) && o.ValueKind == JsonValueKind.True;
                string message = r.TryGetProperty("message", out var m) && m.ValueKind == JsonValueKind.String ? CatalogLimits.Text(m.GetString() ?? "", 600) : "";
                int pending = r.TryGetProperty("pending", out var pe) && pe.ValueKind == JsonValueKind.Number && pe.TryGetInt32(out var pn) ? Math.Clamp(pn, 0, 300) : 0;
                return new PackActionResult(ok, message, pending);
            }
            catch (JsonException) { }
        }
        return new PackActionResult(false, run.StartError ?? "The program did not answer.");
    }

    private static IReadOnlyList<string> Args(string verb, string? action, string? gameExe)
    {
        var a = new List<string> { "setup", verb };
        if (action is not null) a.Add(action);
        if (!string.IsNullOrWhiteSpace(gameExe) && gameExe.Length <= 260 && !gameExe.Any(char.IsControl)) { a.Add("--game-exe"); a.Add(gameExe); }
        return a;
    }

    private sealed record Run(bool TimedOut, string? StartError);

    // the program's arguments are a list, never a command line to be split; its output is read as lines and it is stopped at the timeout
    private static async Task<Run> RunAsync(string program, IReadOnlyList<string> args, TimeSpan timeout, Action<string> onLine, CancellationToken ct)
    {
        if (!File.Exists(program)) return new Run(false, "The program of the mod is not there.");
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
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { return new Run(false, "The program of the mod did not start (" + ex.Message + ")."); }
        if (p is null) return new Run(false, "The program of the mod did not start.");
        using (p)
        {
            using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
            cts.CancelAfter(timeout);
            try
            {
                string? line;
                while ((line = await p.StandardOutput.ReadLineAsync(cts.Token).ConfigureAwait(false)) is not null)
                    if (line.Length is > 0 and <= 4096) onLine(line);
                await p.WaitForExitAsync(cts.Token).ConfigureAwait(false);
                return new Run(false, null);
            }
            catch (OperationCanceledException)
            {
                try { if (!p.HasExited) p.Kill(entireProcessTree: true); } catch (Exception ex) when (ex is InvalidOperationException or System.ComponentModel.Win32Exception) { }
                if (ct.IsCancellationRequested) throw;
                return new Run(true, null);
            }
        }
    }
}
