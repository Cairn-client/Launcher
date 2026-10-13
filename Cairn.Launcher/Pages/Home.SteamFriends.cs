// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using System.Net.Http;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// The Home tile of Steam friends and what they are doing. The friends and their names come from Steam's own files; their status from
    /// each friend's public Steam Community profile (private profiles show as unknown). Looked at while Home is showing, at most every
    /// 90 seconds, a few at a time.
    /// </summary>
    public partial class Home
    {
        private const int MaxFriendLookups = 60;
        private static readonly HttpClient steamProfiles = new() { Timeout = TimeSpan.FromSeconds(10) };

        private List<SteamFriendRow> homeSteamFriends = new();
        private bool homeSteamFriendsLoaded;
        private bool steamStatusBusy;
        private DateTime steamStatusAt = DateTime.MinValue;

        private async Task RefreshSteamFriendStatusAsync()
        {
            if (steamStatusBusy || DateTime.UtcNow - steamStatusAt < TimeSpan.FromSeconds(90)) return;
            steamStatusBusy = true;
            try
            {
                var me = await Task.Run(() => SteamIdentity.Read(RealSetupHost.Instance.SteamRootFromRegistry()));
                if (me?.Named is not { Count: > 0 } named) { homeSteamFriendsLoaded = true; homeSteamFriends = new(); return; }

                var rows = new List<SteamFriendRow>();
                using var gate = new SemaphoreSlim(4);
                var work = named.Take(MaxFriendLookups).Select(async f =>
                {
                    await gate.WaitAsync();
                    try
                    {
                        var s = await SteamProfileStatus.FetchAsync(steamProfiles, f.Id, CancellationToken.None);
                        lock (rows) rows.Add(new SteamFriendRow(f.Id, f.Name, s.State, s.Game, s.Message));
                    }
                    finally { gate.Release(); }
                });
                await Task.WhenAll(work);

                homeSteamFriends = rows.OrderBy(r => FriendRank(r.State)).ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase).ToList();
                homeSteamFriendsLoaded = true;
                Log.Debug("home: {Count} Steam friends looked at, {Online} online", rows.Count, rows.Count(r => r.State is SteamProfileStatus.Online or SteamProfileStatus.InGame));
            }
            catch (Exception ex) { Log.Debug(ex, "home: Steam friends' status could not be read"); homeSteamFriendsLoaded = true; }
            finally
            {
                steamStatusAt = DateTime.UtcNow;
                steamStatusBusy = false;
                await InvokeAsync(StateHasChanged);
            }
        }

        private static int FriendRank(string state) => state switch
        {
            SteamProfileStatus.InGame => 0,
            SteamProfileStatus.Online => 1,
            SteamProfileStatus.Offline => 2,
            _ => 3,
        };

        /// <summary>The hosting friend (asked of the pack's program) whose name is this Steam friend's, or null.</summary>
        private SteamFriendData? HostingAs(SteamFriendRow f) =>
            homeFriends?.FirstOrDefault(h => string.Equals(h.Name, f.Name, StringComparison.OrdinalIgnoreCase));
    }
}
