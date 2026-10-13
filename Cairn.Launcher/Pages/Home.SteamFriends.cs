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
        private static readonly HttpClient steamProfiles = new() { Timeout = TimeSpan.FromSeconds(6) };

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

                // the friends are on screen at once, from Steam's own files; each one's status fills in as its answer arrives
                // (what was known from the last look stays until then), and a slow profile holds up nobody else
                var known = homeSteamFriends.ToDictionary(x => x.Id);
                var rows = named.Take(MaxFriendLookups)
                    .Select(f => known.TryGetValue(f.Id, out var old) ? old with { Name = f.Name, Avatar = f.Avatar } : new SteamFriendRow(f.Id, f.Name, "loading", "", "", f.Avatar))
                    .ToList();
                homeSteamFriends = SortedFriends(rows);
                homeSteamFriendsLoaded = true;
                await InvokeAsync(StateHasChanged);

                using var gate = new SemaphoreSlim(8);
                DateTime painted = DateTime.MinValue;
                var work = rows.Select(r => r.Id).ToList().Select(async id =>
                {
                    await gate.WaitAsync();
                    try
                    {
                        var s = await SteamProfileStatus.FetchAsync(steamProfiles, id, CancellationToken.None);
                        bool repaint;
                        lock (rows)
                        {
                            int i = rows.FindIndex(x => x.Id == id);
                            if (i >= 0) rows[i] = rows[i] with { State = s.State, Game = s.Game, Message = s.Message };
                            homeSteamFriends = SortedFriends(rows);
                            repaint = DateTime.UtcNow - painted > TimeSpan.FromMilliseconds(250);
                            if (repaint) painted = DateTime.UtcNow;
                        }
                        if (repaint) await InvokeAsync(StateHasChanged);
                    }
                    finally { gate.Release(); }
                });
                await Task.WhenAll(work);
                lock (rows) homeSteamFriends = SortedFriends(rows);
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

        private static List<SteamFriendRow> SortedFriends(IEnumerable<SteamFriendRow> rows) =>
            rows.OrderBy(r => FriendRank(r.State)).ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase).ToList();

        private static int FriendRank(string state) => state switch
        {
            SteamProfileStatus.InGame => 0,
            SteamProfileStatus.Online => 1,
            SteamProfileStatus.Offline => 2,
            "loading" => 2,   // not known yet: with the offline ones, until the answer moves it
            _ => 3,
        };

        /// <summary>The friend as the presence feature sees them (online with Cairn, and what they do), or null.</summary>
        private Cairn.Wire.PresenceFriend? CairnFriend(SteamFriendRow f) => friendSeen.TryGetValue(f.Id, out var p) ? p : null;

        /// <summary>The hosting friend (asked of the pack's program) whose name is this Steam friend's, or null.</summary>
        private SteamFriendData? HostingAs(SteamFriendRow f) =>
            homeFriends?.FirstOrDefault(h => string.Equals(h.Name, f.Name, StringComparison.OrdinalIgnoreCase));
    }
}
