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
    public sealed record MenuState(string Game, string Title, string Blocked, string Note, IReadOnlyList<MenuItem> Items, bool Submenu = false)
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

        /// <summary>The virtual-key code of a gate key name ("f1" to "f12"), or null for a name this does not know.</summary>
        public static int? VkOfGate(string name)
        {
            string n = (name ?? "").Trim().ToLowerInvariant();
            return n.Length is 2 or 3 && n[0] == 'f' && int.TryParse(n.AsSpan(1), NumberStyles.None, CultureInfo.InvariantCulture, out int k) && k is >= 1 and <= 12 ? 0x70 + k - 1 : null;
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
        public (string Id, string Verb, int Dir)? Request(MenuKey key)
        {
            var sel = Selected;
            if (sel is null || sel.Disabled) return null;
            return key switch
            {
                MenuKey.Enter => (sel.Id, "enter", 1),
                MenuKey.Left or MenuKey.Right => sel.Kind is MenuKind.OnOff or MenuKind.Choice or MenuKind.Key ? (sel.Id, "nudge", key == MenuKey.Left ? -1 : 1) : null,
                _ => null,
            };
        }
    }
}
