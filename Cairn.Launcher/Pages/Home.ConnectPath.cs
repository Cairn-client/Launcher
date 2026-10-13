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
    /// Steam as a connection path, plain connection errors, Test
    /// connection. The network work runs in the agent (CairnClient.exe), as a
    /// one-shot helper here (AgentHelper) or as the session agent; this file
    /// only shows what it says. Nothing identifying is logged: no Steam code,
    /// no persona names.
    /// </summary>
    public partial class Home
    {
        #region connection path state
        // Host
        private string hostSteamState = "off";
        private string hostSteamMessage = "";
        private string? hostSteamCode;
        private string hostRefusedNotice = "";

        // Join through Steam
        private bool showJoinSteam;
        private string steamCodeInput = "";
        private List<SteamFriendData>? steamFriends;
        private string steamFriendsNote = "";
        private string steamResultMessage = "";
        private bool steamResultOk;
        private bool steamBusy;
        private string steamBusyText = "";

        // Message modal (test results, Steam fallback)
        private bool showMessage;
        private string messageTitle = "";
        private string messageText = "";
        private string messageNext = "";
        private bool messageAddressFallback;
        private bool messageReportBug;
        private string fallbackAddress = "";

        // The agent's connection, in plain words
        private string connStatusLine = "";
        private bool connStatusBad;
        #endregion

        // ------------------------------------------------------------ host

        // ------------------------------------------------------------ join through Steam

        private async Task OpenJoinSteam()
        {
            if (!await EnsureSetupReadyAsync()) return;   // Host and Join unlock at Ready!
            showJoinSteam = true;
            steamCodeInput = settings.LastSteamCode ?? "";
            steamResultMessage = "";
            steamFriendsNote = "";
            steamFriends = null;
            StateHasChanged();
            _ = WatchSteamFriendsAsync();   // Cairn: the friends who host are looked for at once, then every 15 s while the window is open
        }

        private async Task WatchSteamFriendsAsync()
        {
            try
            {
                while (showJoinSteam)
                {
                    if (!steamBusy) await FindSteamFriendsAsync();
                    for (int i = 0; i < 15 && showJoinSteam; i++) await Task.Delay(1000);
                }
            }
            catch (Exception ex) { Log.Warning(ex, "Steam friends watch stopped"); }
        }

        private async Task JoinThroughSteamAsync()
        {
            string code = steamCodeInput.Trim();
            if (code.Length == 0) return;
            // the agent reads a pasted code forgivingly (text, odd dashes around it); a quote or a line
            // break would cut its command-line argument, so those go here.
            code = new string(code.Where(c => c is not ('"' or '\r' or '\n')).ToArray()).Trim();
            if (code.Length == 0) return;
            // This run only: LastSteamCode is never written (AppModels.cs). the whole-object
            // save that used to follow wrote nothing of it and rewrote every other setting.
            settings.LastSteamCode = code;
            showJoinSteam = false;
            Log.Information("Join through Steam (app {App})", settings.SteamAppId);
            await LaunchGame(new ServerInfo { Name = "(Steam)", Ip = "", Port = 0, Ping = 0, SteamCode = code });
        }

        // ------------------------------------------------------------ Test connection (address)

        private void ShowMessage(string title, string text, string next, bool addressFallback = false, bool reportBug = false, bool launchAnyway = false)
        {
            messageTitle = title; messageText = text; messageNext = next; messageAddressFallback = addressFallback;
            messageReportBug = reportBug;   // the blocked-file message offers Report a bug
            launchAnywayAvailable = launchAnyway;   // and, for the mod's own files, LAUNCH ANYWAY
            showMessage = true;
            _ = InvokeAsync(StateHasChanged);
        }

        // ------------------------------------------------------------ the session agent

    }
}
