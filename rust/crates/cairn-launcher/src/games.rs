// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The games the launcher knows: the catalog (read from the network, kept on disk) turned into profiles, plus the one game it ships with so a
//! first start with no network is not empty.

use cairn_setup::catalog::{Component, FeatureGroup, GameEntry, GameList, GameManifest, STEAM_APP};
use cairn_setup::catalog_client::{CatalogGame, CatalogSnapshot};
use cairn_setup::game_options::GameOption;
use cairn_setup::game_setup::SetupRule;
use std::collections::BTreeMap;

/// The address prefix of the pictures that ship inside the launcher.
pub const SEED_BASE: &str = "seed://";

const SEED_LIST: &str = include_str!("seed/games.seed.json");
const SEED_KCD2: &str = include_str!("seed/kcd2.game.json");

/// The bytes of a picture that ships with the launcher, by file name.
pub fn seed_picture(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "kcd2.jpg" => include_bytes!("seed/kcd2.jpg"),
        "kcd2-hero.jpg" => include_bytes!("seed/kcd2-hero.jpg"),
        "kcd2-logo.png" => include_bytes!("seed/kcd2-logo.png"),
        "kcd2-poster.jpg" => include_bytes!("seed/kcd2-poster.jpg"),
        "kcd2-shot-1.jpg" => include_bytes!("seed/kcd2-shot-1.jpg"),
        "kcd2-shot-2.jpg" => include_bytes!("seed/kcd2-shot-2.jpg"),
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamRequirement {
    pub name: String,
    pub app_id: u32,
    pub has_store_page: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameProfile {
    pub id: String,
    pub display_name: String,
    pub developer: String,
    pub franchise: String,
    pub steam_app_id: u32,
    /// The app whose store page describes the game, when it is not the one it runs from.
    pub store_app_id: u32,
    pub default_exe_name: String,
    pub mod_repo: String,
    /// The game's executable is a launcher setting (a build only the mod's setup finds).
    pub exe_is_setting: bool,
    pub background_image: String,
    pub logo_image: String,
    pub hero_image: String,
    pub poster_image: String,
    pub screenshots: Vec<String>,
    pub description: String,
    pub accent: String,
    pub is_supported: bool,
    pub requirements: Vec<SteamRequirement>,
    pub features: Vec<FeatureGroup>,
    pub host_steps: Vec<String>,
    pub join_steps: Vec<String>,
    pub own_session: bool,
    pub setup: Vec<SetupRule>,
    pub options: Vec<GameOption>,
    pub content_packs: Vec<String>,
    pub is_installed: bool,
    pub install_path: String,
}

impl GameProfile {
    /// The Steam page of the game: the store app when it has one of its own.
    pub fn page_app_id(&self) -> u32 {
        if self.store_app_id != 0 {
            self.store_app_id
        } else {
            self.steam_app_id
        }
    }

    pub fn from_catalog(g: &CatalogGame, catalog: &GameList) -> Self {
        let m: &GameManifest = &g.manifest;
        let requirements = m
            .resolve_requirements(catalog)
            .into_iter()
            .filter(|c: &Component| c.kind == STEAM_APP)
            .map(|c| SteamRequirement {
                name: c.name,
                app_id: c.app_id,
                has_store_page: c.has_store_page,
            })
            .collect();
        Self {
            id: m.id.clone(),
            display_name: m.name.clone(),
            developer: m.developer.clone(),
            franchise: m.franchise.clone(),
            steam_app_id: m.steam_app_id,
            store_app_id: m.store_app_id,
            default_exe_name: m.exe.clone(),
            mod_repo: g.entry.repo.clone(),
            exe_is_setting: g.entry.exe_setting,
            background_image: g.picture(&m.images.background),
            logo_image: g.picture(&m.images.logo),
            hero_image: g.picture(&m.images.hero),
            poster_image: g.picture(&m.images.poster),
            screenshots: m.images.screenshots.iter().map(|s| g.picture(s)).collect(),
            description: m.description.clone(),
            accent: m.accent.clone(),
            is_supported: m.supported,
            requirements,
            features: m.features.clone(),
            host_steps: m.host_steps.clone(),
            join_steps: m.join_steps.clone(),
            own_session: m.own_session,
            setup: m.setup.clone(),
            options: m.options.clone(),
            content_packs: m.content.clone(),
            is_installed: false,
            install_path: String::new(),
        }
    }
}

/// The games, from the catalog.
pub struct GameRegistry {
    profiles: BTreeMap<String, GameProfile>,
    catalog: GameList,
}

impl Default for GameRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl GameRegistry {
    /// Starts with the game the launcher ships with.
    pub fn new() -> Self {
        let mut r = Self {
            profiles: BTreeMap::new(),
            catalog: GameList::default(),
        };
        if let Some(seed) = seed() {
            r.apply(&seed);
        }
        r
    }

    pub fn catalog(&self) -> &GameList {
        &self.catalog
    }

    /// Takes a catalog. A game that was installed stays so; a listed game whose `game.json` could not be had keeps what it was (or what the
    /// seed knows) instead of being dropped.
    pub fn apply(&mut self, snapshot: &CatalogSnapshot) {
        let before = std::mem::take(&mut self.profiles);
        for g in &snapshot.games {
            let mut p = GameProfile::from_catalog(g, &snapshot.list);
            if let Some(old) = before.get(&p.id.to_lowercase()) {
                p.install_path = old.install_path.clone();
                p.is_installed = old.is_installed;
            }
            self.profiles.insert(p.id.to_lowercase(), p);
        }
        let missing: Vec<&GameEntry> = snapshot
            .list
            .games
            .iter()
            .filter(|e| !self.profiles.contains_key(&e.id.to_lowercase()))
            .collect();
        for entry in missing {
            if let Some(kept) = before.get(&entry.id.to_lowercase()) {
                self.profiles.insert(entry.id.to_lowercase(), kept.clone());
            } else if let Some(from_seed) = seed_game(entry) {
                self.profiles
                    .insert(entry.id.to_lowercase(), GameProfile::from_catalog(&from_seed, &snapshot.list));
            }
        }
        self.catalog = snapshot.list.clone();
    }

    pub fn get(&self, id: &str) -> Option<&GameProfile> {
        self.profiles.get(&id.to_lowercase())
    }

    /// Every game, by name.
    pub fn all(&self) -> Vec<&GameProfile> {
        let mut v: Vec<&GameProfile> = self.profiles.values().collect();
        v.sort_by_key(|p| p.display_name.to_lowercase());
        v
    }

    /// Records where a game is installed (or that it is not).
    pub fn mark_installed(&mut self, id: &str, install_path: Option<&str>) -> bool {
        let Some(p) = self.profiles.get_mut(&id.to_lowercase()) else {
            return false;
        };
        p.install_path = install_path.unwrap_or("").to_string();
        p.is_installed = !p.install_path.trim().is_empty();
        true
    }
}

/// The game the launcher ships with.
pub fn seed() -> Option<CatalogSnapshot> {
    let list = GameList::parse(SEED_LIST)?;
    let games = list
        .games
        .iter()
        .filter_map(|e| {
            let json = (e.id == "kcd2").then_some(SEED_KCD2)?;
            GameManifest::parse(json).map(|m| CatalogGame {
                entry: e.clone(),
                manifest: m,
                base_url: SEED_BASE.into(),
            })
        })
        .collect();
    Some(CatalogSnapshot {
        list,
        games,
        from_cache: true,
    })
}

fn seed_game(entry: &GameEntry) -> Option<CatalogGame> {
    seed()?
        .games
        .into_iter()
        .find(|g| g.entry.id.eq_ignore_ascii_case(&entry.id))
        .map(|g| CatalogGame { entry: entry.clone(), ..g })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(list: &str, games: &[(&str, &str)]) -> CatalogSnapshot {
        let list = GameList::parse(list).unwrap();
        let games = games
            .iter()
            .map(|(id, json)| CatalogGame {
                entry: list.games.iter().find(|e| e.id == *id).unwrap().clone(),
                manifest: GameManifest::parse(json).unwrap(),
                base_url: "https://x.test/".into(),
            })
            .collect();
        CatalogSnapshot {
            list,
            games,
            from_cache: false,
        }
    }

    #[test]
    fn the_launcher_starts_with_the_game_it_ships_with() {
        let r = GameRegistry::new();
        let kcd2 = r.get("KCD2").expect("the seed game");
        assert_eq!(kcd2.display_name, "Kingdom Come: Deliverance II");
        assert_eq!(
            (kcd2.steam_app_id, kcd2.store_app_id, kcd2.page_app_id()),
            (2_429_020, 1_771_300, 1_771_300)
        );
        assert!(kcd2.exe_is_setting && kcd2.background_image == "seed://kcd2.jpg");
        assert!(seed_picture("kcd2.jpg").is_some() && seed_picture("kcd2-logo.png").is_some() && seed_picture("nope.png").is_none());
        assert!(kcd2.requirements.is_empty()); // the seed names none
        for name in [
            "kcd2.jpg",
            "kcd2-hero.jpg",
            "kcd2-logo.png",
            "kcd2-poster.jpg",
            "kcd2-shot-1.jpg",
            "kcd2-shot-2.jpg",
        ] {
            assert!(seed_picture(name).is_some_and(|b| b.len() > 100), "{name}");
        }
    }

    #[test]
    fn a_catalog_replaces_the_games_but_keeps_what_is_installed_and_what_could_not_be_had() {
        let mut r = GameRegistry::new();
        assert!(r.mark_installed("kcd2", Some("D:\\Games\\KCD2")));
        let list = r#"{ "schema": 1, "games": [ { "id": "kcd2", "repo": "Cairn-client/mod-kcd2" }, { "id": "noita", "repo": "Cairn-client/mod-noita" }, { "id": "ghost", "repo": "Cairn-client/mod-ghost" } ] }"#;
        let noita = r##"{ "schema": 1, "id": "noita", "name": "Noita", "developer": "Nolla Games", "accent": "#E8A35A", "ownSession": true, "images": { "logo": "noita-logo.png" } }"##;
        let kcd2 = r#"{ "schema": 1, "id": "kcd2", "name": "KCD2 from the catalog" }"#;
        r.apply(&snapshot(list, &[("noita", noita), ("kcd2", kcd2)]));
        assert_eq!(
            r.all().iter().map(|p| p.display_name.as_str()).collect::<Vec<_>>(),
            vec!["KCD2 from the catalog", "Noita"]
        );
        let k = r.get("kcd2").unwrap();
        assert!(k.is_installed && k.install_path == "D:\\Games\\KCD2"); // the install state survives a new catalog
        let n = r.get("noita").unwrap();
        assert!(n.own_session && n.logo_image == "https://x.test/noita-logo.png" && n.background_image.is_empty());
        assert!(r.get("ghost").is_none()); // nothing known about it: not invented
        assert_eq!(r.catalog().games.len(), 3);
        // the next catalog could not read noita: it stays as it was
        r.apply(&snapshot(list, &[("kcd2", kcd2)]));
        assert!(r.get("noita").is_some());
        assert!(r.get("missing").is_none() && !r.mark_installed("missing", None));
    }
}
