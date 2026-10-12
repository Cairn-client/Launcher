// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;

namespace Cairn.Launcher.Models
{
    /// <summary>
    /// The folders players pointed Cairn at for games it cannot find through Steam (a GOG or Epic copy, a manual install):
    /// <c>%LOCALAPPDATA%\Cairn\game-paths.json</c>, game id to install folder. Kept apart from settings.json, which
    /// belongs to the player and is only written for the player's own changes.
    /// </summary>
    public sealed class GamePathStore
    {
        private readonly string _file;
        private readonly object _gate = new();
        private Dictionary<string, string> _paths = new(StringComparer.OrdinalIgnoreCase);

        public GamePathStore(string? file = null)
        {
            _file = file ?? Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cairn", "game-paths.json");
            Load();
        }

        public string? Get(string gameId)
        {
            lock (_gate) return _paths.TryGetValue(gameId, out var p) ? p : null;
        }

        /// <summary>Remembers the folder for a game; an empty path forgets it. Returns false when it could not be saved.</summary>
        public bool Set(string gameId, string? folder)
        {
            lock (_gate)
            {
                if (string.IsNullOrWhiteSpace(folder)) _paths.Remove(gameId); else _paths[gameId] = folder;
                try
                {
                    Directory.CreateDirectory(Path.GetDirectoryName(_file)!);
                    File.WriteAllText(_file, JsonSerializer.Serialize(_paths, new JsonSerializerOptions { WriteIndented = true }));
                    return true;
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return false; }
            }
        }

        /// <summary>
        /// The game's install folder when <paramref name="path"/> is its executable or a folder that contains it
        /// (looked for one level down, where launchers often put it), otherwise null.
        /// </summary>
        public static string? ResolveInstallFolder(string? path, string exeName)
        {
            if (string.IsNullOrWhiteSpace(path)) return null;
            try
            {
                if (File.Exists(path))
                    return string.Equals(Path.GetFileName(path), exeName, StringComparison.OrdinalIgnoreCase) ? Path.GetDirectoryName(path) : null;
                if (!Directory.Exists(path)) return null;
                if (File.Exists(Path.Combine(path, exeName))) return path;
                foreach (var sub in Directory.EnumerateDirectories(path))
                    if (File.Exists(Path.Combine(sub, exeName))) return sub;
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
            return null;
        }

        private void Load()
        {
            try
            {
                if (!File.Exists(_file)) return;
                var read = JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(_file));
                if (read is not null) _paths = new Dictionary<string, string>(read, StringComparer.OrdinalIgnoreCase);
            }
            catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException) { /* unreadable: start empty */ }
        }
    }
}
