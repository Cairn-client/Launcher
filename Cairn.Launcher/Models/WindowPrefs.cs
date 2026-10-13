// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.IO;
using System.Text.Json;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// Where the window was left: size, position and whether it was maximised. <c>%LOCALAPPDATA%\Cairn\window.json</c>;
    /// missing or unreadable means the defaults (maximised, centred).
    /// </summary>
    public sealed class WindowPrefs
    {
        public int Width { get; set; } = 1600;
        public int Height { get; set; } = 900;
        public int X { get; set; }
        public int Y { get; set; }
        public bool Maximized { get; set; } = true;

        public static string DefaultFile => Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "window.json");

        /// <summary>The smallest the window can be made: the pages are laid out down to this size, and none is meant for less.</summary>
        public const int MinWidth = 1200, MinHeight = 720;   // in layout pixels (what the page sees), not screen pixels

        [System.Runtime.InteropServices.DllImport("user32.dll")] private static extern uint GetDpiForSystem();

        /// <summary>The screen's scale (1.0 at 96 dpi, 1.5 at 150 %): a window of 1100 screen pixels on a 150 % screen shows only 733 pixels of page.</summary>
        public static double Scale()
        {
            try { return Math.Clamp(GetDpiForSystem() / 96.0, 1.0, 4.0); }
            catch (Exception ex) when (ex is DllNotFoundException or EntryPointNotFoundException) { return 1.0; }
        }

        /// <summary>The smallest window in screen pixels on this screen.</summary>
        public static (int Width, int Height) MinSize() { double s = Scale(); return ((int)Math.Ceiling(MinWidth * s), (int)Math.Ceiling(MinHeight * s)); }

        /// <summary>A saved size that is too small, or a position that no screen shows any more, is not worth restoring.</summary>
        public bool IsUsable(Func<int, int, bool> pointIsOnAScreen) =>
            Width >= MinSize().Width && Height >= MinSize().Height && Width <= 10000 && Height <= 10000 && pointIsOnAScreen(X + 40, Y + 20);

        public static WindowPrefs? Load(string? file = null)
        {
            try
            {
                file ??= DefaultFile;
                if (File.Exists(file)) return JsonSerializer.Deserialize<WindowPrefs>(File.ReadAllText(file));
            }
            catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException) { /* the defaults */ }
            return null;
        }

        public void Save(string? file = null)
        {
            try
            {
                file ??= DefaultFile;
                Directory.CreateDirectory(Path.GetDirectoryName(file)!);
                File.WriteAllText(file, JsonSerializer.Serialize(this));
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { /* best effort */ }
        }
    }
}
