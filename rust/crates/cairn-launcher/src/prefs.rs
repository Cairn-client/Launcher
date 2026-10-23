// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The small files the launcher keeps for the player in `%LOCALAPPDATA%\Cairn`: how the game list is shown (`list-prefs.json`), the window's place
//! (`window.json`) and where the player's games are (`game-paths.json`). The names and shapes are the C# launcher's.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub mod filter {
    pub const ALL: u32 = 0;
    pub const ON_THIS_PC: u32 = 1;
    pub const MOD_INSTALLED: u32 = 2;
    pub const FAVORITES: u32 = 4;
}

/// What the folders of the game list group by (stored as the number it has in the C# launcher).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping {
    #[default]
    Title,
    Developer,
    Franchise,
}

impl Grouping {
    pub fn key(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Developer => "Developer",
            Self::Franchise => "Franchise",
        }
    }
}

impl Serialize for Grouping {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(match self {
            Self::Title => 0,
            Self::Developer => 1,
            Self::Franchise => 2,
        })
    }
}

impl<'de> Deserialize<'de> for Grouping {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match u8::deserialize(d)? {
            1 => Self::Developer,
            2 => Self::Franchise,
            _ => Self::Title,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct GameListPrefs {
    /// Bit flags: see [`filter`].
    pub filter: u32,
    pub grouping: Grouping,
    pub descending: bool,
    pub recent_first: bool,
    /// Folders the player closed, as `<grouping>:<header>`.
    pub collapsed: Vec<String>,
    pub favorites: Vec<String>,
    /// The games in the library; `None` until the player has chosen (then every game is in it).
    pub library: Option<Vec<String>>,
    pub tour_seen: bool,
    pub dismissed_notices: Vec<String>,
}

impl Default for GameListPrefs {
    fn default() -> Self {
        Self {
            filter: filter::ALL,
            grouping: Grouping::Title,
            descending: false,
            recent_first: false,
            collapsed: vec![],
            favorites: vec![],
            library: None,
            tour_seen: false,
            dismissed_notices: vec![],
        }
    }
}

impl GameListPrefs {
    pub fn load(file: &Path) -> Self {
        fs::read(file)
            .ok()
            .and_then(|b| serde_json::from_slice(strip_bom(&b)).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> bool {
        write_json(file, self)
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.iter().any(|f| f.eq_ignore_ascii_case(id))
    }

    /// Adds or removes a favourite; returns whether it is one now.
    pub fn toggle_favorite(&mut self, id: &str) -> bool {
        if let Some(i) = self.favorites.iter().position(|f| f.eq_ignore_ascii_case(id)) {
            self.favorites.remove(i);
            false
        } else {
            self.favorites.push(id.to_string());
            true
        }
    }

    /// Whether the game is in the player's library (every game is, until the player has chosen).
    pub fn in_library(&self, id: &str) -> bool {
        self.library.as_ref().is_none_or(|l| l.iter().any(|g| g.eq_ignore_ascii_case(id)))
    }

    /// Takes a game out of the library; `all` is every game there is, to start the list from when the player had not chosen yet.
    pub fn remove_from_library(&mut self, id: &str, all: &[String]) {
        let list = self.library.get_or_insert_with(|| all.to_vec());
        list.retain(|g| !g.eq_ignore_ascii_case(id));
    }

    pub fn add_to_library(&mut self, id: &str) {
        if let Some(list) = &mut self.library {
            if !list.iter().any(|g| g.eq_ignore_ascii_case(id)) {
                list.push(id.to_string());
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct WindowPrefs {
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub maximized: bool,
}

impl Default for WindowPrefs {
    fn default() -> Self {
        Self {
            width: 1600,
            height: 900,
            x: 0,
            y: 0,
            maximized: true,
        }
    }
}

impl WindowPrefs {
    /// In layout pixels (what the page sees), not screen pixels.
    pub const MIN_WIDTH: i32 = 1200;
    pub const MIN_HEIGHT: i32 = 720;

    pub fn load(file: &Path) -> Option<Self> {
        fs::read(file).ok().and_then(|b| serde_json::from_slice(strip_bom(&b)).ok())
    }

    pub fn save(&self, file: &Path) {
        let _ = write_json(file, self);
    }

    /// A saved window is used only when it is big enough, not absurd, and its top left is on a screen (`on_a_screen` says).
    pub fn is_usable(&self, scale: f64, on_a_screen: impl Fn(i32, i32) -> bool) -> bool {
        let (min_w, min_h) = (
            (Self::MIN_WIDTH as f64 * scale).ceil() as i32,
            (Self::MIN_HEIGHT as f64 * scale).ceil() as i32,
        );
        self.width >= min_w
            && self.height >= min_h
            && self.width <= 10_000
            && self.height <= 10_000
            && on_a_screen(self.x + 40, self.y + 20)
    }
}

/// Where the player's games are (`game-paths.json`): game id -> folder.
pub struct GamePathStore {
    file: PathBuf,
    paths: BTreeMap<String, String>,
}

impl GamePathStore {
    pub fn load(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let paths: BTreeMap<String, String> = fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(strip_bom(&b)).ok())
            .unwrap_or_default();
        Self { file, paths }
    }

    pub fn get(&self, game_id: &str) -> Option<&str> {
        self.paths
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(game_id))
            .map(|(_, v)| v.as_str())
    }

    /// Remembers (or, with an empty folder, forgets) where a game is; `false` when it could not be written.
    pub fn set(&mut self, game_id: &str, folder: Option<&str>) -> bool {
        self.paths.retain(|k, _| !k.eq_ignore_ascii_case(game_id));
        if let Some(f) = folder.filter(|f| !f.trim().is_empty()) {
            self.paths.insert(game_id.to_string(), f.to_string());
        }
        write_json(&self.file, &self.paths)
    }

    /// The game's folder for what the player chose: the exe itself, its folder, or a folder with the game one level down. `None` when it is not there.
    pub fn resolve_install_folder(path: Option<&str>, exe_name: &str) -> Option<PathBuf> {
        let p = Path::new(path.filter(|p| !p.trim().is_empty())?);
        if p.is_file() {
            return p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(exe_name))
                .then(|| p.parent().map(Path::to_path_buf))
                .flatten();
        }
        if !p.is_dir() {
            return None;
        }
        if p.join(exe_name).is_file() {
            return Some(p.to_path_buf());
        }
        fs::read_dir(p)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|sub| sub.is_dir() && sub.join(exe_name).is_file())
    }
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b)
}

fn write_json<T: Serialize>(file: &Path, value: &T) -> bool {
    let Ok(text) = serde_json::to_string_pretty(value) else {
        return false;
    };
    if let Some(dir) = file.parent() {
        if fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    fs::write(file, text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cairn-prefs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_list_prefs_read_what_the_csharp_launcher_wrote() {
        let d = temp("list");
        let f = d.join("list-prefs.json");
        fs::write(&f, "\u{feff}{ \"Filter\": 5, \"Grouping\": 2, \"Descending\": true, \"RecentFirst\": false, \"Collapsed\": [\"Title:A\"], \"Favorites\": [\"noita\"], \"Library\": [\"kcd2\"], \"TourSeen\": true, \"DismissedNotices\": [\"n1\"] }").unwrap();
        let p = GameListPrefs::load(&f);
        assert_eq!(
            (p.filter, p.grouping, p.descending, p.tour_seen),
            (filter::ON_THIS_PC | filter::FAVORITES, Grouping::Franchise, true, true)
        );
        assert!(p.is_favorite("NOITA") && p.in_library("KCD2") && !p.in_library("noita"));
        assert!(p.save(&f));
        let text = fs::read_to_string(&f).unwrap();
        assert!(
            text.contains("\"Filter\": 5") && text.contains("\"Grouping\": 2") && text.contains("\"Library\""),
            "{text}"
        );
        assert_eq!(GameListPrefs::load(&f), p);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_or_damaged_prefs_give_the_defaults() {
        let d = temp("damaged");
        assert_eq!(GameListPrefs::load(&d.join("none.json")), GameListPrefs::default());
        fs::write(d.join("bad.json"), "{ nope").unwrap();
        assert_eq!(GameListPrefs::load(&d.join("bad.json")), GameListPrefs::default());
        assert!(GameListPrefs::default().in_library("anything")); // every game, until the player chose
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn favourites_and_the_library_change() {
        let mut p = GameListPrefs::default();
        assert!(p.toggle_favorite("noita") && p.is_favorite("Noita"));
        assert!(!p.toggle_favorite("NOITA") && p.favorites.is_empty());
        let all = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        p.remove_from_library("B", &all);
        assert_eq!(p.library, Some(vec!["a".to_string(), "c".to_string()]));
        assert!(!p.in_library("b") && p.in_library("c"));
        p.add_to_library("b");
        p.add_to_library("b");
        assert_eq!(p.library.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn a_saved_window_is_used_only_when_it_is_sane() {
        let w = WindowPrefs {
            width: 1400,
            height: 800,
            x: 100,
            y: 50,
            maximized: false,
        };
        assert!(w.is_usable(1.0, |_, _| true));
        assert!(!w.is_usable(1.0, |_, _| false)); // off every screen
        assert!(!w.is_usable(2.0, |_, _| true)); // smaller than the minimum at this scale
        assert!(!WindowPrefs {
            width: 20_000,
            ..w.clone()
        }
        .is_usable(1.0, |_, _| true));
        let d = temp("window");
        w.save(&d.join("window.json"));
        assert_eq!(WindowPrefs::load(&d.join("window.json")), Some(w));
        assert_eq!(WindowPrefs::load(&d.join("none.json")), None);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_game_paths_are_remembered_and_resolved() {
        let d = temp("paths");
        let mut store = GamePathStore::load(d.join("game-paths.json"));
        assert!(store.get("noita").is_none());
        assert!(store.set("Noita", Some("D:\\Games\\Noita")));
        assert_eq!(
            GamePathStore::load(d.join("game-paths.json")).get("NOITA"),
            Some("D:\\Games\\Noita")
        );
        assert!(store.set("noita", None));
        assert!(store.get("noita").is_none());

        let game = d.join("Steam").join("Noita");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("noita.exe"), "x").unwrap();
        let exe = game.join("noita.exe");
        assert_eq!(
            GamePathStore::resolve_install_folder(Some(exe.to_str().unwrap()), "noita.exe"),
            Some(game.clone())
        );
        assert_eq!(
            GamePathStore::resolve_install_folder(Some(game.to_str().unwrap()), "noita.exe"),
            Some(game.clone())
        );
        assert_eq!(
            GamePathStore::resolve_install_folder(Some(d.join("Steam").to_str().unwrap()), "noita.exe"),
            Some(game.clone())
        ); // one level down
        assert_eq!(
            GamePathStore::resolve_install_folder(Some(exe.to_str().unwrap()), "other.exe"),
            None
        );
        assert_eq!(GamePathStore::resolve_install_folder(Some("/no/such"), "noita.exe"), None);
        assert_eq!(GamePathStore::resolve_install_folder(None, "noita.exe"), None);
        let _ = fs::remove_dir_all(&d);
    }
}
