// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! A game's ready check, as data: up to 12 lines, each one of a fixed set of kinds the launcher knows how to check.

use crate::catalog_limits as lim;
use serde_json::Value;

pub const MAX_RULES: usize = 12;

pub mod kinds {
    pub const STEAM_RUNNING: &str = "steam-running";
    pub const GAME_FOUND: &str = "game-found";
    pub const MOD_INSTALLED: &str = "mod-installed";
    pub const STEAM_APP: &str = "steam-app";
    pub const GAME_FILE: &str = "game-file";
    pub const FREE_DISK: &str = "free-disk";

    pub fn is_known(kind: &str) -> bool {
        matches!(kind, STEAM_RUNNING | GAME_FOUND | MOD_INSTALLED | STEAM_APP | GAME_FILE | FREE_DISK)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupRule {
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub app_id: u32,
    pub path: String,
    pub mb: i32,
    pub optional: bool,
}

impl SetupRule {
    fn simple(kind: &str, title: &str, detail: &str) -> Self {
        Self {
            kind: kind.into(),
            title: title.into(),
            detail: detail.into(),
            app_id: 0,
            path: String::new(),
            mb: 0,
            optional: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupState {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupResult {
    pub title: String,
    pub state: SetupState,
    pub detail: String,
    pub kind: String,
}

/// What the checks ask of the machine.
pub trait GameSetupHost {
    fn steam_running(&self) -> bool;
    fn game_found(&self) -> bool;
    fn mod_installed(&self) -> bool;
    fn steam_app_installed(&self, app_id: u32) -> bool;
    fn game_file_exists(&self, relative_path: &str) -> bool;
    /// Free space on the game's drive in MB; `None` when unknown.
    fn free_disk_mb(&self) -> Option<i64>;
}

/// A game with no `setup` of its own is checked for Steam, the game and the mod.
pub fn defaults() -> Vec<SetupRule> {
    vec![
        SetupRule::simple(
            kinds::STEAM_RUNNING,
            "Steam",
            "Start Steam and sign in: the game starts through it.",
        ),
        SetupRule::simple(
            kinds::GAME_FOUND,
            "Game",
            "Cairn has not found the game on this PC. Use Locate game.",
        ),
        SetupRule::simple(
            kinds::MOD_INSTALLED,
            "Mod",
            "Download the mod first: it is installed in the game's folder.",
        ),
    ]
}

pub fn evaluate(rules: &[SetupRule], host: &dyn GameSetupHost) -> Vec<SetupResult> {
    let mut results = Vec::new();
    for r in rules.iter().take(MAX_RULES) {
        if !kinds::is_known(&r.kind) {
            continue; // a check this launcher does not know is skipped, not failed
        }
        let (ok, good, title) = match r.kind.as_str() {
            kinds::STEAM_RUNNING => (host.steam_running(), "Steam is running.", "Steam"),
            kinds::GAME_FOUND => (host.game_found(), "Found on this PC.", "Game"),
            kinds::MOD_INSTALLED => (host.mod_installed(), "Installed in the game's folder.", "Mod"),
            kinds::STEAM_APP => (host.steam_app_installed(r.app_id), "Installed in your Steam library.", "Steam app"),
            kinds::GAME_FILE => (host.game_found() && host.game_file_exists(&r.path), "There.", "File"),
            // an unknown size is not a failure
            _ => (
                host.free_disk_mb().is_none_or(|mb| mb >= r.mb as i64),
                "Enough free space.",
                "Disk space",
            ),
        };
        let shown = if r.title.is_empty() { title.to_string() } else { r.title.clone() };
        results.push(if ok {
            SetupResult {
                title: shown,
                state: SetupState::Ok,
                detail: good.into(),
                kind: r.kind.clone(),
            }
        } else {
            SetupResult {
                title: shown,
                state: if r.optional { SetupState::Warn } else { SetupState::Fail },
                detail: if r.detail.is_empty() {
                    "Not so yet.".into()
                } else {
                    r.detail.clone()
                },
                kind: r.kind.clone(),
            }
        });
    }
    results
}

/// The `setup` array of a `game.json`; `None` when any line is not valid (a line the launcher does not know makes the whole file invalid).
pub fn parse(root: &Value) -> Option<Vec<SetupRule>> {
    let mut rules = Vec::new();
    for e in lim::items(root, "setup") {
        if !e.is_object() || rules.len() >= MAX_RULES {
            return None;
        }
        let kind = lim::str_of(e, "kind");
        if !kinds::is_known(&kind) {
            return None;
        }
        let title = lim::text(&lim::str_of(e, "title"), 60);
        let detail = lim::text(&lim::str_of(e, "detail"), 200);
        let app_id = lim::u32_of(e, "appId").unwrap_or(0);
        let path = lim::str_of(e, "path");
        let mb = e.get("mb").and_then(Value::as_i64).and_then(|n| i32::try_from(n).ok()).unwrap_or(0);
        let optional = lim::flag(e, "optional");
        // each kind needs its parameter, and a path stays inside the game's folder
        if kind == kinds::STEAM_APP && app_id == 0 {
            return None;
        }
        if kind == kinds::GAME_FILE && !is_game_file_path(&path) {
            return None;
        }
        if kind == kinds::FREE_DISK && !(1..=1_000_000).contains(&mb) {
            return None;
        }
        rules.push(SetupRule {
            title,
            detail,
            app_id,
            path: if kind == kinds::GAME_FILE { path } else { String::new() },
            mb: if kind == kinds::FREE_DISK { mb } else { 0 },
            optional,
            kind,
        });
    }
    Some(rules)
}

/// A path inside the game's folder: forward slashes, plain segments, at most six, never `..` or a drive.
pub fn is_game_file_path(s: &str) -> bool {
    if s.is_empty() || s.len() > 160 || s.contains("..") || s.contains(':') || s.contains('\\') || s.starts_with('/') || s.ends_with('/') {
        return false;
    }
    let parts: Vec<&str> = s.split('/').collect();
    parts.len() <= 6
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b' '))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Host {
        steam: bool,
        found: bool,
        installed: bool,
        apps: Vec<u32>,
        files: Vec<&'static str>,
        disk: Option<i64>,
    }
    impl GameSetupHost for Host {
        fn steam_running(&self) -> bool {
            self.steam
        }
        fn game_found(&self) -> bool {
            self.found
        }
        fn mod_installed(&self) -> bool {
            self.installed
        }
        fn steam_app_installed(&self, app_id: u32) -> bool {
            self.apps.contains(&app_id)
        }
        fn game_file_exists(&self, p: &str) -> bool {
            self.files.contains(&p)
        }
        fn free_disk_mb(&self) -> Option<i64> {
            self.disk
        }
    }
    fn host() -> Host {
        Host {
            steam: true,
            found: true,
            installed: true,
            apps: vec![5],
            files: vec!["a/b.dll"],
            disk: Some(500),
        }
    }

    #[test]
    fn the_default_checks_are_steam_the_game_and_the_mod() {
        let r = evaluate(&defaults(), &host());
        assert!(r.iter().all(|x| x.state == SetupState::Ok));
        assert_eq!(r.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), vec!["Steam", "Game", "Mod"]);
        let mut h = host();
        h.steam = false;
        let r = evaluate(&defaults(), &h);
        assert_eq!(r[0].state, SetupState::Fail);
        assert!(r[0].detail.contains("Start Steam"));
    }

    #[test]
    fn an_optional_failure_is_a_warning_and_an_unknown_disk_size_passes() {
        let rules = parse(&json!({"setup": [
            {"kind": "game-file", "path": "x/y.dll", "optional": true, "detail": "Install the loader."},
            {"kind": "free-disk", "mb": 900},
            {"kind": "steam-app", "appId": 7, "title": "Tools"}
        ]}))
        .unwrap();
        let r = evaluate(&rules, &host());
        assert_eq!((r[0].state, r[0].detail.as_str()), (SetupState::Warn, "Install the loader."));
        assert_eq!(r[1].state, SetupState::Fail);
        assert_eq!(
            (r[2].state, r[2].title.as_str(), r[2].detail.as_str()),
            (SetupState::Fail, "Tools", "Not so yet.")
        );
        let mut h = host();
        h.disk = None;
        assert_eq!(evaluate(&rules, &h)[1].state, SetupState::Ok);
    }

    #[test]
    fn a_game_file_needs_the_game_to_be_found() {
        let rules = parse(&json!({"setup": [{"kind": "game-file", "path": "a/b.dll"}]})).unwrap();
        assert_eq!(evaluate(&rules, &host())[0].state, SetupState::Ok);
        let mut h = host();
        h.found = false;
        assert_eq!(evaluate(&rules, &h)[0].state, SetupState::Fail);
    }

    #[test]
    fn a_line_the_launcher_does_not_know_makes_the_file_invalid() {
        assert!(parse(&json!({"setup": [{"kind": "run-program"}]})).is_none());
        assert!(parse(&json!({"setup": [{"kind": "steam-app"}]})).is_none());
        assert!(parse(&json!({"setup": [{"kind": "free-disk", "mb": 0}]})).is_none());
        assert!(parse(&json!({"setup": [{"kind": "game-file", "path": "../x.dll"}]})).is_none());
        assert!(parse(&json!({"setup": [{"kind": "game-file", "path": "C:/x.dll"}]})).is_none());
        assert!(parse(&json!({"setup": ["steam-running"]})).is_none());
        let too_many: Vec<Value> = (0..13).map(|_| json!({"kind": "steam-running"})).collect();
        assert!(parse(&json!({"setup": too_many})).is_none());
        assert_eq!(parse(&json!({})).unwrap().len(), 0);
    }

    #[test]
    fn game_file_paths_stay_inside_the_game() {
        assert!(is_game_file_path("Bin32/FarCry.exe") && is_game_file_path("hollow_knight_Data/Managed/Assembly-CSharp.dll.v"));
        assert!(!is_game_file_path("") && !is_game_file_path(".hidden/x") && !is_game_file_path("a//b") && !is_game_file_path("a/b/"));
        assert!(!is_game_file_path("a/b/c/d/e/f/g.dll"));
    }
}
