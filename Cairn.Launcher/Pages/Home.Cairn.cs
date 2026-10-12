// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
using System.Diagnostics;
using System.IO;
using System.Text.Json;
using System.Windows.Forms;
using Cairn.Launcher.Components.Shared;
using Cairn.Setup;
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
        // the agent's own checker (the agent's validate helper) reads the file, the playstyle it fits is found, and the file goes
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


        // ------------------------------------------------------------ what the game's pack does for the settings window and the logs

        /// <summary>The program of the game's pack (<c>optionsProgram</c>), or null when it is not downloaded.</summary>
        private string? PackProgramPath() => ExternalInstall.OptionsProgramPath(PacksRoot, ModInstall.PackId) is { } p && File.Exists(p) ? p : null;

        /// <summary>Settings > Saves: every save of the game and every character and chest the mod kept goes, so a new adventure starts clean (the pack's program does it).</summary>
        private async Task DeleteAllSavesAsync()
        {
            if (gameRunning) { UiService.ShowError("Close the game first: its saves cannot be deleted while it is running."); return; }
            if (PackProgramPath() is not { } program) { UiService.ShowError(ModTooOldText); return; }
            var (ok, message) = await SavesRunner.DeleteAllAsync(program);
            Log.Information("saves: delete all: {Ok}: {Message}", ok, message);
            if (ok) ShowToast(message); else UiService.ShowError(message);
            await InvokeAsync(StateHasChanged);
        }

        /// <summary>
        /// A joiner's new character (and a host's New adventure) begin from a start save placed in start-save\&lt;playstyle&gt; beside the launcher. A player makes one from his own
        /// Modding Tools game: he picks the file, the pack's program checks it and files it under the playstyle it fits.
        /// </summary>
        private async Task AddStartSaveAsync()
        {
            string? file = PickSaveFile();
            if (file is null) return;
            if (PackProgramPath() is not { } program) { ShowMessage("START SAVE", ModTooOldText, ""); return; }
            bool answered = false;
            await PackSession.RunOnceAsync(program, new[] { "startsave", "add", "--file", file, "--start-saves", StartSaveRoot },
                s => s.Message += (title, text, next) => { answered = true; _ = InvokeAsync(() => ShowMessage(title, text, next)); }, TimeSpan.FromMinutes(4));
            if (!answered) ShowMessage("START SAVE", "The start save could not be added.", "");
        }

        /// <summary>The game's side of Save the logs and Report a bug: the pack's program copies its game's logs into a folder, which the bundle adds. Null when there is nothing.</summary>
        private async Task<string?> CollectPackLogsAsync()
        {
            if (PackProgramPath() is not { } program) return null;
            string dir = Path.Combine(Path.GetTempPath(), "cairn-logs-" + Guid.NewGuid().ToString("N")[..8]);
            var args = new List<string> { "logs", "collect", "--out", dir };
            if (!string.IsNullOrWhiteSpace(settings.GamePath)) { args.Add("--game-exe"); args.Add(settings.GamePath); }
            await PackSession.RunOnceAsync(program, args, _ => { }, TimeSpan.FromSeconds(60));
            return Directory.Exists(dir) ? dir : null;
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
        private async Task CollectMyLogs()
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
                string zip = LogBundle.Collect(await CollectPackLogsAsync(), target);
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