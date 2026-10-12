// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Services
{
    /// <summary>
    /// The shared in-game menu of one running game. The game's bridge (Cairn.Setup/BridgeProtocol.cs, docs/BRIDGE.md) says what the mod's menu holds, its notices and
    /// the lines its characters speak; this draws them over the game, takes the keys while the menu is open (Insert opens and closes it, Up and Down choose, Left and Right
    /// change, Enter selects, Esc goes back or closes) and sends the bridge what was chosen. Until the mod has answered once nothing is taken: a game without the
    /// launcher's menu keeps its own keys.
    /// </summary>
    internal sealed class InGameMenuDriver
    {
        [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
        [DllImport("user32.dll")] private static extern bool GetClientRect(IntPtr hWnd, out Rect rect);
        [DllImport("user32.dll")] private static extern bool ClientToScreen(IntPtr hWnd, ref Point pt);

        [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left, Top, Right, Bottom; }
        [StructLayout(LayoutKind.Sequential)] private struct Point { public int X, Y; }

        private const int VK_BACK_ESC = 0x1B, VK_RETURN = 0x0D, VK_LEFT = 0x25, VK_UP = 0x26, VK_RIGHT = 0x27, VK_DOWN = 0x28,
                           VK_INSERT = 0x2D, VK_ADD = 0x6B, VK_SUBTRACT = 0x6D;

        private readonly Process _game;
        private readonly int _toggleVk;
        private readonly BridgeServer _bridge;

        private readonly MenuNav _nav = new();
        private readonly ConcurrentQueue<MenuKey> _keys = new();
        private readonly ConcurrentQueue<MenuState> _states = new();
        private volatile bool _open;          // read by the hook thread: the menu holds the keys
        private volatile bool _attached;      // the mod answered: it knows the launcher draws the menu
        private volatile bool _stop;
        private volatile int[] _blockedVks = Array.Empty<int>();   // keys the menu keeps from the game while their switch is off (hook thread reads)
        private double _openedAt = -100;   // when the menu was opened (the driver's clock)
        private readonly bool[] _down = new bool[256];   // hook thread: which keys are held (a repeat of Enter or Esc is not a press)

        public InGameMenuDriver(Process game, string menuKeyName, BridgeServer bridge)
        {
            _bridge = bridge;
            _game = game;
            _toggleVk = (menuKeyName ?? "").Trim().ToLowerInvariant() switch { "np_add" => VK_ADD, "np_subtract" => VK_SUBTRACT, _ => VK_INSERT };
        }

        /// <summary>The menu key the player chose (settings.json's MenuKey); Insert when there is none.</summary>
        public static string ReadMenuKey(string settingsFile)
        {
            try
            {
                if (!File.Exists(settingsFile)) return "insert";
                using var doc = JsonDocument.Parse(File.ReadAllText(settingsFile));
                return doc.RootElement.TryGetProperty("MenuKey", out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() ?? "insert" : "insert";
            }
            catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException) { return "insert"; }
        }

        public void Stop() => _stop = true;

        internal static MenuState ToMenuState(string game, BridgeMenu m) => new(game, m.Title.Length > 0 ? m.Title : "Mod menu", m.Blocked, m.Note,
            m.Items.Select(i => new MenuItem(i.Id, i.Group, i.Kind switch { "onoff" => MenuKind.OnOff, "choice" => MenuKind.Choice, "key" => MenuKind.Key, "action" => MenuKind.Action, _ => MenuKind.Info },
                i.On, i.Locked, i.Value, i.Label, i.Help, i.Gate, i.Disabled)).ToList(), m.Back);

        internal static void Trace(string text)
        {
            try
            {
                string file = Path.Combine(Path.GetTempPath(), "cairn-menu-trace.txt");
                if (File.Exists(file) && new FileInfo(file).Length > 200_000) File.Delete(file);
                File.AppendAllText(file, $"{DateTime.Now:HH:mm:ss.fff} {text}{Environment.NewLine}");
            }
            catch (Exception) { }
        }

        public async Task RunAsync()
        {
            Trace("RunAsync starts");
            string gameName = "the game";
            try { gameName = _game.ProcessName; } catch (Exception ex) { Trace("ProcessName failed: " + ex.Message); }
            Log.Information("in-game menu: watching {Game} (toggle key vk 0x{Vk:X2})", gameName, _toggleVk);
            OverlayWindow? overlay = null;
            KeyboardHook? hook = null;
            var clock = Stopwatch.StartNew();
            double nextAttach = 0, nextRefresh = 0, nextWindow = 0;
            Int32Rect client = default;
            bool gameFront = false;
            string subtitle = "";      // a partner's spoken line, shown until subtitleUntil (the driver's clock)
            double subtitleUntil = 0;
            // what the overlay was last told: it is told again only when something changed (the loop runs every 30 ms)
            (bool Vis, MenuState? State, int Sel, string Blocked, Int32Rect Rect, int Tick) lastMenu = (false, null, -2, "", default, 0);
            (bool Vis, string Text, Int32Rect Rect, int Tick) lastSub = (false, "", default, 0), lastNote = (false, "", default, 0);   // Tick: once a second the window is told again, which also puts it back above the game
            string notice = "";        // a notice of the mod (a partner joining), shown until the mod hides it or two minutes pass
            double noticeUntil = 0;

            // the game's bridge speaks for the mod: its menu, notices and spoken lines
            Action<BridgeIn> onBridge = msg =>
            {
                switch (msg)
                {
                    case BridgeMenu m: _states.Enqueue(ToMenuState(_bridge.Game, m)); break;
                    case BridgeNotice n: notice = n.Text; noticeUntil = n.Show ? clock.Elapsed.TotalSeconds + 120 : 0; break;
                    case BridgeSubtitle s: subtitle = s.Text; subtitleUntil = clock.Elapsed.TotalSeconds + s.Seconds; break;
                }
            };
            Action<string> onBridgeConnection = game =>
            {
                if (game.Length == 0) { _attached = false; return; }   // the bridge went away: the menu is the game's own until it is back
                Log.Information("in-game menu: the bridge of {Game} connected", game);
            };

            try
            {
                overlay = new OverlayWindow();
                hook = new KeyboardHook { Handler = OnKey };
                if (overlay.Dispatcher is { } d) await d.InvokeAsync(() => hook.Install()).Task;
                if (overlay.Dispatcher is null) { Log.Warning("in-game menu: the overlay could not start"); return; }

                _bridge.Received += onBridge;
                _bridge.ConnectionChanged += onBridgeConnection;

                while (!_game.HasExited && !_stop)
                {
                    double now = clock.Elapsed.TotalSeconds;

                    // the mod answers (menu messages): attached, and the picture is fresh
                    while (_states.TryDequeue(out var st))
                    {
                        _blockedVks = st.Items.Where(i => i.Gate.Length > 0 && i.On != true)
                            .Select(i => InGameMenuProtocol.VkOfGate(i.Gate)).Where(v => v is not null).Select(v => v!.Value).Distinct().ToArray();
                        if (!_attached) Log.Information("in-game menu: the mod answered ({Items} items): the launcher draws the menu from now on", st.Items.Count);
                        _attached = true;
                        _nav.Update(st);
                    }

                    // until the mod has answered, ask (it learns the launcher draws the menu, and says what it holds)
                    if (!_attached && _bridge.Connected && now >= nextAttach)
                    {
                        nextAttach = now + 3;
                        _bridge.Send(BridgeProtocol.Attach());
                    }

                    // keys
                    while (_keys.TryDequeue(out var key))
                    {
                        switch (key)
                        {
                            case MenuKey.Toggle:
                                if (_open) Close();
                                else Open(now, ref nextRefresh);
                                break;
                            case MenuKey.Back:
                                if (_nav.State.Submenu) _bridge.Send(BridgeProtocol.Back());   // a submenu of the game's own program: Back goes to it
                                else Close();
                                break;
                            case MenuKey.Up: _nav.Step(-1); break;
                            case MenuKey.Down: _nav.Step(1); break;
                            default:
                                if (_nav.Request(key) is { } req && BridgeProtocol.Do(req.Id, req.Verb, req.Dir) is { } m) _bridge.Send(m);
                                break;
                        }
                    }

                    // while it is open the picture is asked for once a second (the role, the partner and the connection change by themselves)
                    if (_open && now >= nextRefresh) { nextRefresh = now + (now - _openedAt < 2 ? 0.25 : 1); _bridge.Send(BridgeProtocol.AskState()); }

                    // the window: where the game is, and whether it is in front
                    if (now >= nextWindow)
                    {
                        nextWindow = now + 0.12;
                        _game.Refresh();
                        IntPtr hwnd = _game.MainWindowHandle;
                        gameFront = hwnd != IntPtr.Zero && IsForeground(_game.Id);
                        if (hwnd != IntPtr.Zero && GetClientRect(hwnd, out var r))
                        {
                            var origin = new Point { X = 0, Y = 0 };
                            ClientToScreen(hwnd, ref origin);
                            client = new Int32Rect(origin.X, origin.Y, r.Right - r.Left, r.Bottom - r.Top);
                        }
                    }

                    var state = _nav.State;
                    // the mod can say "a game menu" for a moment after a screen of the game closed: a reason is shown only if it lasts
                    bool settling = state.Blocked.Length > 0 && now - _openedAt < 1.5;
                    string blocked = state.Blocked.Length > 0 && !settling ? BlockedText(state.Blocked) : "";
                    if (settling) state = MenuState.Empty;
                    var menu = (Vis: _open && gameFront, State: (MenuState?)state, Sel: _nav.SelectedIndex, Blocked: blocked, Rect: client, Tick: (_open && gameFront) ? (int)now : 0);
                    if (!(menu.Vis == false && lastMenu.Vis == false) && (menu.Vis != lastMenu.Vis || !ReferenceEquals(menu.State, lastMenu.State) || menu.Sel != lastMenu.Sel
                        || menu.Blocked != lastMenu.Blocked || !menu.Rect.Equals(lastMenu.Rect) || menu.Tick != lastMenu.Tick))
                        overlay.Render(menu.Vis, state, menu.Sel, blocked, client);
                    lastMenu = menu;
                    var sub = (Vis: gameFront && now < subtitleUntil, Text: subtitle, Rect: client, Tick: (gameFront && now < subtitleUntil) ? (int)now : 0);
                    if (!(sub.Vis == false && lastSub.Vis == false) && (sub.Vis != lastSub.Vis || sub.Text != lastSub.Text || !sub.Rect.Equals(lastSub.Rect) || sub.Tick != lastSub.Tick))
                        overlay.RenderSubtitle(sub.Vis, subtitle, client);
                    lastSub = sub;
                    var note = (Vis: gameFront && now < noticeUntil, Text: notice, Rect: client, Tick: (gameFront && now < noticeUntil) ? (int)now : 0);
                    if (!(note.Vis == false && lastNote.Vis == false) && (note.Vis != lastNote.Vis || note.Text != lastNote.Text || !note.Rect.Equals(lastNote.Rect) || note.Tick != lastNote.Tick))
                        overlay.RenderNotice(note.Vis, notice, client);
                    lastNote = note;

                    await Task.Delay(30);
                }
            }
            catch (Exception ex)
            {
                Log.Warning(ex, "in-game menu: stopped by an error");
            }
            finally
            {
                _open = false;
                try { overlay?.Render(false, MenuState.Empty, -1, "", default); overlay?.RenderSubtitle(false, "", default); overlay?.RenderNotice(false, "", default); } catch (Exception) { }
                _bridge.Received -= onBridge;
                _bridge.ConnectionChanged -= onBridgeConnection;
                hook?.Dispose();
                overlay?.Dispose();
                Log.Information("in-game menu: stopped");
            }
        }

        private void Open(double now, ref double nextRefresh)
        {
            if (!_attached) return;
            _open = true;
            _nav.Show();
            _openedAt = now;
            _bridge.Send(BridgeProtocol.AskState());
            nextRefresh = now + 0.25;
        }

        private void Close()
        {
            _open = false;
            _nav.Hide();
        }

        // a load (or no world yet) is not a refusal: the menu is on its way, so it says so, in the calm colour
        private static string BlockedText(string why) => why is "a load" or "no world" ? "Loading the mod menu..." : $"The menu is not available now ({why}).";

        private static bool IsForeground(int pid)
        {
            IntPtr fg = GetForegroundWindow();
            if (fg == IntPtr.Zero) return false;
            GetWindowThreadProcessId(fg, out uint fgPid);
            return fgPid == (uint)pid;
        }

        private bool OnKey(int vk, bool down)
        {
            // a key whose switch in the menu is off never reaches the game (F3, the game's own fly mode, while "Free movement" is off)
            if (_attached && Array.IndexOf(_blockedVks, vk) >= 0 && IsForeground(_game.Id)) return true;
            MenuKey? key = vk == _toggleVk || (_toggleVk == VK_INSERT && vk == VK_INSERT) ? MenuKey.Toggle
                : vk == VK_UP ? MenuKey.Up : vk == VK_DOWN ? MenuKey.Down : vk == VK_LEFT ? MenuKey.Left : vk == VK_RIGHT ? MenuKey.Right
                : vk == VK_RETURN ? MenuKey.Enter : vk == VK_BACK_ESC ? MenuKey.Back : null;
            if (key is null || vk is < 0 or > 255) return false;
            if (!IsForeground(_game.Id)) { _down[vk] = false; return false; }

            if (key == MenuKey.Toggle)
            {
                if (!_attached) return false;   // the mod has not answered: its own key does its own thing
            }
            else if (!_open) return false;      // the arrows, Enter and Esc are the game's until the menu is open

            if (!down) { _down[vk] = false; return true; }   // the key's release is kept from the game too
            bool repeat = _down[vk];
            _down[vk] = true;
            // holding Up, Down, Left or Right repeats; Enter, Esc and the menu key are one press each
            if (repeat && key is MenuKey.Toggle or MenuKey.Enter or MenuKey.Back) return true;
            _keys.Enqueue(key.Value);
            return true;
        }
    }
}
