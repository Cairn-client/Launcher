// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Whether a game is on this PC, and where: the folder the player pointed at, or the Steam library that holds it. Steam is asked once per game,
//! not at every frame.

use crate::games::GameProfile;
use crate::prefs::GamePathStore;
use cairn_setup::setup_host::{RealSetupHost, SetupHost, SteamInstall};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct GameFinder {
    /// Folders the player pointed at for games outside Steam (a GOG or Epic copy, a manual install).
    pub paths: GamePathStore,
    steam: Option<SteamInstall>,
    found: HashMap<String, Option<PathBuf>>,
    host: Box<dyn SetupHost + Send>,
}

impl GameFinder {
    pub fn new(paths: GamePathStore) -> Self {
        Self::with_host(paths, Box::new(RealSetupHost))
    }

    pub fn with_host(paths: GamePathStore, host: Box<dyn SetupHost + Send>) -> Self {
        Self {
            paths,
            steam: None,
            found: HashMap::new(),
            host,
        }
    }

    /// Steam's install and libraries, read once.
    pub fn steam(&mut self) -> &SteamInstall {
        let host = &self.host;
        self.steam.get_or_insert_with(|| SteamInstall::locate(host.as_ref(), None))
    }

    /// Forget what was found (the player installed a game, or pointed at one).
    pub fn forget(&mut self, game_id: &str) {
        self.found.remove(&game_id.to_lowercase());
    }

    /// Look at Steam again too.
    pub fn forget_all(&mut self) {
        self.found.clear();
        self.steam = None;
    }

    /// The game's folder: the game's path in the settings for a game whose executable is a setting; else the folder the player pointed at; else
    /// the Steam library it is fully installed in.
    pub fn find(&mut self, profile: &GameProfile, settings_game_path: &str) -> Option<PathBuf> {
        let key = profile.id.to_lowercase();
        if profile.exe_is_setting {
            let exe = Path::new(settings_game_path.trim());
            return (!settings_game_path.trim().is_empty() && exe.is_file())
                .then(|| exe.parent().map(Path::to_path_buf))
                .flatten();
        }
        if let Some(cached) = self.found.get(&key) {
            return cached.clone();
        }
        let found = self.look(profile);
        self.found.insert(key, found.clone());
        found
    }

    fn look(&mut self, profile: &GameProfile) -> Option<PathBuf> {
        if let Some(folder) = self
            .paths
            .get(&profile.id)
            .and_then(|p| GamePathStore::resolve_install_folder(Some(p), &profile.default_exe_name))
        {
            return Some(folder);
        }
        let m = self.steam().manifest(profile.steam_app_id)?;
        (m.fully_installed() && m.install_path().is_dir()).then(|| m.install_path())
    }

    /// The game's executable in its folder, when both are known.
    pub fn exe_of(&mut self, profile: &GameProfile, settings_game_path: &str) -> Option<PathBuf> {
        if profile.exe_is_setting {
            let exe = PathBuf::from(settings_game_path.trim());
            return (!settings_game_path.trim().is_empty() && exe.is_file()).then_some(exe);
        }
        if profile.default_exe_name.is_empty() {
            return None;
        }
        let exe = self.find(profile, settings_game_path)?.join(&profile.default_exe_name);
        exe.is_file().then_some(exe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::GameRegistry;
    use std::fs;

    struct FakeSteam(Option<PathBuf>);
    impl SetupHost for FakeSteam {
        fn steam_root_from_registry(&self) -> Option<PathBuf> {
            self.0.clone()
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cairn-finder-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn profile(id: &str, app: u32, exe: &str, exe_is_setting: bool) -> GameProfile {
        let mut p = GameRegistry::new().get("kcd2").unwrap().clone();
        p.id = id.into();
        p.steam_app_id = app;
        p.default_exe_name = exe.into();
        p.exe_is_setting = exe_is_setting;
        p
    }

    #[test]
    fn a_game_is_found_in_a_steam_library_only_when_it_is_fully_installed() {
        let d = temp("steam");
        let steam = d.join("Steam");
        fs::create_dir_all(steam.join("steamapps").join("common").join("Noita")).unwrap();
        fs::write(
            steam.join("steamapps").join("appmanifest_881100.acf"),
            "\"AppState\" { \"installdir\" \"Noita\" \"StateFlags\" \"4\" }",
        )
        .unwrap();
        fs::write(
            steam.join("steamapps").join("appmanifest_5.acf"),
            "\"AppState\" { \"installdir\" \"Half\" \"StateFlags\" \"1026\" }",
        )
        .unwrap();
        fs::create_dir_all(steam.join("steamapps").join("common").join("Half")).unwrap();
        let mut f = GameFinder::with_host(GamePathStore::load(d.join("paths.json")), Box::new(FakeSteam(Some(steam.clone()))));
        assert_eq!(
            f.find(&profile("noita", 881_100, "noita.exe", false), ""),
            Some(steam.join("steamapps").join("common").join("Noita"))
        );
        assert_eq!(f.find(&profile("half", 5, "half.exe", false), ""), None); // still downloading
        assert_eq!(f.find(&profile("none", 6, "x.exe", false), ""), None);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_folder_the_player_pointed_at_wins_and_forgetting_looks_again() {
        let d = temp("pointed");
        let game = d.join("GOG").join("Game");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("game.exe"), "x").unwrap();
        let mut paths = GamePathStore::load(d.join("paths.json"));
        let mut f = GameFinder::with_host(GamePathStore::load(d.join("paths.json")), Box::new(FakeSteam(None)));
        let p = profile("g", 9, "game.exe", false);
        assert_eq!(f.find(&p, ""), None);
        paths.set("g", Some(game.to_str().unwrap()));
        assert_eq!(f.find(&p, ""), None); // remembered "not found" until told to look again
        f.paths = GamePathStore::load(d.join("paths.json"));
        f.forget("G");
        assert_eq!(f.find(&p, ""), Some(game.clone()));
        assert_eq!(f.exe_of(&p, ""), Some(game.join("game.exe")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_game_whose_exe_is_a_setting_is_found_by_that_setting() {
        let d = temp("setting");
        let exe = d.join("KingdomCome.exe");
        fs::write(&exe, "x").unwrap();
        let mut f = GameFinder::with_host(GamePathStore::load(d.join("paths.json")), Box::new(FakeSteam(None)));
        let p = profile("kcd2", 1, "KingdomCome.exe", true);
        assert_eq!(f.find(&p, exe.to_str().unwrap()), Some(d.clone()));
        assert_eq!(f.exe_of(&p, exe.to_str().unwrap()), Some(exe.clone()));
        assert_eq!(f.find(&p, ""), None);
        assert_eq!(f.find(&p, d.join("missing.exe").to_str().unwrap()), None);
        let _ = fs::remove_dir_all(&d);
    }
}
