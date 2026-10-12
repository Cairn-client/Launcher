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

        /// <summary>A saved size that is too small, or a position that no screen shows any more, is not worth restoring.</summary>
        public bool IsUsable(Func<int, int, bool> pointIsOnAScreen) =>
            Width >= 800 && Height >= 500 && Width <= 10000 && Height <= 10000 && pointIsOnAScreen(X + 40, Y + 20);

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
