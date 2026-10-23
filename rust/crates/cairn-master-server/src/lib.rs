// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The Cairn master server: the server list the launcher reads, relays that announce themselves over a WebSocket (the connection *is* the
//! listing), and friend presence.

pub mod config;
pub mod hub;
pub mod log;
pub mod registry;
pub mod server;
