// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The Cairn launcher: the state and the logic behind the window (settings, the game library, downloads, the services) and, in `ui`, the window itself.

pub mod app;
pub mod app_dirs;
pub mod changelog;
pub mod dev;
pub mod downloads;
pub mod game_extras;
pub mod game_finder;
pub mod game_list;
pub mod games;
pub mod home_data;
pub mod logs;
pub mod mods;
pub mod open;
pub mod play;
pub mod preflight;
pub mod prefs;
pub mod runtime;
pub mod session;
pub mod settings;
pub mod setup_list;
pub mod steam_service;
pub mod ui;
pub mod update_check;
