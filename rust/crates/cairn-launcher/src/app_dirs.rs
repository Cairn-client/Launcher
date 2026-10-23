// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The folders the launcher keeps its files in. They are the C# launcher's, so a player who switches keeps their games, favourites and packs.

use std::path::PathBuf;

/// `%LOCALAPPDATA%\Cairn`: packs, the catalog cache, `list-prefs.json`, `game-paths.json`, `window.json`.
pub fn local_data() -> PathBuf {
    cairn_setup::packs::PackStore::default_root()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("Cairn"))
}

/// `%APPDATA%\Cairn`: the log files.
pub fn app_folder() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("Cairn")
}

pub fn list_prefs_file() -> PathBuf {
    local_data().join("list-prefs.json")
}
pub fn game_paths_file() -> PathBuf {
    local_data().join("game-paths.json")
}
pub fn window_file() -> PathBuf {
    local_data().join("window.json")
}
pub fn packs_root() -> PathBuf {
    local_data().join("packs")
}
pub fn catalog_cache() -> PathBuf {
    local_data().join("cache").join("catalog")
}
pub fn game_info_cache() -> PathBuf {
    local_data().join("game-info")
}
