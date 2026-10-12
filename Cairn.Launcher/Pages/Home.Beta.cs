// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using Cairn.Wire;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// the launcher's last work order before the public beta.
    /// <list type="bullet">
    /// <item>The player's files. settings.json, custom_servers.json and favorites.json belong to
    ///   the player (the maintainer's rule, permanent): every write is the player's own change, key
    ///   by key or entry by entry, atomically; nothing is reset, reordered or dropped, unknown keys
    ///   stay (LauncherSettingsStore, PlayerListFiles). The log names keys, never values.</item>
    /// <item>Windows blocking the mod (Phase 7): every start goes through StartChecked; a refused
    ///   or quarantined file is one plain message by its name (LaunchBlock).</item>
    /// <item>CONNECT waits until it can work (Phase 4.3), with the reason under it (ConnectGate).</item>
    /// <item>The relay's ports held by an earlier launcher's relay (Phase 7): reused or replaced,
    ///   another program's never touched (RelayReuse).</item>
    /// </list>
    /// </summary>
    public partial class Home
    {
        private readonly LauncherSettingsStore settingsStore = new(SettingsFileName, MigrateStaleSettings);

        /// <summary>
        /// Writes what the player changed (all keys, or only <paramref name="only"/>). A file that is
        /// not one JSON object is never written over: the player is told, in plain words, and the
        /// change stays in memory for this run. True when the file has the change.
        /// </summary>
        private bool WriteSettings(string why, params string[] only)
        {
            var r = settingsStore.Save(settings, only.Length == 0 ? null : only);
            if (r.Wrote)
                Log.Information("settings: {Why}: wrote {Set}{Filled}", why, string.Join(", ", r.Set),
                    r.Filled.Count == 0 ? "" : $" (and added the missing keys {string.Join(", ", r.Filled)})");
            else if (!r.Ok)
            {
                Log.Warning("settings: {Why}: settings.json not written -- {Reason}", why, r.Result.Why);
                UiService.ShowError(NotWrittenText("settings.json", r.Result.Why));
            }
            return r.Ok;
        }

        /// <summary>A player's file the launcher left as it is, in plain words: unreadable (never written over) or not writable.</summary>
        private static string NotWrittenText(string file, string why) =>
            why.StartsWith("not one JSON", StringComparison.Ordinal) || why.StartsWith("unreadable JSON", StringComparison.Ordinal)
            || why.StartsWith("the file is not one JSON", StringComparison.Ordinal) || why.StartsWith("unexpected token", StringComparison.Ordinal)
            || why.StartsWith("the root object", StringComparison.Ordinal) || why.StartsWith("text after", StringComparison.Ordinal) || why == "truncated"
                ? $"Your {file} couldn't be read, so the launcher did not change it. Your change is used until the launcher closes. " +
                  $"To keep it, fix or delete {file} in the install folder, then make the change again."
                : $"Windows didn't let the launcher save {file} (it may be read-only, or open in another program). Your change is used " +
                  "until the launcher closes. Check the file, then make the change again.";

        /// <summary>The settings window opens: values another program wrote (the mod menu, through the agent) show as saved.</summary>
        private void OpenSettings()
        {
            var taken = settingsStore.Refresh(settings);
            if (taken.Count > 0) Log.Information("settings: changed outside the launcher since it read them: {Keys}", string.Join(", ", taken));
            showSettings = true;
        }

        /// <summary>The settings window closes without SAVE: its unsaved edits go back.</summary>
        private void CancelSettings()
        {
            var reverted = settingsStore.Revert(settings);
            if (reverted.Count > 0) Log.Debug("settings: closed without saving; {Keys} put back", string.Join(", ", reverted));
            showSettings = false;
        }

        // ------------------------------------------------------------ custom_servers.json, favorites.json

        private enum ServerChange { Add, Edit, Remove }

        // Each listed server's address and port as the file has them: an edit changes the address
        // in memory before it reaches here, and the file's entry is found by the old one.
        private readonly Dictionary<ServerInfo, (string Ip, int Port)> customServerKeys = new(ReferenceEqualityComparer.Instance);

        private void NoteCustomServerKeys()
        {
            customServerKeys.Clear();
            foreach (var s in customServers) customServerKeys[s] = (s.Ip, s.Port);
        }

        // ------------------------------------------------------------ CONNECT waits until it can work (Phase 4.3)
        //
        // The rule is ConnectGateRule (Models/ConnectGate.cs): the host's button opens when the game's own
        // kcd.log says the world is loaded and has been for ConnectGateRule.SettleS; the joiner's when the
        // host's relay says the host's agent is in. The reason is shown in plain words under the button.
        // Passive: the launcher reads a log and asks the relay -- no key, click or focus change, ever.

        private bool connectGateOpen = true;
        private string connectGateReason = "";

        /// <summary>The CONNECT button's state: always on with the ConnectGate setting off.</summary>
        private bool ConnectEnabled => !settings.ConnectGate || connectGateOpen;

    }
}
