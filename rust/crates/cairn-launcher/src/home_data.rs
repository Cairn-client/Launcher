// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! What Home shows about the player: the sessions played (`recent.json`) and the small rules for wording them.

use crate::changelog::ReleaseNote;
use cairn_protocol::release_version::{compare, ReleaseVersionComparison};
use cairn_setup::time_util;
use cairn_setup::version::Version;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct RecentSession {
    pub game_id: String,
    /// `host` or `join`.
    pub role: String,
    /// When it was last played, as an ISO 8601 UTC time (the way the C# launcher wrote it).
    pub last_utc: String,
    pub sessions: i32,
    pub minutes: i32,
    /// Who the player joined, for a joined session.
    pub with: String,
}

impl RecentSession {
    pub fn last_secs(&self) -> i64 {
        time_util::parse_utc(&self.last_utc).unwrap_or(0)
    }
}

pub struct RecentSessions {
    file: PathBuf,
    pub entries: Vec<RecentSession>,
}

impl RecentSessions {
    pub fn load(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let entries = fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&b)).ok())
            .unwrap_or_default();
        Self { file, entries }
    }

    /// The session played last.
    pub fn last(&self) -> Option<&RecentSession> {
        self.entries.iter().max_by_key(|e| e.last_secs())
    }

    pub fn of(&self, game_id: &str) -> Option<&RecentSession> {
        self.entries.iter().find(|e| e.game_id.eq_ignore_ascii_case(game_id))
    }

    pub fn record(&mut self, game_id: &str, role: &str, now: i64, with: &str) {
        if game_id.trim().is_empty() {
            return;
        }
        let i = match self.entries.iter().position(|e| e.game_id.eq_ignore_ascii_case(game_id)) {
            Some(i) => i,
            None => {
                self.entries.push(RecentSession {
                    game_id: game_id.to_string(),
                    ..Default::default()
                });
                self.entries.len() - 1
            }
        };
        let e = &mut self.entries[i];
        e.role = role.to_string();
        e.last_utc = time_util::format_utc(now);
        e.sessions += 1;
        e.with = if role == "join" { with.to_string() } else { String::new() };
        self.save();
    }

    pub fn add_playtime(&mut self, game_id: &str, minutes: i32) {
        if minutes < 1 {
            return;
        }
        if let Some(e) = self.entries.iter_mut().find(|e| e.game_id.eq_ignore_ascii_case(game_id)) {
            e.minutes += minutes;
            self.save();
        }
    }

    fn save(&self) {
        if let Some(dir) = self.file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(&self.entries) {
            let _ = fs::write(&self.file, text);
        }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }
}

/// How a game's mod stands for the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModState {
    NotAvailable,
    NotDownloaded,
    NotInstalled,
    UpdateAvailable,
    Installed,
}

/// The catalog's version is newer than the downloaded one (a version that cannot be compared counts as different).
pub fn is_newer_pack(catalog_version: &str, downloaded_version: &str) -> bool {
    match compare(downloaded_version, catalog_version) {
        ReleaseVersionComparison::LocalIsOlder => true,
        ReleaseVersionComparison::Ambiguous => catalog_version != downloaded_version,
        _ => false,
    }
}

pub fn mod_state_of(catalog_version: Option<&str>, downloaded_version: Option<&str>, installed: bool) -> ModState {
    match (downloaded_version, catalog_version) {
        (None, None) => ModState::NotAvailable,
        (None, Some(_)) => ModState::NotDownloaded,
        (Some(have), Some(cat)) if is_newer_pack(cat, have) => ModState::UpdateAvailable,
        _ => {
            if installed {
                ModState::Installed
            } else {
                ModState::NotInstalled
            }
        }
    }
}

/// "45 min", "2 h", "1 h 30 min".
pub fn played(minutes: i32) -> String {
    if minutes < 60 {
        format!("{minutes} min")
    } else if minutes % 60 == 0 || minutes >= 600 {
        format!("{} h", minutes / 60)
    } else {
        format!("{} h {} min", minutes / 60, minutes % 60)
    }
}

/// "just now", "5 minutes ago", "yesterday", "3 weeks ago".
pub fn ago(then: i64, now: i64) -> String {
    let secs = (now - then).max(0);
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match secs {
        s if s < 60 => "just now".into(),
        s if s < 3600 => plural(s / 60, "minute"),
        s if s < 86_400 => plural(s / 3600, "hour"),
        s if s < 2 * 86_400 => "yesterday".into(),
        s if s < 14 * 86_400 => format!("{} days ago", s / 86_400),
        s if s < 60 * 86_400 => plural(s / (7 * 86_400), "week"),
        s => plural(s / (30 * 86_400), "month"),
    }
}

/// The games that are hosting now and were not before (`None` before: the first look says nothing new).
pub fn newly_hosting(before: Option<&[String]>, now: &[String]) -> Vec<String> {
    let Some(before) = before else { return vec![] };
    let mut out: Vec<String> = Vec::new();
    for n in now {
        if !before.iter().any(|b| b.eq_ignore_ascii_case(n)) && !out.iter().any(|o| o.eq_ignore_ascii_case(n)) {
            out.push(n.clone());
        }
    }
    out
}

/// The newest stable release that is newer than `current`.
pub fn newer_release<'a>(current: &str, releases: Option<&'a [ReleaseNote]>) -> Option<&'a ReleaseNote> {
    let cur = try_version(current)?;
    releases?
        .iter()
        .filter(|r| !r.prerelease)
        .filter_map(|r| try_version(&r.tag).map(|v| (v, r)))
        .filter(|(v, _)| *v > cur)
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, r)| r)
}

fn try_version(s: &str) -> Option<Version> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let s = s.split(['+', '-']).next().unwrap_or("");
    Version::parse(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cairn-home-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sessions_are_recorded_counted_and_kept_in_the_csharp_shape() {
        let d = temp("recent");
        let f = d.join("recent.json");
        let mut r = RecentSessions::load(&f);
        assert!(r.last().is_none());
        r.record("noita", "host", 1_791_801_000, "");
        r.record("noita", "join", 1_791_801_600, "Ann");
        r.record("kcd2", "host", 1_791_700_000, "");
        r.add_playtime("noita", 45);
        r.add_playtime("noita", 0);
        r.add_playtime("ghost", 10);
        assert_eq!(r.last().unwrap().game_id, "noita");
        let n = r.of("NOITA").unwrap();
        assert_eq!((n.sessions, n.minutes, n.role.as_str(), n.with.as_str()), (2, 45, "join", "Ann"));
        assert_eq!(r.of("kcd2").unwrap().with, "");
        let text = fs::read_to_string(&f).unwrap();
        assert!(
            text.contains("\"GameId\": \"noita\"") && text.contains("\"LastUtc\": \"2026-10-12T10:40:00Z\""),
            "{text}"
        );
        let again = RecentSessions::load(&f);
        assert_eq!(again.entries.len(), 2);
        r.record("  ", "host", 0, "");
        assert_eq!(r.entries.len(), 2);
        // a file the C# launcher wrote (a fractional second)
        fs::write(
            &f,
            r#"[ { "GameId": "a", "Role": "host", "LastUtc": "2026-10-12T10:30:00.1234567Z", "Sessions": 3, "Minutes": 10, "With": "" } ]"#,
        )
        .unwrap();
        assert_eq!(RecentSessions::load(&f).last().unwrap().last_secs(), 1_791_801_000);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_state_of_a_mod_follows_the_catalog_and_the_download() {
        assert_eq!(mod_state_of(None, None, false), ModState::NotAvailable);
        assert_eq!(mod_state_of(Some("1.0.0"), None, false), ModState::NotDownloaded);
        assert_eq!(mod_state_of(Some("1.1.0"), Some("1.0.0"), true), ModState::UpdateAvailable);
        assert_eq!(mod_state_of(Some("1.0.0"), Some("1.0.0"), true), ModState::Installed);
        assert_eq!(mod_state_of(Some("1.0.0"), Some("1.0.0"), false), ModState::NotInstalled);
        assert_eq!(mod_state_of(None, Some("1.0.0"), true), ModState::Installed);
        assert!(is_newer_pack("1.10.0", "1.9.0") && !is_newer_pack("1.9.0", "1.10.0") && !is_newer_pack("1.0.0", "1.0.0"));
        assert!(is_newer_pack("nightly", "1.0.0") && !is_newer_pack("x", "x")); // cannot compare: different is newer
    }

    #[test]
    fn times_are_worded_the_way_the_home_page_reads() {
        assert_eq!(
            (played(45).as_str(), played(120).as_str(), played(90).as_str(), played(700).as_str()),
            ("45 min", "2 h", "1 h 30 min", "11 h")
        );
        let now = 10_000_000;
        assert_eq!(ago(now - 30, now), "just now");
        assert_eq!(ago(now - 60, now), "1 minute ago");
        assert_eq!(ago(now - 3 * 3600, now), "3 hours ago");
        assert_eq!(ago(now - 30 * 3600, now), "yesterday");
        assert_eq!(ago(now - 5 * 86_400, now), "5 days ago");
        assert_eq!(ago(now - 21 * 86_400, now), "3 weeks ago");
        assert_eq!(ago(now - 90 * 86_400, now), "3 months ago");
        assert_eq!(ago(now + 100, now), "just now");
    }

    #[test]
    fn a_game_that_started_hosting_is_noticed_once() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(newly_hosting(None, &s(&["noita"])).is_empty());
        assert_eq!(newly_hosting(Some(&s(&["noita"])), &s(&["Noita", "kcd2", "KCD2"])), vec!["kcd2"]);
        assert!(newly_hosting(Some(&s(&[])), &s(&[])).is_empty());
    }

    #[test]
    fn the_newest_stable_release_newer_than_this_one_is_found() {
        let note = |tag: &str, pre: bool| ReleaseNote {
            tag: tag.into(),
            published: None,
            prerelease: pre,
            items: vec![],
        };
        let rel = vec![
            note("0.3.38", false),
            note("0.4.0-beta", true),
            note("0.3.40", false),
            note("v0.3.39", false),
        ];
        assert_eq!(newer_release("0.3.38", Some(&rel)).unwrap().tag, "0.3.40");
        assert!(newer_release("0.3.40+abc", Some(&rel)).is_none());
        assert!(newer_release("garbage", Some(&rel)).is_none() && newer_release("0.1.0", None).is_none());
    }
}
