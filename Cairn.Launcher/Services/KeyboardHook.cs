// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Cairn.Launcher.Services
{
    /// <summary>
    /// A low-level keyboard hook for the in-game menu: it sees the menu's keys while the game is the window in front and can keep
    /// them from the game (the arrows and Enter are also the game's walking and interact keys, so while the menu is open they are
    /// the menu's alone). It must be installed on a thread that pumps messages, and the handler has to be quick.
    /// </summary>
    internal sealed class KeyboardHook : IDisposable
    {
        private delegate IntPtr LowLevelProc(int nCode, IntPtr wParam, IntPtr lParam);

        [StructLayout(LayoutKind.Sequential)]
        private struct KbdLlHookStruct { public uint VkCode, ScanCode, Flags, Time; public IntPtr ExtraInfo; }

        [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr SetWindowsHookEx(int idHook, LowLevelProc lpfn, IntPtr hMod, uint dwThreadId);
        [DllImport("user32.dll", SetLastError = true)] private static extern bool UnhookWindowsHookEx(IntPtr hhk);
        [DllImport("user32.dll")] private static extern IntPtr CallNextHookEx(IntPtr hhk, int nCode, IntPtr wParam, IntPtr lParam);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr GetModuleHandle(string? name);

        private const int WH_KEYBOARD_LL = 13;
        private const int WM_KEYDOWN = 0x100, WM_KEYUP = 0x101, WM_SYSKEYDOWN = 0x104, WM_SYSKEYUP = 0x105;
        private const uint LLKHF_INJECTED = 0x10;

        private readonly LowLevelProc _proc;
        private IntPtr _hook;

        /// <summary>(virtual key, key down) -> true to keep the key from the game. Runs on the hook's thread.</summary>
        public Func<int, bool, bool>? Handler { get; set; }

        public KeyboardHook() { _proc = Callback; }

        public bool Install()
        {
            if (_hook != IntPtr.Zero) return true;
            using var cur = Process.GetCurrentProcess();
            using var mod = cur.MainModule;
            _hook = SetWindowsHookEx(WH_KEYBOARD_LL, _proc, GetModuleHandle(mod?.ModuleName), 0);
            return _hook != IntPtr.Zero;
        }

        private IntPtr Callback(int nCode, IntPtr wParam, IntPtr lParam)
        {
            if (nCode >= 0)
            {
                int msg = wParam.ToInt32();
                if (msg is WM_KEYDOWN or WM_KEYUP or WM_SYSKEYDOWN or WM_SYSKEYUP)
                {
                    var k = Marshal.PtrToStructure<KbdLlHookStruct>(lParam);
                    if ((k.Flags & LLKHF_INJECTED) == 0)   // never our own or another program's synthetic keys
                    {
                        try
                        {
                            if (Handler?.Invoke((int)k.VkCode, msg is WM_KEYDOWN or WM_SYSKEYDOWN) == true) return (IntPtr)1;
                        }
                        catch (Exception) { /* a handler fault must never take the keyboard with it */ }
                    }
                }
            }
            return CallNextHookEx(_hook, nCode, wParam, lParam);
        }

        public void Dispose()
        {
            if (_hook != IntPtr.Zero) { UnhookWindowsHookEx(_hook); _hook = IntPtr.Zero; }
        }
    }
}
