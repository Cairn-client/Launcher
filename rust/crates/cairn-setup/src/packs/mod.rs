// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Mod packs: one zip per mod, attached to the mod repository's releases, with a contract (`pack.json`) and a manifest of every file. The
//! launcher downloads a pack, checks every byte of it against what the catalog and the manifest say, and keeps it in its own store.

pub mod contract;
pub mod index;
pub mod installer;
pub mod store;

pub use contract::{PackContract, PackDependency, PackKinds};
pub use index::{PackIndex, PackInfo};
pub use store::{InstalledPack, PackStore};

/// The outcome of a pack operation: whether it worked and what to tell the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackResult {
    pub ok: bool,
    pub message: String,
}

impl PackResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}
