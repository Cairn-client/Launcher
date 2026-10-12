// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
// Portions from the original project, marczukmichal/kcd2-multiplayer; its author keeps their copyright (AUTHORS.md).
using System.Collections.Generic;
using Cairn.Wire;

// no launcher-only usings here -- the agent test project compiles this file by link
// (the settings store's tests need AppSettings itself).
namespace Cairn.Launcher.Models
{
    public class ServerInfo
    {
        public string? Token { get; set; } = null; // The master server's listing id, or null for a manually-added server.
        public string Name { get; set; } = "";
        public string Ip { get; set; } = "";
        public int Port { get; set; }

        // The relay's own /api/information port, as the master server
        // published it. 0 for a manually-added server, which falls
        // back to AppSettings.ServerInfoPort instead -- there is nothing to
        // publish it for one of those.
        public int InfoPort { get; set; } = 0;

        public string MapName { get; set; } = "";
        public int Players { get; set; } = 0;
        public int MaxPlayers { get; set; } = 0;
        public int Ping { get; set; } = -1;

        public bool IsOnline { get; set; } = false;

        // a host reached through Steam (its join code) instead of Ip:Port.
        // Never stored in custom_servers.json: built on the fly by the Steam join window.
        [System.Text.Json.Serialization.JsonIgnore]
        public string? SteamCode { get; set; }
    }

    // the host's relay, GET api/local/status (loopback only).
    public class RelayLocalStatusData
    {
        public string Release { get; set; } = "";
        public RelaySteamData Steam { get; set; } = new();
        public int Players { get; set; }
        public bool HostConnected { get; set; }
        public string? RefusedRelease { get; set; }
        public int RefusedSecondsAgo { get; set; } = -1;
    }

    public class RelaySteamData
    {
        public string State { get; set; } = "off";
        public string Message { get; set; } = "";
        public uint AppId { get; set; }
        public string AppName { get; set; } = "";
        public string? Code { get; set; }
        public int Peers { get; set; }
    }

    // CairnClient.exe --test-connection (ConnectionTest.Result.ToJson).
    public class TestConnectionData
    {
        public bool Reachable { get; set; }
        public string Via { get; set; } = "";
        public int RttMs { get; set; } = -1;
        public int ConnectMs { get; set; } = -1;
        public string? HostRelease { get; set; }
        public string MyRelease { get; set; } = "";
        public bool VersionMatch { get; set; }
        public bool? HostConnected { get; set; }
        public int Players { get; set; } = -1;
        public string Kind { get; set; } = "";
        public string Message { get; set; } = "";
        public string Next { get; set; } = "";
        public int SteamPingMs { get; set; } = -1;
        public bool? Relayed { get; set; }
        public string Detail { get; set; } = "";
    }

    // CairnClient.exe --steam-friends (SteamFriendsList). Names are shown, never logged.
    public class SteamFriendsData
    {
        public string State { get; set; } = "";
        public string Message { get; set; } = "";
        public string Next { get; set; } = "";
        public string App { get; set; } = "";
        public List<SteamFriendData> Friends { get; set; } = new();
    }

    public class SteamFriendData
    {
        public string Name { get; set; } = "";
        public string Code { get; set; } = "";
        public string Release { get; set; } = "";
    }

    public class DedicatedServerInfoData
    {
        public string MapName { get; set; } = "Unknown";
        public int Players { get; set; } = 0;
        public int MaxPlayers { get; set; } = 0;
    }

    // Mirrors VersionStatusDto/PeerVersionDto in dotnet/Cairn.Client/VersionIpcServer.cs.
    public class VersionStatusData
    {
        public string MyReleaseVersion { get; set; } = "";
        public List<PeerVersionData> Peers { get; set; } = new();
    }

    public class PeerVersionData
    {
        public byte GhostId { get; set; }
        public string ReleaseVersion { get; set; } = "";
    }

    public class AppSettings
    {
        // KCD2 must be started through the Modding Tools build: the debug REST
        // API on port 1403 that CairnClient talks to exists only there, and the
        // retail executable is monolithic and exports nothing to hook. Pointing
        // this at the base game produces a running game the agent cannot reach.
        public string GamePath { get; set; } = "";

        // Empty means "the pack's": the programs of a game's mod (native plugin, agent, relay) come with that mod's pack and are
        // started from the pack store. A path the player (or a developer) writes here wins; a relative one means "next to the launcher".
        public string DllPath { get; set; } = "";

        // The agent. Launching the game and injecting the DLL is only half the
        // system — without this process nothing talks to the relay at all.
        public string AgentPath { get; set; } = "";

        // How long to let the game get far enough along to be injected. The
        // injector needs the target's modules loaded, not just a live pid.
        public int InjectDelaySeconds { get; set; } = 20;

        // Where the master server IS, not an endpoint on it (the C#
        // master server replaces the old Flask service) -- the launcher
        // appends MasterApi.ListPath itself, tolerating a URL that already
        // names it. 127.0.0.1 rather than "localhost": resolving "localhost"
        // tried this machine's IPv6 loopback first, which sat in SYN_SENT
        // rather than refusing outright, and made the very first fetch of
        // every launch look like the master could not be reached even once
        // it was actually up. Confirmed live --
        // !IMPORTANT change the host in release
        public string MasterServerUrl { get; set; } = "http://127.0.0.1:5100";

        // The relay's HTTP listener for /api/information, which is a different
        // port from the TCP port peers connect on. The master server records
        // only the TCP port, so the browser assumes this one is the same for
        // every relay — true for a default build (appsettings.json binds 5273),
        // and an assumption rather than something the master tells us.
        public int ServerInfoPort { get; set; } = 5273;

        // Where the locally-running agent's dice IPC listener binds (see
        // dotnet/Cairn.Client/DiceIpcServer.cs). Local-machine only, unlike
        // ServerInfoPort -- there is no per-relay value to guess here.
        //
        // dice overlay retired the launcher's dice window: dice is played in game now,
        // and nothing in this process reads this any more. Kept as a settings
        // field so an existing settings.json still round-trips unchanged, and
        // because the agent-side endpoint itself is still there as a debug
        // mirror -- for that decision.
        public int DiceIpcPort { get; set; } = 5901;

        // Where the locally-running agent's release-version IPC listener
        // binds (see dotnet/Cairn.Client/VersionIpcServer.cs,). Local
        // machine only, matching Cairn.Client's own VersionIpcPort default --
        // this one is live: it is how the version-mismatch notification
        // learns the connected peer's release version at all.
        public int VersionIpcPort { get; set; } = 5902;

        public string Language { get; set; } = "en";

        // The relay executable, started locally when the player clicks "Host".
        // Same resolution rule as AgentPath/DllPath: relative means "next to
        // the launcher", which is where a packaged release puts it.
        public string RelayPath { get; set; } = "";

        // The master server executable, auto-started alongside the
        // launcher itself -- not on Host, unlike RelayPath -- whenever
        // MasterServerUrl names a loopback address, since a default install
        // otherwise has nothing answering it there. See Home.razor.cs's
        // EnsureLocalMasterServerAsync. Resolved the same way as
        // RelayPath/AgentPath/DllPath (relative to the launcher), but in its
        // own subfolder rather than flat-merged with everything else:
        // confirmed live, sharing a folder let another project's publish
        // step silently overwrite one of its dependency DLLs with an
        // incompatible version, crashing it on next launch. See
        // tools/Publish-Release.ps1, which places it here.
        public string MasterServerPath { get; set; } = "MasterServer\\CairnMasterServer.exe";

        // TCP port the locally-hosted relay listens on, and what a joining
        // friend needs in their own address bar. Matches Cairn.Server's own
        // default (Tcp:Port in appsettings.json) so the common case needs no
        // coordination.
        public int HostPort { get; set; } = 7778;

        // Friends on Cairn: the launcher tells the master server who you are (your Steam name and friend ids, read from Steam's own files) and
        // what you play, so your Steam friends who also run Cairn can see you are online and invite you. A friend is shown only when you
        // both list each other. Off = nothing is sent and no notices appear.
        public bool PresenceEnabled { get; set; } = true;

        // A corner notice when a friend comes online or starts playing, and when one invites you.
        public bool NotifyFriends { get; set; } = true;

        // "Also allow Steam" in the Host window (the relay also listens on Steam P2P).
        public bool HostAllowSteam { get; set; } = true;

        // Cairn: the host's own addresses are never shown in the Host window unless the player asks for them (Settings).
        public bool ShowHostAddress { get; set; } = false;

        // DLSS 5 in the Kingdom Come: Deliverance II game folder, put in and taken out by DLSS 5 Autopilot when the game's settings are saved (Services/Dlss5Service.cs).
        public bool Dlss5Enabled { get; set; } = false;

        // CONNECT waits until it can work, with the reason under it -- the
        // host's until the game's log says the world is loaded and settled, the
        // joiner's until the host's relay says the host is in (Models/ConnectGate.cs).
        // false = the button is always enabled, as before 0.45.0.
        public bool ConnectGate { get; set; } = true;

        // a game this launcher starts shows Start Game / Join Game on its main menu (Continue, New Game
        // and Load Game greyed, the Modding Tools debug entries gone). A game started any other way keeps the
        // game's own menu. false = the game's own menu here too (the CONNECT flow as before).
        public bool MenuTakeover { get; set; } = true;

        // the bundled start save (a folder holding one save, or the file) a host's New adventure starts
        // from. Relative = next to the launcher, where Setup puts it.
        public string StartSavePath { get; set; } = "start-save";

        // which release of a game's mod to follow, by game id: "stable" (the default, also for a game that is not in the list) or "beta"
        // (the newest release, pre-releases included). Set from the mod row on the game's page.
        public Dictionary<string, string> ModChannels { get; set; } = new();

        // Host finds the relay's ports taken by this install's own relay (a
        // launcher closed with the window's X leaves it running): the same release with
        // the same settings is used again, any other is stopped and replaced; another
        // program's port is never touched (Models/RelayReuse.cs). false = no check, the
        // relay is started as before 0.45.0.
        public bool RelayReuse { get; set; } = true;

        // connection path (Settings, advanced): the Steam app id both players use. 2429020 is the
        // game's own (Modding Tools); 480 and 1771300 are selectable. The maintainer decides
        // the final default after the test.
        public uint SteamAppId { get; set; } = 2429020;

        // the last Steam code typed in the Join window, for this launcher
        // run only: never written to settings.json (the log bundle carries that
        // file, and a code names the host's Steam account).
        [System.Text.Json.Serialization.JsonIgnore]
        public string LastSteamCode { get; set; } = "";
    }
}