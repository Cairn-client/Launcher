// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! `settings.json`: read key by key, written key by key. A value that does not read keeps its default instead of resetting every setting;
//! only what the player changed is written (through [`cairn_protocol::settings_json`], which keeps the rest of the file as it was); a key
//! the file lacks is added with its default. The file is the C# launcher's, with the same keys.

use cairn_protocol::settings_json::{self, Edit, Outcome, SettingsResult};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AppSettings {
    pub game_path: String,
    pub dll_path: String,
    pub agent_path: String,
    pub inject_delay_seconds: i32,
    pub master_server_url: String,
    pub server_info_port: i32,
    pub dice_ipc_port: i32,
    pub version_ipc_port: i32,
    pub language: String,
    pub relay_path: String,
    pub master_server_path: String,
    pub host_port: i32,
    /// Friends on Cairn: tell the master server who you are and what you play.
    pub presence_enabled: bool,
    pub notify_friends: bool,
    /// `home` or `library`: the page shown when Cairn starts.
    pub start_page: String,
    pub confirm_quit: bool,
    /// gold, blue, green, pink, violet or red.
    pub accent_color: String,
    pub reduce_motion: bool,
    pub slideshow_auto: bool,
    pub tray_notices: bool,
    pub friend_status: bool,
    /// game id -> the mod version the player went back from.
    pub held_back_mods: BTreeMap<String, String>,
    pub host_allow_steam: bool,
    pub show_host_address: bool,
    pub dlss5_enabled: bool,
    pub connect_gate: bool,
    pub menu_takeover: bool,
    pub start_save_path: String,
    /// game id -> `stable` or `beta`.
    pub mod_channels: BTreeMap<String, String>,
    pub game_options: BTreeMap<String, BTreeMap<String, bool>>,
    pub relay_reuse: bool,
    pub steam_app_id: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            game_path: String::new(),
            dll_path: String::new(),
            agent_path: String::new(),
            inject_delay_seconds: 20,
            master_server_url: "http://127.0.0.1:5100".into(),
            server_info_port: 5273,
            dice_ipc_port: 5901,
            version_ipc_port: 5902,
            language: "en".into(),
            relay_path: String::new(),
            master_server_path: "MasterServer\\CairnMasterServer.exe".into(),
            host_port: 7778,
            presence_enabled: true,
            notify_friends: true,
            start_page: "home".into(),
            confirm_quit: true,
            accent_color: "gold".into(),
            reduce_motion: false,
            slideshow_auto: true,
            tray_notices: true,
            friend_status: true,
            held_back_mods: BTreeMap::new(),
            host_allow_steam: true,
            show_host_address: false,
            dlss5_enabled: false,
            connect_gate: true,
            menu_takeover: true,
            start_save_path: "start-save".into(),
            mod_channels: BTreeMap::new(),
            game_options: BTreeMap::new(),
            relay_reuse: true,
            steam_app_id: 2_429_020,
        }
    }
}

impl AppSettings {
    /// The mod channel the player follows for a game: `beta` or `stable`.
    pub fn channel_of(&self, game_id: &str) -> &str {
        self.mod_channels
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(game_id))
            .map(|(_, v)| v.as_str())
            .filter(|c| *c == "beta")
            .unwrap_or("stable")
    }

    fn as_map(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        }
    }
}

/// What a save did.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveResult {
    pub result: SettingsResult,
    /// The keys the player changed and that were written.
    pub set: Vec<String>,
    /// The keys the file lacked and that were added with their value.
    pub filled: Vec<String>,
}

impl SaveResult {
    pub fn ok(&self) -> bool {
        self.result.ok()
    }
    pub fn wrote(&self) -> bool {
        matches!(self.result.outcome, Outcome::Written | Outcome::Created)
    }
}

/// Reads and writes `settings.json`.
pub struct SettingsStore {
    path: PathBuf,
    migrate: Option<fn(&mut AppSettings)>,
    /// Each key as the launcher holds it "not changed by the player" (as loaded and corrected, or as last written).
    known: Map<String, Value>,
    /// Each key as the file itself held it when last read or written (absent: the file lacked it, or it did not read).
    disk: Map<String, Value>,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>, migrate: Option<fn(&mut AppSettings)>) -> Self {
        Self {
            path: path.into(),
            migrate,
            known: AppSettings::default().as_map(),
            disk: Map::new(),
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.path
    }

    /// Reads the file key by key; `problems` gets a line for each thing that could not be read.
    pub fn load(&mut self, problems: &mut Vec<String>) -> AppSettings {
        let disk = read_root(&self.path, problems);
        self.disk.clear();
        let (mut s, read) = apply_from(disk.as_ref(), problems);
        for name in read {
            if let Some(v) = s.as_map().get(&name) {
                self.disk.insert(name, v.clone());
            }
        }
        if let Some(m) = self.migrate {
            m(&mut s);
        }
        self.known = s.as_map();
        s
    }

    /// The keys that differ from what was loaded or last written.
    pub fn changed(&self, now: &AppSettings) -> Vec<String> {
        let map = now.as_map();
        map.iter()
            .filter(|(k, v)| self.known.get(*k) != Some(*v))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Writes only what the player changed (limited to `only` when given) and adds the keys the file lacks.
    pub fn save(&mut self, now: &AppSettings, only: Option<&[&str]>) -> SaveResult {
        let changed: Vec<String> = self
            .changed(now)
            .into_iter()
            .filter(|k| only.is_none_or(|o| o.contains(&k.as_str())))
            .collect();
        if changed.is_empty() {
            return SaveResult {
                result: SettingsResult {
                    outcome: Outcome::Unchanged,
                    why: "nothing the player changed".into(),
                    changed: vec![],
                },
                set: vec![],
                filled: vec![],
            };
        }
        // read again: another program may have written the file since it was loaded
        let mut problems = Vec::new();
        let disk = read_root(&self.path, &mut problems);
        if let Some(why) = problems.first() {
            return SaveResult {
                result: SettingsResult {
                    outcome: Outcome::Refused,
                    why: why.clone(),
                    changed: vec![],
                },
                set: vec![],
                filled: vec![],
            };
        }
        let map = now.as_map();
        let (mut edits, mut set, mut filled) = (Vec::new(), Vec::new(), Vec::new());
        for (name, value) in &map {
            if changed.contains(name) {
                edits.push(Edit::set(name, value.clone()));
                set.push(name.clone());
            } else if disk.as_ref().is_none_or(|d| !d.contains_key(name)) {
                edits.push(Edit::fill(name, value.clone()));
                filled.push(name.clone());
            }
        }
        let r = settings_json::update(&self.path, &edits, true);
        if r.ok() {
            for name in &set {
                self.known.insert(name.clone(), map[name].clone());
                self.disk.insert(name.clone(), map[name].clone());
            }
            for name in &filled {
                self.disk.insert(name.clone(), map[name].clone());
            }
        }
        let filled = if r.ok() { filled } else { vec![] };
        SaveResult { result: r, set, filled }
    }

    /// Takes what another program wrote to the file since it was read, except a key the player has a pending change to. Returns the keys taken.
    pub fn refresh(&mut self, now: &mut AppSettings) -> Vec<String> {
        let mut problems = Vec::new();
        let Some(disk) = read_root(&self.path, &mut problems) else {
            return vec![];
        };
        let (raw, read) = apply_from(Some(&disk), &mut Vec::new());
        let mut corrected = raw.clone();
        if let Some(m) = self.migrate {
            m(&mut corrected);
        }
        let (raw_map, corrected_map) = (raw.as_map(), corrected.as_map());
        let mut now_map = now.as_map();
        let mut taken = Vec::new();
        for name in read {
            let on_disk = raw_map[&name].clone();
            if self.disk.get(&name) == Some(&on_disk) {
                continue; // the file did not change it
            }
            self.disk.insert(name.clone(), on_disk);
            if now_map.get(&name) != self.known.get(&name) {
                continue; // the player's pending change wins
            }
            now_map.insert(name.clone(), corrected_map[&name].clone());
            self.known.insert(name.clone(), corrected_map[&name].clone());
            taken.push(name);
        }
        if let Ok(s) = serde_json::from_value(Value::Object(now_map)) {
            *now = s;
        }
        taken
    }

    /// Puts back what was loaded for every key the player changed and did not save. Returns the keys reverted.
    pub fn revert(&self, now: &mut AppSettings) -> Vec<String> {
        let changed = self.changed(now);
        let mut map = now.as_map();
        for name in &changed {
            if let Some(v) = self.known.get(name) {
                map.insert(name.clone(), v.clone());
            }
        }
        if let Ok(s) = serde_json::from_value(Value::Object(map)) {
            *now = s;
        }
        changed
    }

    /// Takes the current value of the keys as the new baseline (they were handled some other way).
    pub fn accept(&mut self, now: &AppSettings, keys: &[&str]) {
        let map = now.as_map();
        for k in keys {
            if let Some(v) = map.get(*k) {
                self.known.insert((*k).to_string(), v.clone());
            }
        }
    }

    /// The file has no value for the key (missing, null, or an empty string); an unreadable file is never "unset".
    pub fn is_unset_on_disk(&self, key: &str) -> bool {
        let mut problems = Vec::new();
        let disk = read_root(&self.path, &mut problems);
        if !problems.is_empty() {
            return false;
        }
        match disk.as_ref().and_then(|d| d.get(key)) {
            None | Some(Value::Null) => true,
            Some(Value::String(s)) => s.trim().is_empty(),
            Some(_) => false,
        }
    }
}

/// The root object of the file; `None` when there is none (and nothing in `problems`) or it could not be read (and `problems` says why).
fn read_root(path: &Path, problems: &mut Vec<String>) -> Option<Map<String, Value>> {
    if !path.exists() {
        return None;
    }
    match settings_json::read(path) {
        Some(m) => Some(m),
        None => {
            problems
                .push("settings.json could not be read (not one JSON object); the defaults are used and the file is left as it is".into());
            None
        }
    }
}

/// Applies each key of the file that reads as its type; returns the settings and the names read.
fn apply_from(disk: Option<&Map<String, Value>>, problems: &mut Vec<String>) -> (AppSettings, Vec<String>) {
    let defaults = AppSettings::default().as_map();
    let mut current = defaults.clone();
    let mut read = Vec::new();
    if let Some(disk) = disk {
        for (name, default) in &defaults {
            let Some(v) = disk.get(name) else { continue };
            if v.is_null() {
                continue; // null: not set, the default stays
            }
            let mut candidate = current.clone();
            candidate.insert(name.clone(), v.clone());
            if serde_json::from_value::<AppSettings>(Value::Object(candidate.clone())).is_ok() && same_shape(default, v) {
                current = candidate;
                read.push(name.clone());
            } else {
                problems.push(format!(
                    "{name} does not read as its type; the default is used and the file keeps its value"
                ));
            }
        }
    }
    let s = serde_json::from_value(Value::Object(current)).unwrap_or_default();
    (s, read)
}

/// serde is lenient where .NET is not (a number for a string is refused by both; a float for an int is refused by .NET): the kind must match.
fn same_shape(default: &Value, v: &Value) -> bool {
    match (default, v) {
        (Value::String(_), Value::String(_)) | (Value::Bool(_), Value::Bool(_)) | (Value::Object(_), Value::Object(_)) => true,
        (Value::Number(_), Value::Number(n)) => n.is_i64() || n.is_u64(),
        _ => false,
    }
}

/// The launcher's corrections of known stale values (memory only: the file keeps what the player has until the player changes it).
pub fn migrate_stale(s: &mut AppSettings) {
    let defaults = AppSettings::default();
    const STALE_URLS: [&str; 2] = ["http://localhost:5000/servers/servers_list", "http://localhost:5100"];
    if STALE_URLS.contains(&s.master_server_url.as_str()) {
        s.master_server_url = defaults.master_server_url;
    }
    if s.master_server_path == "CairnMasterServer.exe" {
        s.master_server_path = defaults.master_server_path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cairn-settings-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_a_first_save_writes_nothing_until_something_changes() {
        let d = temp("first");
        let mut store = SettingsStore::new(d.join("settings.json"), Some(migrate_stale));
        let mut problems = vec![];
        let s = store.load(&mut problems);
        assert_eq!(s, AppSettings::default());
        assert!(problems.is_empty());
        let r = store.save(&s, None);
        assert_eq!(r.result.outcome, Outcome::Unchanged);
        assert!(!d.join("settings.json").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn only_what_the_player_changed_is_written_and_the_rest_of_the_file_stays() {
        let d = temp("edit");
        let f = d.join("settings.json");
        fs::write(&f, "{\n  // my note\n  \"AccentColor\": \"blue\",\n  \"Unknown\": [1, 2]\n}\n").unwrap();
        let mut store = SettingsStore::new(&f, None);
        let mut s = store.load(&mut vec![]);
        assert_eq!(s.accent_color, "blue");
        s.reduce_motion = true;
        assert_eq!(store.changed(&s), vec!["ReduceMotion"]);
        let r = store.save(&s, None);
        assert!(r.wrote() && r.set == vec!["ReduceMotion"]);
        let text = fs::read_to_string(&f).unwrap();
        assert!(
            text.contains("// my note") && text.contains("\"Unknown\": [1, 2]") && text.contains("\"ReduceMotion\": true"),
            "{text}"
        );
        assert!(r.filled.contains(&"HostPort".to_string())); // the keys the file lacked are added with their defaults
        let again = SettingsStore::new(&f, None).load(&mut vec![]);
        assert!(again.reduce_motion && again.accent_color == "blue");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_value_that_does_not_read_keeps_its_default_and_the_file_keeps_its_value() {
        let d = temp("bad");
        let f = d.join("settings.json");
        fs::write(&f, r#"{ "HostPort": "seventy", "ConfirmQuit": false, "InjectDelaySeconds": 1.5 }"#).unwrap();
        let mut store = SettingsStore::new(&f, None);
        let mut problems = vec![];
        let s = store.load(&mut problems);
        assert_eq!((s.host_port, s.confirm_quit, s.inject_delay_seconds), (7778, false, 20));
        assert_eq!(problems.len(), 2);
        let mut s2 = s.clone();
        s2.language = "fr".into();
        store.save(&s2, None);
        assert!(fs::read_to_string(&f).unwrap().contains("\"HostPort\": \"seventy\"")); // not written over
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_that_is_not_an_object_is_never_written_over() {
        let d = temp("garbage");
        let f = d.join("settings.json");
        fs::write(&f, "[not settings]").unwrap();
        let mut store = SettingsStore::new(&f, None);
        let mut problems = vec![];
        let mut s = store.load(&mut problems);
        assert_eq!(problems.len(), 1);
        s.reduce_motion = true;
        let r = store.save(&s, None);
        assert_eq!(r.result.outcome, Outcome::Refused);
        assert_eq!(fs::read_to_string(&f).unwrap(), "[not settings]");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stale_values_are_corrected_in_memory_only() {
        let d = temp("stale");
        let f = d.join("settings.json");
        fs::write(&f, r#"{ "MasterServerUrl": "http://localhost:5000/servers/servers_list" }"#).unwrap();
        let mut store = SettingsStore::new(&f, Some(migrate_stale));
        let s = store.load(&mut vec![]);
        assert_eq!(s.master_server_url, AppSettings::default().master_server_url);
        assert!(store.changed(&s).is_empty());
        assert!(fs::read_to_string(&f).unwrap().contains("localhost:5000"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_change_made_by_another_program_is_taken_unless_the_player_has_a_pending_one() {
        let d = temp("refresh");
        let f = d.join("settings.json");
        fs::write(&f, r#"{ "Language": "en", "HostPort": 7778 }"#).unwrap();
        let mut store = SettingsStore::new(&f, None);
        let mut s = store.load(&mut vec![]);
        fs::write(&f, r#"{ "Language": "fr", "HostPort": 9000 }"#).unwrap();
        s.host_port = 8000; // pending
        let taken = store.refresh(&mut s);
        assert_eq!(taken, vec!["Language"]);
        assert_eq!((s.language.as_str(), s.host_port), ("fr", 8000));
        assert_eq!(store.revert(&mut s), vec!["HostPort"]);
        assert_eq!(s.host_port, 7778);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_unset_check_and_the_channel_of_a_game() {
        let d = temp("unset");
        let f = d.join("settings.json");
        fs::write(&f, r#"{ "GamePath": "  ", "AgentPath": "x", "RelayPath": null }"#).unwrap();
        let store = SettingsStore::new(&f, None);
        assert!(store.is_unset_on_disk("GamePath") && store.is_unset_on_disk("RelayPath") && store.is_unset_on_disk("Missing"));
        assert!(!store.is_unset_on_disk("AgentPath"));
        let mut s = AppSettings::default();
        s.mod_channels.insert("Noita".into(), "beta".into());
        assert_eq!((s.channel_of("noita"), s.channel_of("kcd2")), ("beta", "stable"));
        let _ = fs::remove_dir_all(&d);
    }
}
