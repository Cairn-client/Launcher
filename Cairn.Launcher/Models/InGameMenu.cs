// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;

namespace Cairn.Launcher.Models
{
    /// <summary>What an item of the in-game menu is: a switch, a choice between two words, a key, something to do, or a line of information.</summary>
    public enum MenuKind { OnOff, Choice, Key, Action, Info }

    /// <summary>One row of the menu. <see cref="On"/> is set for a switch (null for the rest). A locked item belongs to the host. A switch with a
    /// <see cref="Gate"/> (a key name such as "f3") decides whether the launcher lets that key reach the game: only while the switch is on.</summary>
    public sealed record MenuItem(string Id, string Group, MenuKind Kind, bool? On, bool Locked, string Value, string Label, string Help, string Gate = "", bool Disabled = false)
    {
        public bool Selectable => Kind != MenuKind.Info;
    }

    /// <summary>The whole menu as a game's mod describes it: same shape for every game, only the content differs.</summary>
    public sealed record MenuState(string Game, string Title, string Blocked, string Note, IReadOnlyList<MenuItem> Items)
    {
        public static readonly MenuState Empty = new("", "Mod menu", "", "", Array.Empty<MenuItem>());
    }

    /// <summary>
    /// The shared in-game menu. Every game's mod tells the launcher what its menu holds with one line in the game's log,
    /// <c>[CAIRN-EVT] v1 &lt;seq&gt; menu_state &lt;game&gt;~&lt;title&gt;~&lt;blocked&gt;~&lt;note&gt;|&lt;item&gt;|&lt;item&gt;...</c>, an item being
    /// <c>id~group~kind~on~locked~value~label~help~gate</c> (kind: onoff|choice|key|action|info; on: 1, 0 or -; locked: 1 or 0).
    /// The launcher draws it, takes the keys, and asks the mod to change something with a console call. Nothing here is
    /// about one game: the mod chooses the words and the items.
    /// </summary>
    public static class InGameMenuProtocol
    {
        public const char ItemSep = '|';
        public const char FieldSep = '~';

        /// <summary>The menu_state line's payload from a kcd.log line, or null when the line is something else.</summary>
        public static MenuState? Parse(string line)
        {
            string pfx = ModLua.EventPrefix;
            int at = line.IndexOf(pfx, StringComparison.Ordinal);
            if (at < 0) return null;
            string rest = line[(at + pfx.Length)..];
            int s1 = rest.IndexOf(' ');
            if (s1 < 0 || !long.TryParse(rest.AsSpan(0, s1), NumberStyles.None, CultureInfo.InvariantCulture, out _)) return null;
            rest = rest[(s1 + 1)..];
            const string name = "menu_state ";
            if (!rest.StartsWith(name, StringComparison.Ordinal)) return null;
            string payload = rest[name.Length..].TrimEnd();

            var parts = payload.Split(ItemSep);
            var head = parts[0].Split(FieldSep);
            if (head.Length < 4) return null;
            var items = new List<MenuItem>();
            for (int i = 1; i < parts.Length; i++)
            {
                var f = parts[i].Split(FieldSep);
                if (f.Length < 8 || f[0].Length == 0) continue;   // a damaged item is left out, not the whole menu
                MenuKind? kind = f[2] switch { "onoff" => MenuKind.OnOff, "choice" => MenuKind.Choice, "key" => MenuKind.Key, "action" => MenuKind.Action, "info" => MenuKind.Info, _ => null };
                if (kind is null) continue;
                bool? on = f[3] == "1" ? true : f[3] == "0" ? false : null;
                items.Add(new MenuItem(f[0], f[1], kind.Value, on, f[4] == "1", f[5], f[6], f[7], f.Length > 8 ? f[8] : ""));
            }
            return new MenuState(head[0], head[1], head[2], head[3], items);
        }

        /// <summary>
        /// A partner's spoken line from a kcd.log line: <c>[CAIRN-EVT] v1 &lt;seq&gt; subtitle &lt;seconds&gt;~&lt;text&gt;</c> (the mod's Cairn_SpeechShow),
        /// or null when the line is something else. The seconds are kept between 1 and 15, the text to 400 characters.
        /// </summary>
        public static (double Seconds, string Text)? ParseSubtitle(string line)
        {
            string pfx = ModLua.EventPrefix;
            int at = line.IndexOf(pfx, StringComparison.Ordinal);
            if (at < 0) return null;
            string rest = line[(at + pfx.Length)..];
            int s1 = rest.IndexOf(' ');
            if (s1 < 0 || !long.TryParse(rest.AsSpan(0, s1), NumberStyles.None, CultureInfo.InvariantCulture, out _)) return null;
            rest = rest[(s1 + 1)..];
            const string name = "subtitle ";
            if (!rest.StartsWith(name, StringComparison.Ordinal)) return null;
            string payload = rest[name.Length..].TrimEnd();
            int sep = payload.IndexOf(FieldSep);
            if (sep <= 0 || !double.TryParse(payload.AsSpan(0, sep), NumberStyles.Float, CultureInfo.InvariantCulture, out double secs)) return null;
            string text = payload[(sep + 1)..].Trim();
            if (text.Length == 0) return null;
            return (Math.Clamp(secs, 1, 15), text.Length > 400 ? text[..400] : text);
        }

        /// <summary>
        /// A notice the mod wants shown (a partner joining the world): <c>[CAIRN-EVT] v1 &lt;seq&gt; notice &lt;text&gt;</c> shows it until
        /// <c>[CAIRN-EVT] v1 &lt;seq&gt; notice_hide</c>. Null when the line is something else; Text is empty for the hide.
        /// </summary>
        public static (bool Show, string Text)? ParseNotice(string line)
        {
            string pfx = ModLua.EventPrefix;
            int at = line.IndexOf(pfx, StringComparison.Ordinal);
            if (at < 0) return null;
            string rest = line[(at + pfx.Length)..];
            int s1 = rest.IndexOf(' ');
            if (s1 < 0 || !long.TryParse(rest.AsSpan(0, s1), NumberStyles.None, CultureInfo.InvariantCulture, out _)) return null;
            rest = rest[(s1 + 1)..].TrimEnd();
            if (rest == "notice_hide") return (false, "");
            if (!rest.StartsWith("notice ", StringComparison.Ordinal)) return null;
            string text = rest[7..].Trim();
            if (text.Length == 0) return null;
            return (true, text.Length > 200 ? text[..200] : text);
        }

        /// <summary>The virtual-key code of a gate key name ("f1" to "f12"), or null for a name this does not know.</summary>
        public static int? VkOfGate(string name)
        {
            string n = (name ?? "").Trim().ToLowerInvariant();
            return n.Length is 2 or 3 && n[0] == 'f' && int.TryParse(n.AsSpan(1), NumberStyles.None, CultureInfo.InvariantCulture, out int k) && k is >= 1 and <= 12 ? 0x70 + k - 1 : null;
        }

        /// <summary>The console call that asks the mod for its state (it answers with a menu_state line).</summary>
        public static string AskState => ModLua.Call(ModLua.MenuRequest, "\"state\"");

        /// <summary>Tells the mod the launcher draws the menu: the mod's own key handling steps aside.</summary>
        public static string Attach => ModLua.Call(ModLua.MenuRequest, "\"attach\"");

        /// <summary>The console call for "do this to that item"; the id is the mod's own, kept to letters, digits and underscores.</summary>
        public static string? DoCall(string id, string verb)
        {
            if (id.Length is 0 or > 40 || !id.All(c => char.IsAsciiLetterOrDigit(c) || c == '_')) return null;
            if (verb is not ("enter" or "nudge")) return null;
            return ModLua.Call(ModLua.MenuRequest, $"\"do {id} {verb}\"");
        }
    }

    /// <summary>The keys the menu takes while it is open.</summary>
    public enum MenuKey { Toggle, Up, Down, Left, Right, Enter, Back }

    /// <summary>What the player has done with the menu, in the order of the screen: the selection, and what each key asks for.</summary>
    public sealed class MenuNav
    {
        public MenuState State { get; private set; } = MenuState.Empty;
        public bool Open { get; private set; }
        public string SelectedId { get; private set; } = "";

        /// <summary>The selected row's index in <see cref="MenuState.Items"/> (the first one that can be chosen when nothing is).</summary>
        public int SelectedIndex
        {
            get
            {
                var items = State.Items;
                for (int i = 0; i < items.Count; i++) if (items[i].Id == SelectedId && items[i].Selectable) return i;
                for (int i = 0; i < items.Count; i++) if (items[i].Selectable) return i;
                return -1;
            }
        }

        public MenuItem? Selected => SelectedIndex is int i and >= 0 ? State.Items[i] : null;

        public void Show() { Open = true; }
        public void Hide() { Open = false; }

        /// <summary>A new state from the mod: the selection stays on the same item when it is still there.</summary>
        public void Update(MenuState state)
        {
            string keep = Selected?.Id ?? SelectedId;
            State = state;
            SelectedId = keep;
            var sel = Selected;
            SelectedId = sel?.Id ?? "";
        }

        /// <summary>Up or Down: the next item that can be chosen, wrapping round at either end.</summary>
        public void Step(int dir)
        {
            var items = State.Items;
            int n = items.Count, i = SelectedIndex;
            if (n == 0 || i < 0) return;
            for (int k = 0; k < n; k++)
            {
                i += dir;
                if (i < 0) i = n - 1; else if (i >= n) i = 0;
                if (items[i].Selectable) { SelectedId = items[i].Id; return; }
            }
        }

        /// <summary>
        /// The request a key makes of the mod, or null when it makes none. Left and Right change a value (never run an
        /// action: a stray arrow must not do "I'm stuck"); Enter does the item, whatever it is. A locked item (the host's)
        /// is left to the mod, which says why it refuses.
        /// </summary>
        public (string Id, string Verb)? Request(MenuKey key)
        {
            var sel = Selected;
            if (sel is null || sel.Disabled) return null;
            return key switch
            {
                MenuKey.Enter => (sel.Id, "enter"),
                MenuKey.Left or MenuKey.Right => sel.Kind is MenuKind.OnOff or MenuKind.Choice or MenuKind.Key ? (sel.Id, "nudge") : null,
                _ => null,
            };
        }
    }
}
