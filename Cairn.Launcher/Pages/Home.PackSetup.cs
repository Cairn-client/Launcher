// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// A game's setup checklist from the game's pack (docs/PACKS.md, <c>"setup": true</c>): the pack's program lists the steps and does them, this file only
    /// draws the list, passes the actions back and starts the ones the program asks to have started. A pack that does not have it (an older mod, or none
    /// downloaded yet) keeps the checklist the launcher always had (Home.FirstRun.cs), which is used whenever <see cref="packSetup"/> is null.
    /// </summary>
    public partial class Home
    {
        /// <summary>The pack's checklist as of the last check; null when the game's pack does not provide one (the launcher's own checklist is used then).</summary>
        private PackSetupReport? packSetup;
        private string packSetupGameId = "";   // the game packSetup is about
        private readonly Dictionary<string, string> packExeNotes = new();   // what each game's pack said about the executable it was given ("" = nothing wrong)
        private readonly Dictionary<string, string> packGameDirs = new();   // the folder each game's pack said its options belong in (setup check, found.gameDir)
        private bool packProgramSilent;   // the pack has a setup program that did not answer the last check (not the same as a pack too old to have one)
        private (string StepId, string Action)? packBusy;
        private string? packBusyLine;
        private double? packBusyProgress;
        private readonly Dictionary<string, string> packNotes = new();       // step -> what a finished action said (shown instead of the step's detail)
        private readonly Dictionary<string, string> packFailures = new();    // step -> why the last try failed
        private readonly Dictionary<string, DateTime> packPendingUntil = new();   // step -> an action was started and the PC needs a moment (Steam is starting)
        private readonly HashSet<string> packAutoRan = new();
        private readonly Dictionary<string, string> packLogged = new();

        /// <summary>The program of the setup game's pack when the pack does the setup; null otherwise.</summary>
        private string? PackSetupProgram(string? gameId = null) => ProgramCached(ref setupProgramCache, gameId ?? ActiveGameId, p => p is { Setup: true });

        /// <summary>Asks the pack's program for its checklist; false (and the launcher's own checklist is used) when the pack has none or it did not answer.</summary>
        private async Task<bool> RefreshPackSetupAsync()
        {
            setupProgramCache = default; sessionProgramCache = default;   // the pack may have changed since the last look
            string gameId = ActiveGameId;
            if (packSetupGameId != gameId)
            {
                // another game: nothing of the last one's list, notes or failures applies
                packSetup = null; packSetupGameId = gameId; setupReady = false;
                packNotes.Clear(); packFailures.Clear(); packPendingUntil.Clear(); packAutoRan.Clear(); packLogged.Clear();
            }
            var program = PackSetupProgram(gameId);
            if (program is null) { packSetup = null; setupReady = false; return false; }
            string? mt = GameExeOf(gameId) is { } known && System.IO.File.Exists(known) ? known : null;
            PackSetupReport? report = null;
            // the first run of a program right after an update is often slow (the new files are being scanned): one more try before giving up
            for (int attempt = 0; attempt < 2 && report is null; attempt++)
            {
                if (attempt > 0) await Task.Delay(1500);
                try { report = await PackSetup.CheckAsync(program, mt); }
                catch (Exception ex) { Log.Warning(ex, "setup: the pack's checklist failed"); }
            }
            if (report is null)
            {
                // the last good list stays on screen (an action that places files makes the program busy for a while)
                Log.Warning("setup: the pack's program gave no checklist");
                packProgramSilent = true;
                return false;
            }
            packProgramSilent = false;
            packSetup = report;
            if (!string.IsNullOrWhiteSpace(report.GameDir)) packGameDirs[gameId] = report.GameDir;
            packExeNotes[gameId] = report.GameExeNote ?? "";
            AdoptDetectedGamePath(report.GameExe);
            foreach (var done in report.Steps.Where(x => x.Status == PackStepStatus.Done).Select(x => x.Id).ToList())
            {
                packNotes.Remove(done); packFailures.Remove(done); packPendingUntil.Remove(done);
            }
            bool was = setupReady;
            setupReady = report.Ready;
            LogPackStepChanges(report);
            if (setupReady && !was && packLogged.Count > 0) Log.Information("setup: Ready! (the pack's checklist) Host and Join unlocked");
            return true;
        }

        private void LogPackStepChanges(PackSetupReport report)
        {
            foreach (var s in report.Steps)
            {
                string key = $"{s.Status}|{s.Action}|{(s.Progress is double p ? ((int)(p * 10)).ToString() : "")}";
                if (packLogged.TryGetValue(s.Id, out var last) && last == key) continue;
                packLogged[s.Id] = key;
                Log.Information("setup: step {Step} {Title}: {Status} -- {Detail}", s.Id, s.Title, s.Status, Redact.Text(s.Detail));
            }
        }

        /// <summary>What the checklist window shows: the pack's steps with what the launcher knows on top (a try that failed, an action that is working), or the launcher's own steps.</summary>
        private IReadOnlyList<PackStep> SetupView()
        {
            if (packSetup is null) return ModPackSteps();
            var now = DateTime.UtcNow;
            return packSetup.Steps.Select(s =>
            {
                if (s.Status == PackStepStatus.Done) return s;
                if (packBusy is { } b && b.StepId == s.Id)
                    return s with { Status = PackStepStatus.Working, Detail = packBusyLine ?? s.Detail, Progress = packBusyProgress ?? s.Progress, Action = null, ActionLabel = null, Auto = null };
                if (packFailures.TryGetValue(s.Id, out var why))
                    return s with { Status = PackStepStatus.NeedsYou, Detail = why, Action = s.Action, ActionLabel = s.Action is null ? null : "Try again", Auto = null };
                if (packPendingUntil.TryGetValue(s.Id, out var until) && until > now && packNotes.TryGetValue(s.Id, out var starting))
                    return s with { Status = PackStepStatus.Working, Detail = starting, Action = null, ActionLabel = null, Auto = null };
                if (packNotes.TryGetValue(s.Id, out var note))
                    return s with { Detail = note, ActionLabel = s.Action is null ? null : "Open it again" };
                return s;
            }).ToList();
        }

        private bool PackStepsAllDone => packSetup is not null && packSetup.Steps.All(s => s.Status == PackStepStatus.Done);
        private bool PackSystemGood => packSetup is not null && packSetup.Steps.Where(s => s.Id == "steam").All(s => s.Status == PackStepStatus.Done);
        private bool OptionalNeedsYouNow() => packSetup is { OptionalNeedsYou: true };

        // ------------------------------------------------------------ doing what a step asks

        private async Task RunPackActionAsync(string stepId, string action)
        {
            setupProgramCache = default; sessionProgramCache = default;   // the pack may have changed since the last look
            var program = PackSetupProgram();
            if (program is null || setupBusy) return;
            string? mt = GameExeOf(ActiveGameId) is { } known && System.IO.File.Exists(known) ? known : null;
            setupBusy = true;
            packBusy = (stepId, action);
            packBusyLine = null; packBusyProgress = null;
            packFailures.Remove(stepId);
            setupLines.Clear();
            StateHasChanged();
            Log.Information("setup: the pack's action {Action} ({Step})", action, stepId);

            PackActionResult result;
            try
            {
                result = await PackSetup.DoAsync(program, action, mt,
                    say => _ = InvokeAsync(() => { packBusyLine = say; AddSetupLine(Redact.Text(say)); }),
                    p => _ = InvokeAsync(() => { packBusyProgress = p; StateHasChanged(); }));
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "setup: the pack's action {Action} failed", action);
                result = new PackActionResult(false, "That did not work. Click Try again; if it keeps failing, send the logs with Report a bug.");
            }
            Log.Information("setup: the pack's action {Action} {Ok}: {Message}", action, result.Ok ? "done" : "FAILED", Redact.Text(result.Message));
            if (!result.Ok) packFailures[stepId] = result.Message.Length > 0 ? result.Message : "That did not work. Click Try again.";
            else if (result.Message.Length > 0)
            {
                packNotes[stepId] = result.Message;
                if (result.PendingSeconds > 0) packPendingUntil[stepId] = DateTime.UtcNow.AddSeconds(result.PendingSeconds);
            }
            packBusy = null; packBusyLine = null; packBusyProgress = null;
            setupBusy = false;
            await CheckSetupAgainAsync();
        }

        /// <summary>
        /// The steps the program asks to have started by themselves (<c>auto</c>): <c>once</c> a session for what opens a window or starts a program, <c>while</c> the step
        /// is the next thing to do for the work itself. A step whose last try failed waits for the player's click.
        /// </summary>
        private async Task RunPackAutoActionsAsync(bool onlyFacials = false)
        {
            if (packSetup is null || setupBusy) return;
            foreach (var s in packSetup.Steps)
            {
                if (s.Auto is null || s.Action is null || packFailures.ContainsKey(s.Id)) continue;
                if (onlyFacials && s.Action != "link-facials") continue;
                if (s.Auto == "once" && !packAutoRan.Add(s.Action)) continue;
                await RunPackActionAsync(s.Id, s.Action);
                return;
            }
        }
    }
}
