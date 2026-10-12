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
    /// A game's options (game.json "options": toggles such as DLSS 5): shown on its settings page, saved with it, and done by the program its pack names
    /// (<c>optionsProgram</c>, Cairn.Setup/GameOptions.cs). The launcher knows nothing of what a tick means; it asks the program what is so, and tells it what the
    /// player chose. For Kingdom Come II, until its pack offers DLSS 5 this way, the launcher's own DLSS 5 code (Home.Dlss5.cs) still does it.
    /// </summary>
    public partial class Home
    {
        private IReadOnlyDictionary<string, OptionStatus> optionStatuses = new Dictionary<string, OptionStatus>();
        private bool optionsBusy;

        /// <summary>The game's folder as its options program wants it: for Kingdom Come II the game's root (the Modding Tools build's own, as DLSS 5 has always had it), else the folder the game was found in.</summary>
        private string? OptionsGameDir(GameProfile g)
        {
            if (GameExeIsSetting(g.Id))
                return string.IsNullOrWhiteSpace(settings.GamePath) ? null : GameLocator.InstallRootOf(settings.GamePath) ?? Path.GetDirectoryName(settings.GamePath);
            return GameFolderOf(g);
        }

        /// <summary>Asks the pack's program what is so for each of the game's options (when the settings window opens).</summary>
        private async Task LoadOptionStatusesAsync(GameProfile g)
        {
            if (g.Options.Count == 0) { optionStatuses = new Dictionary<string, OptionStatus>(); return; }
            string? program = ExternalInstall.OptionsProgramPath(PacksRoot, g.Id), dir = OptionsGameDir(g);
            var found = new Dictionary<string, OptionStatus>();
            if (program is not null && dir is not null && Directory.Exists(dir))
            {
                foreach (var o in g.Options)
                    if (await OptionRunner.CheckAsync(program, o.Id, dir) is { } st) found[o.Id] = st;
            }
            optionStatuses = found;
            // what the tick shows is what is so on this PC (a tick that was lost, or a folder changed by hand)
            foreach (var o in g.Options.Where(o => found.ContainsKey(o.Id))) SetOption(g.Id, o.Id, found[o.Id].On);
            await InvokeAsync(StateHasChanged);
        }

        private bool OptionOn(string gameId, string optionId) =>
            settings.GameOptions.TryGetValue(gameId, out var o) && o.TryGetValue(optionId, out var on) && on;

        private void SetOption(string gameId, string optionId, bool on)
        {
            if (!settings.GameOptions.TryGetValue(gameId, out var o)) settings.GameOptions[gameId] = o = new Dictionary<string, bool>();
            o[optionId] = on;
        }

        /// <summary>After the game's settings were saved: each option whose tick differs from what the program says is so is done by the program.</summary>
        private async Task ApplyGameOptionsAsync(GameProfile g)
        {
            if (g.Options.Count == 0 || optionsBusy) return;
            string? program = ExternalInstall.OptionsProgramPath(PacksRoot, g.Id), dir = OptionsGameDir(g);
            if (program is null) return;   // the pack has not been downloaded (or has no program): nothing to do
            var changed = g.Options.Where(o => optionStatuses.TryGetValue(o.Id, out var st) && st.Available && st.On != OptionOn(g.Id, o.Id)).ToList();
            if (changed.Count == 0) return;
            if (gameRunning || dir is null || !Directory.Exists(dir))
            {
                foreach (var o in changed) SetOption(g.Id, o.Id, optionStatuses[o.Id].On);   // nothing was done: the ticks go back
                WriteSettings("an option could not be changed");
                UiService.ShowError(gameRunning ? "Close the game first: this can only be changed while it is not running." : "Set the game's path in the game's settings first.");
                await InvokeAsync(StateHasChanged);
                return;
            }

            optionsBusy = true;
            await InvokeAsync(StateHasChanged);
            try
            {
                foreach (var o in changed)
                {
                    bool want = OptionOn(g.Id, o.Id);
                    ShowToast(want ? $"Turning on {o.Title}..." : $"Turning off {o.Title}...");
                    var run = await OptionRunner.ApplyAsync(program, o.Id, want, dir, s => Log.Information("options: {Game} {Option}: {Line}", g.Id, o.Id, s));
                    Log.Information("options: {Game} {Option} {State}: {Ok} {Message}", g.Id, o.Id, want ? "on" : "off", run.Ok, run.Message);
                    if (run.Ok)
                    {
                        optionStatuses = new Dictionary<string, OptionStatus>(optionStatuses) { [o.Id] = optionStatuses[o.Id] with { On = want } };
                        ShowToast(run.Message.Length > 0 ? run.Message : want ? $"{o.Title} is on." : $"{o.Title} is off.");
                    }
                    else
                    {
                        SetOption(g.Id, o.Id, !want);   // it stayed as it was
                        WriteSettings($"{o.Title} stayed {(want ? "off" : "on")}");
                        UiService.ShowError((want ? $"{o.Title} could not be turned on." : $"{o.Title} could not be turned off.") + (run.Message.Length > 0 ? "\n\n" + run.Message : ""));
                    }
                }
            }
            finally
            {
                optionsBusy = false;
                await InvokeAsync(StateHasChanged);
            }
        }
    }
}
