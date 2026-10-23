// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Where Steam and the games are, the pack store, the catalog, and the clients of the pack program.

pub mod app_manifest;
pub mod bridge_protocol;
pub mod catalog;
pub mod catalog_client;
pub mod catalog_limits;
pub mod external_install;
pub mod game_options;
pub mod game_setup;
pub mod http;
pub mod mod_install;
pub mod pack_session;
pub mod pack_setup;
pub mod packs;
pub mod process_runner;
pub mod redact;
pub mod setup_host;
pub mod steam_identity;
pub mod steam_profile_status;
pub mod time_util;
pub mod vdf;
pub mod version;
