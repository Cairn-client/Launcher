// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// The setup checklist window ("Before you play", "Check setup"): what it shows and when it opens. The steps themselves are the game's pack's
    /// (Home.PackSetup.cs: the pack's program lists them and does them). The one step the launcher has of its own is the mod: a game's pack is
    /// downloaded before it can say anything, and a pack too old to run its setup is replaced by downloading the newest.
    ///
    /// Nothing is shown at start. The checklist opens when the player presses Host or Join and something is missing (checked again every time),
    /// or from Settings > Check setup; while it is open it is read again every two seconds.
    /// </summary>
    public partial class Home
    {
        private bool showSetup;
        private bool setupReady;
        private bool setupBusy;
        private bool setupChecked;   // the first look at the PC has been taken
        private bool setupSystemOnly;
        private bool setupOptionalShown;
        private Task? setupFirstCheck;
        private CancellationTokenSource? setupPollCts;
        private readonly List<string> setupLines = new();

        // the mod's own download, shown as a step while there is no (usable) pack to ask
        private bool setupPackDownloading;
        private double? setupPackProgress;
        private string? setupPackFailure;

        private const int SetupLineCount = 6;
        private static readonly TimeSpan SetupPollPeriod = TimeSpan.FromSeconds(2);
        private const string DownloadPackAction = "download-pack";

        /// <summary>From OnInitializedAsync: starts the first look without delaying the launcher's normal screen.</summary>
        private void StartSetupCheck()
        {
            setupFirstCheck = Task.Run(async () =>
            {
                var sw = System.Diagnostics.Stopwatch.StartNew();
                await InvokeAsync(async () =>
                {
                    await RefreshPackSetupAsync();
                    // the regular game's facial data is linked by itself at start (hard links: no prompt, no copy)
                    if (packSetup is not null)
                    {
                        await RunPackAutoActionsAsync(onlyFacials: true);
                        await RefreshPackSetupAsync();
                    }
                    setupChecked = true;
                    Log.Information("setup: checked in {Ms} ms: {State}", sw.ElapsedMilliseconds, setupReady ? "ready, nothing to do" : "not ready");
                    StateHasChanged();
                });
            });
        }

        private void StartSetupPoll()
        {
            if (setupPollCts is { IsCancellationRequested: false }) return;
            var cts = setupPollCts = new CancellationTokenSource();
            _ = Task.Run(async () =>
            {
                while (!cts.IsCancellationRequested)
                {
                    try
                    {
                        if (!setupBusy)
                        {
                            await InvokeAsync(async () =>
                            {
                                if (await RefreshPackSetupAsync()) await RunPackAutoActionsAsync();
                                StateHasChanged();
                            });
                            // Ready and the checklist closed: nothing left to watch.
                            if (!showSetup && (setupReady || PackSetupProgram() is null)) break;
                        }
                    }
                    catch (Exception ex) { Log.Warning(ex, "setup: a background check failed"); }
                    try { await Task.Delay(SetupPollPeriod, cts.Token); } catch (OperationCanceledException) { break; }
                }
            });
        }

        private async Task HandleSetupActionAsync(string actionName)
        {
            Log.Information("setup: the player clicked {Action}", actionName);
            if (actionName == DownloadPackAction) { await DownloadPackAsync(ActiveGameId); await CheckSetupAgainAsync(); return; }
            if (packSetup?.Steps.FirstOrDefault(x => x.Action == actionName) is { } step) await RunPackActionAsync(step.Id, actionName);
        }

        private async Task CheckSetupAgainAsync()
        {
            await RefreshPackSetupAsync();
            setupChecked = true;
            if (!setupReady) StartSetupPoll();
            StateHasChanged();
        }

        /// <summary>"Check setup" in the status bar: the same checklist, any time.</summary>
        private Task OpenSetupChecklist() => OpenChecklistAsync(systemOnly: true);

        private async Task OpenChecklistAsync(bool systemOnly)
        {
            setupSystemOnly = systemOnly;
            Log.Information("setup: the player opened Check setup");
            showSetup = true;
            await CheckSetupAgainAsync();
            StartSetupPoll();
        }

        /// <summary>The game page's "Setup needed" chip: the whole list, game steps included.</summary>
        private Task OpenGameSetupChecklist() => OpenChecklistAsync(systemOnly: false);

        private void CloseSetupChecklist()
        {
            showSetup = false;
            if (setupReady) setupPollCts?.Cancel();
        }

        /// <summary>Host and Join unlock only when every step is done. A locked click opens the checklist, which says why.</summary>
        private async Task<bool> EnsureSetupReadyAsync()
        {
            if (setupFirstCheck is not null) await setupFirstCheck;
            // checked again at every Host and Join (it takes a moment, and the PC may have changed since the last one:
            // the game updated, a file removed, Steam closed), never trusted from an earlier check.
            await CheckSetupAgainAsync();
            if (!setupReady)
            {
                Log.Information("setup: Host/Join asked for before setup is ready; showing the checklist");
                setupSystemOnly = false;
                showSetup = true;
                StartSetupPoll();
                StateHasChanged();
                return false;
            }
            if (OptionalNeedsYouNow() && !setupOptionalShown)
            {
                // ready, but an optional step (the C++ runtimes) asks for the player: shown once, the next click goes on.
                Log.Information("setup: ready; an optional step needs the player, showing the checklist once");
                setupSystemOnly = false;
                setupOptionalShown = true;
                showSetup = true;
                StateHasChanged();
                return false;
            }
            return true;
        }

        // ------------------------------------------------------------ the mod's download as a step

        private void SetPackDownloadActivity(bool downloading, double? progress, string? failure)
        {
            setupPackDownloading = downloading;
            setupPackProgress = progress;
            setupPackFailure = failure;
        }

        /// <summary>The step shown while the game's pack cannot run its setup: not downloaded yet, or too old for this launcher.</summary>
        private IReadOnlyList<PackStep> ModPackSteps()
        {
            bool have = PackStore.IsDownloaded(PacksRoot, ActiveGameId);
            string detail = have
                ? "The downloaded mod is too old for this launcher: it cannot say what your PC still needs. Download the newest mod."
                : $"The {ActiveGameName} mod is not downloaded yet. It is a small download from the project's release page.";
            if (setupPackDownloading)
                return new[] { new PackStep("mod-pack", $"{ActiveGameName} mod downloaded", PackStepStatus.Working,
                    "Downloading the mod" + (setupPackProgress is double p ? $" ({p * 100:0}%)" : "") + "...", Progress: setupPackProgress, System: true) };
            return new[] { new PackStep("mod-pack", $"{ActiveGameName} mod downloaded", PackStepStatus.NeedsYou,
                (setupPackFailure is null ? "" : setupPackFailure + " ") + detail, DownloadPackAction, setupPackFailure is null ? "Download the mod" : "Try again", System: true) };
        }

        // ------------------------------------------------------------ helpers

        private void AddSetupLine(string line)
        {
            setupLines.Add(line);
            while (setupLines.Count > SetupLineCount) setupLines.RemoveAt(0);
            StateHasChanged();
        }

        /// <summary>
        /// A settings.json without a usable Modding Tools path (a new player, an unzipped build, a game that moved) takes the one the pack's setup found,
        /// instead of opening Settings with an error. Written only where the file has no game path (never set) and then as that one key; a path the player set
        /// that does not work right now (a moved game, a drive not plugged in) is replaced for this run only: the file keeps the player's path.
        /// </summary>
        private void AdoptDetectedGamePath(string? exe)
        {
            if (exe is null || !File.Exists(exe) || !GameExeIsSetting(ActiveGameId)) return;
            if (IsModdingToolsBuild(settings.GamePath ?? "") && File.Exists(settings.GamePath)) return;
            settings.GamePath = exe;
            if (settingsStore.IsUnsetOnDisk(nameof(AppSettings.GamePath)))
            {
                if (WriteSettings("setup", nameof(AppSettings.GamePath)))
                    Log.Information("setup: game path set to the Modding Tools found in Steam");
            }
            else
            {
                settingsStore.Accept(settings, nameof(AppSettings.GamePath));
                Log.Information("setup: the saved game path is not the Modding Tools; using the one found in Steam for this run (settings.json keeps the saved path)");
            }
        }
    }
}
