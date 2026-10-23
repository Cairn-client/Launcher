// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The API shared by the launcher and the master server (`MasterApi`, `PresenceApi`), the release version comparison, and the
//! editor that changes single keys of a `settings.json` without disturbing the rest of the file.

pub mod master_api;
pub mod presence_api;
pub mod release_version;
pub mod settings_json;
