// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Game mods are separate downloads: the installer carries only the launcher, and each game's mod is a pack published with the releases of the
//! game's own mod repository, updated on its own. This is what the window needs to know about them: what each repository offers, what is on
//! this PC, and the job that downloads a pack and puts it where it goes.

use crate::downloads::{DownloadItem, RunContext, Runner};
use cairn_setup::catalog::GameList;
use cairn_setup::external_install;
use cairn_setup::http::Http;
use cairn_setup::pack_setup;
use cairn_setup::packs::contract::START_SAVE_TARGET;
use cairn_setup::packs::index::{pack_url, PackInfo};
use cairn_setup::packs::installer;
use cairn_setup::packs::plan::{self, BETA, STABLE};
use cairn_setup::packs::{PackKinds, PackStore};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// What the games' repositories offer, as of the last look.
#[derive(Debug, Clone, Default)]
pub struct Offers {
    /// game id (lower case) -> the pack to offer for it (its channel's release).
    pub by_game: BTreeMap<String, PackInfo>,
    /// game id (lower case) -> "version: why" for a newest version the catalog says not to offer.
    pub blocked: BTreeMap<String, String>,
    pub checked_at: Option<SystemTime>,
    /// A look has been made (successful or not).
    pub tried: bool,
    /// At least one game has a repository and none of them could be read.
    pub offline: bool,
}

impl Offers {
    pub fn of(&self, game_id: &str) -> Option<&PackInfo> {
        self.by_game.get(&game_id.to_lowercase())
    }
}

/// A game with a mod repository, as far as asking its releases goes.
#[derive(Debug, Clone)]
pub struct RepoGame {
    pub id: String,
    pub repo: String,
    /// `stable` or `beta`.
    pub channel: String,
}

/// Asks every repository for its latest release (and, for a game on the beta channel, its newest one) and decides what to offer. A blocked
/// beta falls back to the stable release; a blocked stable release is simply not offered.
pub fn fetch_offers(http: &dyn Http, games: &[RepoGame], catalog: &GameList) -> Offers {
    let mut offers = Offers {
        tried: true,
        ..Offers::default()
    };
    let any_repo = !games.is_empty();
    for g in games {
        let stable = plan::fetch_stable(http, &g.repo).and_then(|i| mod_of(&i));
        let beta = (g.channel == BETA)
            .then(|| plan::fetch_beta(http, &g.repo).and_then(|i| mod_of(&i)))
            .flatten();
        let chosen = plan::choose(Some(&g.channel), stable.as_ref(), beta.as_ref()).cloned();
        let Some(chosen) = chosen else { continue };
        if let Some(block) = catalog.block_of(&chosen.id, &chosen.version) {
            offers
                .blocked
                .insert(g.id.to_lowercase(), format!("{}: {}", chosen.version, block.reason));
            let fallback = stable.filter(|s| beta.as_ref() == Some(&chosen) && catalog.block_of(&s.id, &s.version).is_none());
            if let Some(s) = fallback {
                offers.by_game.insert(g.id.to_lowercase(), s);
            }
            continue;
        }
        offers.by_game.insert(g.id.to_lowercase(), chosen);
    }
    if !offers.by_game.is_empty() {
        offers.checked_at = Some(SystemTime::now());
    }
    offers.offline = any_repo && offers.by_game.is_empty() && offers.blocked.is_empty();
    offers
}

fn mod_of(index: &cairn_setup::packs::PackIndex) -> Option<PackInfo> {
    index.packs.iter().find(|p| PackKinds::is_game_mod(&p.kind)).cloned()
}

/// What is needed to download and put a pack where it goes.
pub struct JobEnv {
    pub http: Arc<dyn Http>,
    pub store: PackStore,
    /// The folder a content pack for the start saves is copied to.
    pub start_save_root: PathBuf,
}

/// The queue's job for one pack: download it; then put an external mod in the game's folder, a session mod in the game through its own program,
/// or a content pack's files where the game looks.
pub fn runner(env: Arc<JobEnv>) -> Runner {
    Arc::new(move |item: &DownloadItem, ctx: &RunContext| -> (bool, String) {
        // what it depends on was queued before it: if that did not come, this cannot work
        for dep in &item.info.depends_on {
            let have = env.store.read(&dep.id).map(|p| p.version);
            let enough = have
                .as_deref()
                .and_then(cairn_setup::version::Version::parse)
                .zip(cairn_setup::version::Version::parse(&dep.min))
                .is_some_and(|(h, m)| h >= m);
            if !enough {
                return (false, format!("{} did not download", dep.id));
            }
        }
        let result = installer::download(
            &item.info,
            &item.url,
            &env.store,
            env.http.as_ref(),
            &mut |p| ctx.progress(p),
            &ctx.cancel,
        );
        if !result.ok {
            return (false, result.message);
        }
        match item.info.kind.as_str() {
            PackKinds::CONTENT => {
                if env
                    .store
                    .read_contract(&item.info.id)
                    .is_some_and(|c| c.install_to == START_SAVE_TARGET)
                {
                    let copied = plan::copy_missing(&env.store.files_dir(&item.info.id).join("mod"), &env.start_save_root);
                    return (true, format!("{} file(s) put where the game looks", copied));
                }
                (true, result.message)
            }
            PackKinds::EXTERNAL => {
                ctx.installing();
                match &item.place_in {
                    Some(folder) => {
                        let r = external_install::place(folder, &env.store, &item.info.id, None);
                        (
                            r.ok,
                            if r.ok {
                                result.message
                            } else {
                                format!("could not install the mod: {}", r.message)
                            },
                        )
                    }
                    None => (true, result.message),
                }
            }
            _ => {
                // a mod with a program of its own puts itself in the game
                if let Some(program) = env
                    .store
                    .read_contract(&item.info.id)
                    .filter(|c| c.setup)
                    .and_then(|_| external_install::options_program_path(&env.store, &item.info.id))
                {
                    ctx.installing();
                    let r = pack_setup::do_action(&program, "place-mod", None, &mut |_| {}, &mut |_| {}, None, &ctx.cancel);
                    return (
                        r.ok,
                        if r.ok {
                            result.message
                        } else {
                            format!("could not install the mod: {}", r.message)
                        },
                    );
                }
                (true, result.message)
            }
        }
    })
}

/// A pack's download address: from the repository the games catalog names for it.
pub fn url_for(catalog: &GameList, pack: &PackInfo) -> Option<String> {
    pack_url(catalog.repo_of_pack(&pack.id)?, pack)
}

/// The channel the player follows for a game, as the catalog client wants it.
pub fn channel_name(beta: bool) -> &'static str {
    if beta {
        BETA
    } else {
        STABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_setup::http::fake::FakeHttp;

    fn index_json(id: &str, version: &str) -> String {
        format!(
            r#"{{ "schema": 2, "id": "{id}", "game": "{id}", "version": "{version}", "file": "{id}.zip", "sha256": "{}", "size": 5, "kind": "external" }}"#,
            "a".repeat(64)
        )
    }
    fn game(id: &str, channel: &str) -> RepoGame {
        RepoGame {
            id: id.into(),
            repo: format!("o/mod-{id}"),
            channel: channel.into(),
        }
    }
    fn serve_stable(h: &FakeHttp, id: &str, version: &str) {
        h.put(
            &format!("https://github.com/o/mod-{id}/releases/latest/download/pack-index.json"),
            200,
            index_json(id, version).as_bytes(),
        );
    }

    #[test]
    fn each_repository_offers_its_latest_release_and_an_unreachable_one_is_nothing() {
        let h = FakeHttp::default();
        serve_stable(&h, "noita", "1.2.0");
        let o = fetch_offers(&h, &[game("noita", "stable"), game("ghost", "stable")], &GameList::default());
        assert_eq!(o.of("NOITA").unwrap().version, "1.2.0");
        assert!(o.of("ghost").is_none() && o.tried && o.checked_at.is_some() && !o.offline);
        let none = fetch_offers(&FakeHttp::default(), &[game("noita", "stable")], &GameList::default());
        assert!(none.offline && none.of("noita").is_none() && none.checked_at.is_none());
        assert!(!fetch_offers(&FakeHttp::default(), &[], &GameList::default()).offline);
        // no repository, nothing to reach
    }

    #[test]
    fn the_beta_channel_offers_the_newer_release_and_stable_only_the_stable_one() {
        let h = FakeHttp::default();
        serve_stable(&h, "noita", "1.0.0");
        h.put(
            "https://api.github.com/repos/o/mod-noita/releases?per_page=10",
            200,
            br#"[ {"tag_name":"1.1.0"} ]"#,
        );
        h.put(
            "https://github.com/o/mod-noita/releases/download/1.1.0/pack-index.json",
            200,
            index_json("noita", "1.1.0").as_bytes(),
        );
        assert_eq!(
            fetch_offers(&h, &[game("noita", "beta")], &GameList::default())
                .of("noita")
                .unwrap()
                .version,
            "1.1.0"
        );
        assert_eq!(
            fetch_offers(&h, &[game("noita", "stable")], &GameList::default())
                .of("noita")
                .unwrap()
                .version,
            "1.0.0"
        );
    }

    #[test]
    fn a_blocked_release_is_not_offered_and_a_blocked_beta_falls_back_to_the_stable_one() {
        let catalog =
            GameList::parse(r#"{ "schema": 1, "blocked": [ { "pack": "noita", "version": "1.1.0", "reason": "Saves are lost." } ] }"#)
                .unwrap();
        let h = FakeHttp::default();
        serve_stable(&h, "noita", "1.0.0");
        h.put(
            "https://api.github.com/repos/o/mod-noita/releases?per_page=10",
            200,
            br#"[ {"tag_name":"1.1.0"} ]"#,
        );
        h.put(
            "https://github.com/o/mod-noita/releases/download/1.1.0/pack-index.json",
            200,
            index_json("noita", "1.1.0").as_bytes(),
        );
        let o = fetch_offers(&h, &[game("noita", "beta")], &catalog);
        assert_eq!(o.of("noita").unwrap().version, "1.0.0");
        assert_eq!(o.blocked["noita"], "1.1.0: Saves are lost.");
        let h2 = FakeHttp::default();
        serve_stable(&h2, "noita", "1.1.0");
        let o2 = fetch_offers(&h2, &[game("noita", "stable")], &catalog);
        assert!(o2.of("noita").is_none() && o2.blocked.contains_key("noita") && !o2.offline);
    }

    #[test]
    fn a_pack_is_downloaded_from_the_repository_the_catalog_names() {
        let catalog = GameList::parse(
            r#"{ "schema": 1, "games": [ { "id": "noita", "repo": "o/mod-noita" } ], "packs": [ { "id": "core", "repo": "o/core" } ] }"#,
        )
        .unwrap();
        let pack = |id: &str| PackInfo {
            id: id.into(),
            game: id.into(),
            version: "1.0.0".into(),
            file: format!("{id}.zip"),
            sha256: "a".repeat(64),
            size: 5,
            launcher_min: String::new(),
            upstream: String::new(),
            depends_on: vec![],
            kind: "mod".into(),
        };
        assert_eq!(
            url_for(&catalog, &pack("noita")).unwrap(),
            "https://github.com/o/mod-noita/releases/download/1.0.0/noita.zip"
        );
        assert_eq!(
            url_for(&catalog, &pack("core")).unwrap(),
            "https://github.com/o/core/releases/download/1.0.0/core.zip"
        );
        assert!(url_for(&catalog, &pack("other")).is_none());
        assert_eq!((channel_name(true), channel_name(false)), ("beta", "stable"));
    }
}
