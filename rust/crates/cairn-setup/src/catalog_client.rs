// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Reads the catalog from the network and keeps the last good copy on disk (`%LOCALAPPDATA%\Cairn\cache\catalog`). `games.json` comes
//! from the games repository; each game's `game.json` from the latest release of its own repository, or, when it has none, from
//! `games/<id>/game.json` in the games repository. Offline, the copy on disk is used; with no copy there is no catalog.

use crate::catalog::{GameEntry, GameList, GameManifest, GAMES_FILE, GAME_FILE};
use crate::catalog_limits as lim;
use crate::http::Http;
use crate::packs::plan;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub const DEFAULT_GAMES_BASE: &str = "https://raw.githubusercontent.com/Cairn-client/games/main/";
/// How many games are asked for at once.
const PARALLEL: usize = 6;

/// A game with its manifest and the folder (a URL ending in `/`) its pictures are looked up in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogGame {
    pub entry: GameEntry,
    pub manifest: GameManifest,
    pub base_url: String,
}

impl CatalogGame {
    /// The address of a picture named in the manifest: beside the file that names it.
    pub fn picture(&self, name: &str) -> String {
        if name.is_empty() {
            String::new()
        } else {
            format!("{}{name}", self.base_url)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    pub list: GameList,
    pub games: Vec<CatalogGame>,
    /// Some of it is the last copy on disk: the network did not give the whole answer.
    pub from_cache: bool,
}

pub struct CatalogClient {
    cache_dir: PathBuf,
    games_base: String,
    use_repo_releases: bool,
    follows_beta: Box<dyn Fn(&str) -> bool + Send + Sync>,
}

impl CatalogClient {
    /// `games_base` (default the games repository; `CAIRN_GAMES_URL` points it at a local folder served over http, for working on the
    /// catalog); the games' own releases are used only with the default base unless `use_repo_releases` says otherwise.
    pub fn new(cache_dir: impl Into<PathBuf>, games_base: Option<&str>, use_repo_releases: Option<bool>) -> Self {
        let b = games_base.filter(|b| !b.trim().is_empty()).unwrap_or(DEFAULT_GAMES_BASE);
        let games_base = if b.ends_with('/') { b.to_string() } else { format!("{b}/") };
        let use_repo_releases = use_repo_releases.unwrap_or(games_base == DEFAULT_GAMES_BASE);
        Self {
            cache_dir: cache_dir.into(),
            games_base,
            use_repo_releases,
            follows_beta: Box::new(|_| false),
        }
    }

    /// Whether the player follows the beta channel of a game (its manifest then comes from its newest release, pre-releases included).
    pub fn with_follows_beta(mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        self.follows_beta = Box::new(f);
        self
    }

    pub fn default_cache_dir() -> PathBuf {
        crate::packs::PackStore::default_root()
            .parent()
            .map(|p| p.join("cache").join("catalog"))
            .unwrap_or_else(|| PathBuf::from("catalog-cache"))
    }

    pub fn load(&self, http: &dyn Http) -> Option<CatalogSnapshot> {
        let mut from_cache = false;
        let text = self.get(http, &format!("{}{GAMES_FILE}", self.games_base));
        let mut list = text.as_deref().and_then(GameList::parse);
        if let (Some(_), Some(t)) = (&list, &text) {
            self.save(GAMES_FILE, t);
        } else {
            from_cache = true;
            list = GameList::parse(&self.read_cache(GAMES_FILE)?);
            list.as_ref()?;
        }
        let list = list?;

        let found: Vec<Mutex<Option<CatalogGame>>> = list.games.iter().map(|_| Mutex::new(None)).collect();
        let next = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..PARALLEL.min(list.games.len().max(1)) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(entry) = list.games.get(i) else { break };
                    *found[i].lock().unwrap() = self.load_game(http, entry);
                });
            }
        });
        let games: Vec<CatalogGame> = found.into_iter().filter_map(|m| m.into_inner().unwrap()).collect();
        let partial = games.len() < list.games.len();
        Some(CatalogSnapshot {
            list,
            games,
            from_cache: from_cache || partial,
        })
    }

    fn load_game(&self, http: &dyn Http, entry: &GameEntry) -> Option<CatalogGame> {
        let mut bases: Vec<String> = Vec::new();
        if self.use_repo_releases && !entry.repo.is_empty() {
            if (self.follows_beta)(&entry.id) {
                if let Some(newest) = plan::newest_tag(http, &entry.repo) {
                    bases.push(format!("https://github.com/{}/releases/download/{newest}/", entry.repo));
                }
            }
            bases.push(format!("https://github.com/{}/releases/latest/download/", entry.repo));
        }
        bases.push(format!("{}games/{}/", self.games_base, entry.id));

        for b in bases {
            let Some(text) = self.get(http, &format!("{b}{GAME_FILE}")) else {
                continue;
            };
            let Some(m) = GameManifest::parse(&text).filter(|m| m.id.eq_ignore_ascii_case(&entry.id)) else {
                continue;
            };
            self.save(&format!("game-{}.json", entry.id), &text);
            self.save(&format!("game-{}.base", entry.id), &b);
            return Some(CatalogGame {
                entry: entry.clone(),
                manifest: m,
                base_url: b,
            });
        }
        // not reachable: the last copy, with the folder it came from
        let cached = self.read_cache(&format!("game-{}.json", entry.id))?;
        let base_of = self.read_cache(&format!("game-{}.base", entry.id)).filter(|b| !b.is_empty())?;
        let cm = GameManifest::parse(&cached).filter(|m| m.id.eq_ignore_ascii_case(&entry.id))?;
        Some(CatalogGame {
            entry: entry.clone(),
            manifest: cm,
            base_url: base_of,
        })
    }

    /// A catalog file as text; `None` when it is missing, not a success, or bigger than a catalog file may be.
    fn get(&self, http: &dyn Http, url: &str) -> Option<String> {
        let resp = http.get(url).ok()?;
        if !resp.is_success() || resp.content_length.is_some_and(|n| n > lim::MAX_FILE as u64) {
            return None;
        }
        let text = resp.text(lim::MAX_FILE as u64 + 1).ok()?;
        (text.len() <= lim::MAX_FILE).then_some(text)
    }

    fn save(&self, name: &str, text: &str) {
        // no cache is a lesser launcher, not a broken one
        if fs::create_dir_all(&self.cache_dir).is_err() {
            return;
        }
        let path = self.cache_dir.join(name);
        let tmp = self.cache_dir.join(format!("{name}.tmp"));
        if fs::write(&tmp, text).is_ok() && fs::rename(&tmp, &path).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }

    fn read_cache(&self, name: &str) -> Option<String> {
        fs::read(self.cache_dir.join(name))
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::packs::store::tests::temp;

    const GAMES: &str = r#"{ "schema": 1, "games": [ { "id": "a", "repo": "Cairn-client/mod-a" }, { "id": "b" } ] }"#;
    fn game(id: &str, name: &str) -> String {
        format!(r#"{{ "schema": 1, "id": "{id}", "name": "{name}", "images": {{ "logo": "{id}.png" }} }}"#)
    }
    fn names(s: &CatalogSnapshot) -> Vec<&str> {
        s.games.iter().map(|g| g.manifest.name.as_str()).collect()
    }
    const A_RELEASE: &str = "https://github.com/Cairn-client/mod-a/releases/latest/download/";

    fn online() -> FakeHttp {
        let f = FakeHttp::default();
        f.put(&format!("{DEFAULT_GAMES_BASE}games.json"), 200, GAMES.as_bytes());
        f.put(&format!("{A_RELEASE}game.json"), 200, game("a", "Game A").as_bytes());
        f.put(
            &format!("{DEFAULT_GAMES_BASE}games/b/game.json"),
            200,
            game("b", "Game B").as_bytes(),
        );
        f
    }

    #[test]
    fn a_game_with_a_repository_reads_its_game_json_from_its_latest_release() {
        let cache = temp("cc1");
        let snap = CatalogClient::new(&cache, None, None).load(&online()).unwrap();
        assert!(!snap.from_cache);
        assert_eq!(names(&snap), vec!["Game A", "Game B"]);
        // its pictures are looked up beside the file that names them
        assert_eq!(snap.games[0].picture("a.png"), format!("{A_RELEASE}a.png"));
        assert_eq!(snap.games[1].picture("b.png"), format!("{DEFAULT_GAMES_BASE}games/b/b.png"));
        assert_eq!(snap.games[1].picture(""), "");
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_game_whose_repository_has_no_game_json_yet_falls_back_to_the_games_repository() {
        let cache = temp("cc2");
        let f = FakeHttp::default();
        f.put(&format!("{DEFAULT_GAMES_BASE}games.json"), 200, GAMES.as_bytes());
        f.put(
            &format!("{DEFAULT_GAMES_BASE}games/a/game.json"),
            200,
            game("a", "A from the list repo").as_bytes(),
        );
        f.put(&format!("{DEFAULT_GAMES_BASE}games/b/game.json"), 200, game("b", "B").as_bytes());
        let snap = CatalogClient::new(&cache, None, None).load(&f).unwrap();
        assert_eq!(snap.games[0].manifest.name, "A from the list repo");
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_game_json_that_is_another_game_or_bad_is_not_used() {
        let cache = temp("cc3");
        let f = FakeHttp::default();
        f.put(&format!("{DEFAULT_GAMES_BASE}games.json"), 200, GAMES.as_bytes());
        f.put(&format!("{A_RELEASE}game.json"), 200, game("zzz", "Impostor").as_bytes());
        f.put(&format!("{DEFAULT_GAMES_BASE}games/a/game.json"), 200, b"{ nope");
        f.put(&format!("{DEFAULT_GAMES_BASE}games/b/game.json"), 200, game("b", "B").as_bytes());
        let snap = CatalogClient::new(&cache, None, None).load(&f).unwrap();
        assert_eq!(names(&snap), vec!["B"]);
        assert!(snap.from_cache); // one game could not be had: what is shown is not the whole answer
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn offline_the_last_copy_on_disk_is_used() {
        let cache = temp("cc4");
        assert!(CatalogClient::new(&cache, None, None).load(&online()).is_some());
        let snap = CatalogClient::new(&cache, None, None).load(&FakeHttp::default()).unwrap();
        assert!(snap.from_cache);
        assert_eq!(names(&snap), vec!["Game A", "Game B"]);
        assert_eq!(snap.games[0].picture("a.png"), format!("{A_RELEASE}a.png"));
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn with_neither_network_nor_copy_there_is_no_catalog() {
        let cache = temp("cc5");
        assert!(CatalogClient::new(&cache, None, None).load(&FakeHttp::default()).is_none());
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_broken_list_on_the_network_does_not_replace_the_copy_on_disk() {
        let cache = temp("cc6");
        assert!(CatalogClient::new(&cache, None, None).load(&online()).is_some());
        let broken = FakeHttp::default();
        broken.put(&format!("{DEFAULT_GAMES_BASE}games.json"), 200, b"{ broken");
        let snap = CatalogClient::new(&cache, None, None).load(&broken).unwrap();
        assert!(snap.from_cache);
        assert_eq!(snap.list.games.len(), 2);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_local_copy_of_the_catalog_serves_every_game_itself() {
        let cache = temp("cc7");
        let f = FakeHttp::default();
        f.put("http://localhost:9/games.json", 200, GAMES.as_bytes());
        f.put("http://localhost:9/games/a/game.json", 200, game("a", "A").as_bytes());
        f.put("http://localhost:9/games/b/game.json", 200, game("b", "B").as_bytes());
        let snap = CatalogClient::new(&cache, Some("http://localhost:9"), None).load(&f).unwrap();
        assert_eq!(snap.games.len(), 2);
        assert!(f.asked.lock().unwrap().iter().all(|u| !u.starts_with("https://github.com")));
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_game_on_the_beta_channel_reads_its_game_json_from_its_newest_release() {
        let cache = temp("cc8");
        let f = online();
        f.put(
            "https://api.github.com/repos/Cairn-client/mod-a/releases?per_page=10",
            200,
            br#"[ {"tag_name":"0.2.0"} ]"#,
        );
        f.put(
            "https://github.com/Cairn-client/mod-a/releases/download/0.2.0/game.json",
            200,
            game("a", "Beta A").as_bytes(),
        );
        let snap = CatalogClient::new(&cache, None, None)
            .with_follows_beta(|g| g == "a")
            .load(&f)
            .unwrap();
        assert_eq!(names(&snap), vec!["Beta A", "Game B"]);
        assert!(snap.games[0].base_url.contains("/download/0.2.0/"));
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_file_bigger_than_a_catalog_file_may_be_is_not_read() {
        let cache = temp("cc9");
        let f = FakeHttp::default();
        f.put(
            &format!("{DEFAULT_GAMES_BASE}games.json"),
            200,
            format!("{GAMES}{}", " ".repeat(lim::MAX_FILE)).as_bytes(),
        );
        assert!(CatalogClient::new(&cache, None, None).load(&f).is_none());
        let _ = fs::remove_dir_all(&cache);
    }
}
