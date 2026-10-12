// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using Cairn.Setup;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// How the launcher talks to a game's mod through the game's console (Lua calls) and its log (event lines). The mod's pack
    /// names them (<c>lua</c> in pack.json); until a pack is downloaded the names of the one mod that exists today stand.
    /// </summary>
    public static class ModLua
    {
        private static readonly object Gate = new();
        private static PackContract? _contract;
        private static bool _loaded;

        private static PackContract? Contract
        {
            get
            {
                lock (Gate)
                {
                    if (!_loaded) { _contract = PackStore.ReadContract(PackStore.DefaultRoot, ModInstall.PackId); _loaded = true; }
                    return _contract;
                }
            }
        }

        /// <summary>The downloaded pack changed (a download, an update, a removal): the names are read again.</summary>
        public static void Reload() { lock (Gate) _loaded = false; }

        private static string Name(string key, string fallback) =>
            Contract?.Lua is { } l && l.TryGetValue(key, out var v) ? v : fallback;

        /// <summary>The prefix of the mod's event lines in the game's log, e.g. "[KCD2-MP-EVT] v1 " (then the sequence number and the event).</summary>
        public static string EventPrefix => Name("eventPrefix", "[KCD2-MP-EVT] v1 ");

        /// <summary>The word the start-game menu's choices carry in their event lines.</summary>
        public static string StartGameEvent => Name("startGameEvent", "w159");

        /// <summary>The agent's one-shot helper for the start-game menu (worlds, start save staging): the flag it is run with and the tag of the line it prints.</summary>
        public static string HelperFlag => Name("helperFlag", "--w159");
        public static string HelperTag => Name("helperTag", "W159");

        /// <summary>The native plugin's own log, beside its DLL: the launcher reads it to see that the plugin attached to the game.</summary>
        public static string NativeLog => Name("nativeLog", "kcdmp-native.log");

        public static string StartGameModel => Name("startGameModel", "KCD2MP_W159Model");
        public static string StartGameTick => Name("startGameTick", "KCD2MP_W159Tick");
        public static string StartGameRecapStart => Name("startGameRecapStart", "KCD2MP_W159RecapStart");
        public static string StartGameDisarm => Name("startGameDisarm", "KCD2MP_W159Disarm");
        public static string JoinFlowLoad => Name("joinFlowLoad", "KCD2MP_Wo124LoadGame");

        /// <summary>The in-game mod menu's request function (the launcher draws the menu; the mod answers).</summary>
        public static string MenuRequest => Name("menuRequest", "KCD2MP_MenuRequest");

        /// <summary>
        /// The version two players must share, as the relay and the agents compare it: the downloaded mod's MAJOR.MINOR.0 (the mod has
        /// its own version, apart from the launcher's); the launcher's own when no mod is downloaded.
        /// </summary>
        public static string Handshake()
        {
            var v = PackStore.Read(PackStore.DefaultRoot, ModInstall.PackId)?.Version;
            return Version.TryParse(v, out var p) ? $"{p.Major}.{p.Minor}.0" : Components.Shared.Globals.Version;
        }

        /// <summary>A guarded call: nothing happens in a game whose mod does not have the function.</summary>
        public static string Call(string function, string args = "") => $"if {function} then {function}({args}) end";
    }
}