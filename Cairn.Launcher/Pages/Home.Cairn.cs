// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
using System.Diagnostics;
using System.IO;
using System.Text.Json;
using System.Windows.Forms;
using Cairn.Launcher.Components.Shared;
using Serilog;

namespace Cairn.Launcher.Pages
{
    public partial class Home
    {
        /// <summary>
        /// Cairn: Settings > Uninstall. Runs Setup's own uninstaller beside the launcher (the same one Windows' installed apps
        /// runs) with /SILENT /REMOVEMOD=1: the mod leaves the game's Mods folder, the launcher and its files go, and the mod's own
        /// data (characters, chests, start save) is copied to Documents\Cairn-kept first. The game's own saves are never touched.
        /// The launcher closes so that its files can be removed.
        /// </summary>
        // ---- start saves ------------------------------------------------------------------------------------------------
        // A joiner's new character (and a host's New adventure) begin from a start save placed in start-save\<playstyle>\ beside the
        // launcher. They are not shipped (they are somebody's play data), so a player makes one from his own Modding Tools game:
        // the agent's own checker (CairnClient --start_game validate) reads the file, the playstyle it fits is found, and the file goes
        // where it is expected, replacing a save he put there before.
        private static string StartSaveRoot => Path.Combine(AppContext.BaseDirectory, "start-save");

        private void OpenStartSaveFolder()
        {
            try
            {
                Directory.CreateDirectory(StartSaveRoot);
                Process.Start(new ProcessStartInfo("explorer.exe", "\"" + StartSaveRoot + "\"") { UseShellExecute = true })?.Dispose();
            }
            catch (Exception ex) { Log.Warning(ex, "start save: folder not opened"); }
        }

        private static string? PickSaveFile()
        {
            string? picked = null;
            var t = new Thread(() =>
            {
                using var d = new OpenFileDialog
                {
                    Title = "Pick a save of your Modding Tools game, played past the prologue",
                    Filter = "Saves (*.whs)|*.whs|All files (*.*)|*.*",
                };
                string saves = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Saved Games", "kingdomcome2", "saves");
                if (Directory.Exists(saves)) d.InitialDirectory = saves;
                if (d.ShowDialog() == DialogResult.OK) picked = d.FileName;
            });
            t.SetApartmentState(ApartmentState.STA);
            t.Start();
            t.Join();
            return picked;
        }

        private static async Task<JsonElement?> ValidateStartSaveAsync(string agent, string file, string style)
        {
            var psi = new ProcessStartInfo { FileName = agent, UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, WorkingDirectory = Path.GetDirectoryName(agent) ?? "" };
            foreach (var a in new[] { "--start_game", "validate", "--file", file, "--style", style }) psi.ArgumentList.Add(a);
            using var p = Process.Start(psi);
            if (p is null) return null;
            using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(60));
            string output = await p.StandardOutput.ReadToEndAsync(cts.Token);
            await p.WaitForExitAsync(cts.Token);
            string? line = output.Split('\n').Select(l => l.Trim()).FirstOrDefault(l => l.StartsWith("StartGame ", StringComparison.Ordinal));
            return line is null ? null : JsonDocument.Parse(line[5..]).RootElement.Clone();
        }

        private static bool CheckPassed(JsonElement result, string name) =>
            result.TryGetProperty("checks", out var cs) && cs.EnumerateArray().Any(c => c.GetProperty("name").GetString() == name && c.GetProperty("pass").GetBoolean());

        private async Task AddStartSaveAsync()
        {
            try
            {
                string? file = PickSaveFile();
                if (file is null) return;
                string agent = ResolveAgainstLauncher(settings.AgentPath);
                if (!File.Exists(agent)) { ShowMessage("START SAVE", "The agent (CairnClient.exe) is not beside the launcher, so the save cannot be checked.", "Run Setup again."); return; }

                string? style = null;
                JsonElement? result = null;
                foreach (var st in new[] { "adviser", "soldier", "scout" })
                {
                    var j = await ValidateStartSaveAsync(agent, file, st);
                    if (j is null) continue;
                    result ??= j;
                    if (CheckPassed(j.Value, $"the {st} playstyle")) { style = st; result = j; break; }
                }
                if (result is null) { ShowMessage("START SAVE", "The save could not be read.", "Pick a save of your Modding Tools game."); return; }

                // what must hold for a Henry to be taken from it; the rest only warns
                string[] must = { "reads and verifies", "Modding Tools build", "after the prologue, as Henry", "a playthrough seed" };
                var failed = must.Where(n => !CheckPassed(result.Value, n)).ToList();
                if (failed.Count > 0)
                {
                    string why = string.Join("; ", result.Value.GetProperty("checks").EnumerateArray()
                        .Where(c => !c.GetProperty("pass").GetBoolean() && must.Contains(c.GetProperty("name").GetString()))
                        .Select(c => c.GetProperty("name").GetString() + (c.GetProperty("detail").GetString() is { Length: > 0 } d ? " (" + d + ")" : "")));
                    ShowMessage("START SAVE", "This save cannot be used: " + why + ".", "Pick a save made in the Modding Tools build, as Henry, after the prologue.");
                    return;
                }

                bool guessed = style is null;
                style ??= "adviser";
                string dir = Path.Combine(StartSaveRoot, style);
                Directory.CreateDirectory(dir);
                foreach (var old in Directory.GetFiles(dir, "*.whs")) File.Delete(old);
                File.Copy(file, Path.Combine(dir, Path.GetFileName(file)), overwrite: true);
                Log.Information("start save: {File} placed as the {Style} start save", file, style);

                var warn = result.Value.GetProperty("checks").EnumerateArray()
                    .Where(c => !c.GetProperty("pass").GetBoolean() && c.GetProperty("name").GetString() is "no partner / mod data in the world" or "where Hans and Henry part ways")   // the header names matter only for sharing a save
                    .Select(c => c.GetProperty("name").GetString()).ToList();
                ShowMessage("START SAVE ADDED",
                    $"{Path.GetFileName(file)} is now the {style} start save" + (guessed ? " (its playstyle could not be told, so it is filed as adviser)." : "."),
                    warn.Count == 0 ? "You can now join with a new character or start a new adventure."
                                    : "It works, but it is not a clean start (" + string.Join(", ", warn) + ").");
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "start save: not added");
                ShowMessage("START SAVE", "The start save could not be added: " + ex.Message, "");
            }
        }

        // ---- one-click update -------------------------------------------------------------------------------------------
        // A window opens by itself at start when a newer release is out, and Host / Join look again and ask before they start when one is. Both open the
        // update window: what is new, UPDATE NOW or LATER. UPDATE NOW downloads that release's installer from this repository's own
        // address (a progress bar in the same window), checks it against the SHA-256 GitHub lists for it, and runs it silently over this
        // install (same folder, the shortcut choices Setup remembered, the old shortcuts replaced); the new launcher opens when it is done.
        private UpdateInfo? updateInfo;
        private bool showUpdate, updateBusy;
        private string? declinedUpdateTag;   // the release the player said "later" to: Host and Join do not stop for that one again
        private string updateStage = "offer", updateStatus = "", updateReason = "";
        private double? updateProgress;

        // The version the player was last told about by a pop-up: each new release is announced once, whenever it is found.
        private string? announcedUpdateTag;

        /// <param name="announce">Open the update window by itself when a release the player has not been told about is out
        /// (at start).</param>
        private async Task CheckForUpdateAsync(bool announce = false)
        {
            var n = await UpdateCheck.CheckAsync(Globals.CairnVersion);
            updateInfo = n;
            await InvokeAsync(StateHasChanged);
            if (announce) await AnnounceUpdateAsync();
        }

        /// <summary>Opens the update window for a release not announced yet, unless the player is in the middle of something else (a launch,
        /// Settings, the setup checklist, another update): the button stays, and the next check tries again.</summary>
        private async Task AnnounceUpdateAsync()
        {
            var n = updateInfo;
            if (n is null || n.Tag == announcedUpdateTag || updateBusy || showUpdate) return;
            if (launchStage != LaunchStage.Idle || showSettings || showGameSettings || showSetup || showHostInfo || showJoinSteam) return;
            announcedUpdateTag = n.Tag;
            await InvokeAsync(() => OpenUpdateModal("A new version of Cairn Client is available. Both players need the same version to play together."));
        }


        // the button beside the version: ask GitHub now, then open the update window or say the launcher is current
        private bool updateChecking;

        // asks GitHub, then the window opens at once with the answer (an update to install, or "you have the newest version")
        private async Task CheckUpdateNowAsync()
        {
            if (updateChecking || updateBusy) return;
            updateChecking = true;
            declinedUpdateTag = null;
            await CheckForUpdateAsync();
            updateChecking = false;
            if (updateInfo is not null) { OpenUpdateModal(); return; }
            updateStage = lastCheckFailed ? "unreachable" : "current";
            showUpdate = true;
            await InvokeAsync(StateHasChanged);
        }

        private bool lastCheckFailed => UpdateCheck.LastCheckFailed;

        private void OpenUpdateModal(string reason = "")
        {
            if (updateInfo is null || updateBusy) return;
            updateStage = "offer"; updateReason = reason; updateStatus = ""; updateProgress = null;
            showUpdate = true;
            _ = InvokeAsync(StateHasChanged);
        }

        private void UpdateClick() => OpenUpdateModal();

        private void UpdateLater() { declinedUpdateTag = updateInfo?.Tag; showUpdate = false; }

        /// <summary>Host and Join: a newer version is checked for, and offered once. True = the window opened, the click goes no further.</summary>
        private async Task<bool> OfferUpdateBeforeAsync(string what)
        {
            if (updateBusy) return false;
            // a fresh look at GitHub at every Host and Join, not the answer from an earlier check; a release already put off with "later" is not offered again at each click, a newer one is
            await CheckForUpdateAsync();
            if (updateInfo is null || updateInfo.Tag == declinedUpdateTag) return false;
            OpenUpdateModal($"Before you {what}: both players must be on the same version, and a newer one is out.");
            return true;
        }

        private void UpdateFailed(string text)
        {
            updateBusy = false; updateStage = "error"; updateStatus = text; updateProgress = null;
            _ = InvokeAsync(StateHasChanged);
        }

        private void OpenUpdatePage() { if (updateInfo is not null) UrlLauncher.Open(updateInfo.PageUrl); }

        private async Task RunUpdateAsync()
        {
            if (updateInfo is null || updateBusy) return;
            var info = updateInfo;
            updateBusy = true; updateStage = "busy"; updateProgress = null; updateStatus = "Starting...";
            await InvokeAsync(StateHasChanged);
            try
            {
                if (info.DownloadUrl is null || info.Sha256 is null)
                {
                    UpdateFailed("This release has no installer file the launcher can check. Download it from the release page.");
                    return;
                }
                if (Process.GetProcessesByName("KingdomCome").Length > 0)
                {
                    UpdateFailed("The game is running, and the installer cannot replace files while it is. Close the game, then try again.");
                    return;
                }
                var progress = new Progress<double>(p => { updateProgress = p; updateStatus = $"Downloading {info.Tag}: {(int)(p * 100)}%"; _ = InvokeAsync(StateHasChanged); });
                var (path, why) = await UpdateCheck.DownloadAsync(info, progress, CancellationToken.None);
                if (path is null)
                {
                    Log.Warning("update: {Why}", why);
                    UpdateFailed("The update could not be installed: " + why + ". You can download it by hand from the release page.");
                    return;
                }
                updateProgress = 1; updateStatus = $"Installing {info.Tag}... the launcher closes and opens again by itself.";
                await InvokeAsync(StateHasChanged);
                Log.Information("update: starting {Path} /SILENT /RELAUNCH=1", path);
                Process.Start(new ProcessStartInfo { FileName = path, Arguments = "/SILENT /RELAUNCH=1", UseShellExecute = true })?.Dispose();
                await Task.Delay(1200);
                ConfirmExit();   // the installer replaces the files this launcher runs from, then opens the new one
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "update: failed");
                UpdateFailed("The update could not be started: " + ex.Message);
            }
        }
        /// <summary>Cairn: Settings > Save my logs. The same bundle Report bug used to offer: one zip on the Desktop, then Explorer shows it.</summary>
        private void CollectMyLogs()
        {
            try
            {
                string? target = null;
                var t = new Thread(() =>
                {
                    using var d = new SaveFileDialog
                    {
                        Title = "Where to save the logs",
                        Filter = "Zip file (*.zip)|*.zip",
                        FileName = $"Cairn-logs-{DateTime.Now:yyyyMMdd-HHmmss}.zip",
                        InitialDirectory = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory),
                        OverwritePrompt = true,
                    };
                    if (d.ShowDialog() == DialogResult.OK) target = d.FileName;
                });
                t.SetApartmentState(ApartmentState.STA);
                t.Start();
                t.Join();
                if (target is null) return;
                if (File.Exists(target)) File.Delete(target);   // the dialog asked about overwriting already
                string zip = LogBundle.Collect(LogBundleGameRoot, LogBundleAgentDirectory, LogBundleRelayDirectory, target);
                try { Process.Start(new ProcessStartInfo("explorer.exe", "/select,\"" + zip + "\"") { UseShellExecute = true })?.Dispose(); } catch { }
                ShowMessage("LOGS SAVED", "The logs are saved: " + Path.GetFileName(zip) + ".", "");
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "logs: could not be collected");
                ShowMessage("LOGS", "The logs could not be collected: " + ex.Message, "");
            }
        }

        private void UninstallApp()
        {
            showSettings = false;
            string? unins = null;
            try { unins = Directory.GetFiles(AppContext.BaseDirectory, "unins*.exe").OrderBy(f => f, StringComparer.OrdinalIgnoreCase).FirstOrDefault(); }
            catch (Exception ex) { Log.Warning(ex, "uninstall: the folder could not be read"); }
            if (unins is null)
            {
                ShowMessage("UNINSTALL",
                    "This copy of Cairn was not put here by Setup (there is no uninstaller beside the launcher), so it cannot be uninstalled from here.",
                    "Remove it from Windows' installed apps if it is listed there, or delete this folder.");
                return;
            }
            try
            {
                Log.Information("uninstall: starting {Unins} /SILENT /REMOVEMOD=1", unins);
                Process.Start(new ProcessStartInfo { FileName = unins, Arguments = "/SILENT /REMOVEMOD=1", UseShellExecute = true, WorkingDirectory = AppContext.BaseDirectory })?.Dispose();
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "uninstall: the uninstaller did not start");
                ShowMessage("UNINSTALL", "The uninstaller could not be started.", "Remove Cairn from Windows' installed apps instead.");
                return;
            }
            ConfirmExit();   // the uninstaller removes the files this launcher runs from
        }
    }
}