// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The game catalog: which games the launcher knows, and everything about each one, read from the network (and the last copy of it on
//! disk) instead of being built into the launcher. Two kinds of file:
//!
//! * `games.json` -- one, in the `Cairn-client/games` repository: the list of games (each with the repository of its mod), the repositories of
//!   the packs that are not a game's mod, the shared requirement definitions, the blocked mod versions and the notices.
//! * `game.json` -- one per game, published with the game's own repository (a release asset) or, for a game without a repository yet, in the
//!   games repository (`games/<id>/game.json`): the name, the Steam ids, the artwork, what it requires, what its mod does.
//!
//! Both are data only. Everything in them is checked here: ids, names and paths are plain, text is short, and a picture is a file name that is
//! looked up beside the file that names it, never a URL of its own.

use crate::catalog_limits as lim;
use crate::game_options::{self, GameOption};
use crate::game_setup::{self, SetupRule};
use crate::time_util;
use crate::version::{launcher_core, Version};
use serde_json::Value;
use std::collections::HashMap;

pub const GAMES_FILE: &str = "games.json";
pub const GAME_FILE: &str = "game.json";
pub const CURRENT_SCHEMA: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameEntry {
    pub id: String,
    /// The game's mod repository (`owner/name`); empty when it has none yet.
    pub repo: String,
    /// The game's executable is not found by looking in its folder: the launcher keeps its path as a setting.
    pub exe_setting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSource {
    pub id: String,
    pub repo: String,
}

pub const STEAM_APP: &str = "steam-app";

/// Something a game needs, defined once and named by every game that needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub app_id: u32,
    pub has_store_page: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedPack {
    pub pack: String,
    pub version: String,
    pub max: String,
    pub reason: String,
}

impl BlockedPack {
    /// A version listed exactly, or every version up to `max`.
    pub fn matches(&self, pack: &str, version: &str) -> bool {
        if !self.pack.eq_ignore_ascii_case(pack) {
            return false;
        }
        let Some(v) = Version::parse(version) else { return false };
        if !self.version.is_empty() {
            return Version::parse(&self.version) == Some(v);
        }
        !self.max.is_empty() && Version::parse(&self.max).is_some_and(|max| v <= max)
    }
}

pub const KNOWN_ISSUE: &str = "known-issue";
pub const NEWS: &str = "news";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: String,
    pub title: String,
    pub text: String,
    pub kind: String,
    pub games: Vec<String>,
    pub from_launcher: String,
    pub until_launcher: String,
    pub until: String,
}

impl Notice {
    /// Whether to show it now: not past its end date, for this launcher version, and for a game the player has (`has_game`, when given).
    pub fn is_current(&self, launcher_version: Option<&str>, utc_now: i64, has_game: Option<&dyn Fn(&str) -> bool>) -> bool {
        if !self.until.is_empty() && time_util::parse_utc(&self.until).is_some_and(|end| utc_now > end) {
            return false;
        }
        let have = Version::parse(&launcher_core(launcher_version));
        if let (Some(from), Some(have)) = (Version::parse(&self.from_launcher).filter(|_| !self.from_launcher.is_empty()), have) {
            if have < from {
                return false;
            }
        }
        if let (Some(up_to), Some(have)) = (
            Version::parse(&self.until_launcher).filter(|_| !self.until_launcher.is_empty()),
            have,
        ) {
            if have > up_to {
                return false;
            }
        }
        match has_game {
            Some(f) if !self.games.is_empty() => self.games.iter().any(|g| f(g)),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameList {
    pub games: Vec<GameEntry>,
    pub packs: Vec<PackSource>,
    pub components: HashMap<String, Component>,
    pub blocked: Vec<BlockedPack>,
    pub notices: Vec<Notice>,
}

impl GameList {
    /// The repository a pack is downloaded from: a pack of its own, or a game's mod repository.
    pub fn repo_of_pack(&self, id: &str) -> Option<&str> {
        self.packs
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(id))
            .map(|p| p.repo.as_str())
            .or_else(|| {
                self.games
                    .iter()
                    .find(|g| g.id.eq_ignore_ascii_case(id) && !g.repo.is_empty())
                    .map(|g| g.repo.as_str())
            })
    }

    pub fn block_of(&self, pack: &str, version: &str) -> Option<&BlockedPack> {
        self.blocked.iter().find(|b| b.matches(pack, version))
    }

    pub fn component(&self, id: &str) -> Option<&Component> {
        self.components.get(&id.to_ascii_lowercase())
    }

    /// `None` when the file is too big, of another schema or malformed in any way.
    pub fn parse(json: &str) -> Option<Self> {
        if json.len() > lim::MAX_FILE {
            return None;
        }
        let r: Value = serde_json::from_str(json).ok()?;
        if r.get("schema")?.as_i64()? != CURRENT_SCHEMA {
            return None;
        }
        let mut games: Vec<GameEntry> = Vec::new();
        for g in lim::items(&r, "games") {
            let (id, repo) = (lim::str_of(g, "id"), lim::str_of(g, "repo"));
            if !lim::is_id(&id) || (!repo.is_empty() && !lim::is_repo(&repo)) || games.iter().any(|x| x.id == id) {
                return None;
            }
            games.push(GameEntry {
                id,
                repo,
                exe_setting: lim::flag(g, "exeSetting"),
            });
        }
        let mut packs = Vec::new();
        for p in lim::items(&r, "packs") {
            let (id, repo) = (lim::str_of(p, "id"), lim::str_of(p, "repo"));
            if !lim::is_id(&id) || !lim::is_repo(&repo) {
                return None;
            }
            packs.push(PackSource { id, repo });
        }
        let mut components = HashMap::new();
        if let Some(cs) = r.get("components").and_then(Value::as_object) {
            for (name, c) in cs {
                let comp = parse_component(name, c)?;
                components.insert(name.to_ascii_lowercase(), comp);
            }
        }
        let mut blocked = Vec::new();
        for b in lim::items(&r, "blocked") {
            let bp = BlockedPack {
                pack: lim::str_of(b, "pack"),
                version: lim::str_of(b, "version"),
                max: lim::str_of(b, "max"),
                reason: lim::text(&lim::str_of(b, "reason"), 300),
            };
            // exactly one of version and max
            if !lim::is_id(&bp.pack)
                || bp.version.is_empty() == bp.max.is_empty()
                || (!bp.version.is_empty() && !lim::is_version(&bp.version))
                || (!bp.max.is_empty() && !lim::is_version(&bp.max))
            {
                return None;
            }
            blocked.push(bp);
        }
        let mut notices = Vec::new();
        for n in lim::items(&r, "notices") {
            let mut for_games = Vec::new();
            if let Some(arr) = n.get("games").and_then(Value::as_array) {
                for e in arr {
                    let g = e.as_str().filter(|g| lim::is_id(g))?;
                    for_games.push(g.to_string());
                }
            }
            let kind = if lim::str_of(n, "kind").is_empty() {
                NEWS.to_string()
            } else {
                lim::str_of(n, "kind")
            };
            let no = Notice {
                id: lim::str_of(n, "id"),
                title: lim::text(&lim::str_of(n, "title"), 120),
                text: lim::text(&lim::str_of(n, "text"), 600),
                kind,
                games: for_games,
                from_launcher: lim::str_of(n, "fromLauncher"),
                until_launcher: lim::str_of(n, "untilLauncher"),
                until: lim::str_of(n, "until"),
            };
            if !lim::is_id(&no.id)
                || no.title.is_empty()
                || !matches!(no.kind.as_str(), KNOWN_ISSUE | NEWS)
                || (!no.from_launcher.is_empty() && !lim::is_version(&no.from_launcher))
                || (!no.until_launcher.is_empty() && !lim::is_version(&no.until_launcher))
            {
                return None;
            }
            notices.push(no);
        }
        Some(Self {
            games,
            packs,
            components,
            blocked,
            notices,
        })
    }
}

fn parse_component(id: &str, e: &Value) -> Option<Component> {
    if !lim::is_id(id) || !e.is_object() {
        return None;
    }
    let (kind, name) = (lim::str_of(e, "kind"), lim::text(&lim::str_of(e, "name"), 80));
    if name.is_empty() || !lim::is_id(&kind) {
        return None;
    }
    let mut app_id = 0;
    if kind == STEAM_APP {
        app_id = lim::u32_of(e, "appId").filter(|a| *a != 0)?;
    }
    let has_store_page = e.get("storePage") != Some(&Value::Bool(false));
    Some(Component {
        id: id.to_string(),
        kind,
        name,
        app_id,
        has_store_page,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureState {
    Ready,
    InTest,
    Planned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureLine {
    pub name: String,
    pub detail: String,
    pub state: FeatureState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureGroup {
    pub title: String,
    pub items: Vec<FeatureLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameImages {
    pub background: String,
    pub logo: String,
    pub hero: String,
    pub poster: String,
    pub screenshots: Vec<String>,
}

/// One game's `game.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameManifest {
    pub id: String,
    pub name: String,
    pub developer: String,
    pub franchise: String,
    pub supported: bool,
    pub steam_app_id: u32,
    pub store_app_id: u32,
    pub exe: String,
    pub description: String,
    pub accent: String,
    pub images: GameImages,
    pub requires: Vec<String>,
    pub inline_requires: Vec<Component>,
    pub features: Vec<FeatureGroup>,
    pub content: Vec<String>,
    pub host_steps: Vec<String>,
    pub join_steps: Vec<String>,
    /// The mod runs the session itself: the page shows Play instead of Host and Join.
    pub own_session: bool,
    pub setup: Vec<SetupRule>,
    pub options: Vec<GameOption>,
}

impl GameManifest {
    /// `None` when the file is too big, of another schema or malformed in any way.
    pub fn parse(json: &str) -> Option<Self> {
        if json.len() > lim::MAX_FILE {
            return None;
        }
        let r: Value = serde_json::from_str(json).ok()?;
        if r.get("schema")?.as_i64()? != CURRENT_SCHEMA {
            return None;
        }
        let id = lim::str_of(&r, "id");
        let name = lim::text(&lim::str_of(&r, "name"), 80);
        if !lim::is_id(&id) || name.is_empty() {
            return None;
        }
        let accent = lim::str_of(&r, "accent");
        if !accent.is_empty() && !(accent.len() == 7 && accent.starts_with('#') && accent[1..].bytes().all(|b| b.is_ascii_hexdigit())) {
            return None;
        }
        let exe = lim::str_of(&r, "exe");
        if !exe.is_empty()
            && !(exe.len() <= 80
                && exe
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b' '))
                && !exe.contains(".."))
        {
            return None;
        }

        let mut images = GameImages::default();
        if let Some(im) = r.get("images").filter(|i| i.is_object()) {
            let mut shots = Vec::new();
            for s in lim::items(im, "screenshots") {
                let f = s.as_str().unwrap_or("");
                if !lim::is_image_path(f) {
                    return None;
                }
                shots.push(f.to_string());
            }
            if shots.len() > 8 {
                return None;
            }
            images = GameImages {
                background: lim::str_of(im, "background"),
                logo: lim::str_of(im, "logo"),
                hero: lim::str_of(im, "hero"),
                poster: lim::str_of(im, "poster"),
                screenshots: shots,
            };
            for f in [&images.background, &images.logo, &images.hero, &images.poster] {
                if !f.is_empty() && !lim::is_image_path(f) {
                    return None;
                }
            }
        }

        let mut requires = Vec::new();
        let mut inline = Vec::new();
        for q in lim::items(&r, "requires") {
            match q {
                Value::String(cid) => {
                    if !lim::is_id(cid) {
                        return None;
                    }
                    requires.push(cid.clone());
                }
                Value::Object(_) => inline.push(parse_component(&lim::str_of(q, "id"), q)?),
                _ => return None,
            }
        }
        if requires.len() + inline.len() > 12 {
            return None;
        }

        let mut groups = Vec::new();
        for g in lim::items(&r, "features") {
            let title = lim::text(&lim::str_of(g, "title"), 40);
            let mut lines = Vec::new();
            for f in lim::items(g, "items") {
                let state = match lim::str_of(f, "status").as_str() {
                    "" | "ready" => FeatureState::Ready,
                    "inTest" => FeatureState::InTest,
                    "planned" => FeatureState::Planned,
                    _ => return None,
                };
                let fname = lim::text(&lim::str_of(f, "name"), 80);
                if fname.is_empty() {
                    return None;
                }
                lines.push(FeatureLine {
                    name: fname,
                    detail: lim::text(&lim::str_of(f, "detail"), 400),
                    state,
                });
            }
            if title.is_empty() || lines.len() > 30 {
                return None;
            }
            groups.push(FeatureGroup { title, items: lines });
        }
        if groups.len() > 12 {
            return None;
        }

        let mut content = Vec::new();
        for c in lim::items(&r, "content") {
            let cid = c.as_str().unwrap_or("");
            if !lim::is_id(cid) {
                return None;
            }
            content.push(cid.to_string());
        }

        let steps = |n: &str| -> Option<Vec<String>> {
            let mut list = Vec::new();
            for s in lim::items(&r, n) {
                let line = lim::text(s.as_str()?, 200);
                if !line.is_empty() {
                    list.push(line);
                }
            }
            (list.len() <= 8).then_some(list)
        };
        let (host_steps, join_steps) = (steps("hostSteps")?, steps("joinSteps")?);
        let options = game_options::parse(&r)?;
        let setup = game_setup::parse(&r)?;

        Some(Self {
            id,
            name,
            developer: lim::text(&lim::str_of(&r, "developer"), 80),
            franchise: lim::text(&lim::str_of(&r, "franchise"), 80),
            supported: r.get("supported") != Some(&Value::Bool(false)),
            steam_app_id: lim::u32_of(&r, "steamAppId").unwrap_or(0),
            store_app_id: lim::u32_of(&r, "storeAppId").unwrap_or(0),
            exe,
            description: lim::text(&lim::str_of(&r, "description"), 400),
            accent,
            images,
            requires,
            inline_requires: inline,
            features: groups,
            content,
            host_steps,
            join_steps,
            own_session: lim::flag(&r, "ownSession"),
            setup,
            options,
        })
    }

    /// The components the game needs: the shared ones by name (a name no definition has is left out, not invented), then its own inline ones.
    pub fn resolve_requirements(&self, list: &GameList) -> Vec<Component> {
        let mut all: Vec<Component> = self.requires.iter().filter_map(|id| list.component(id).cloned()).collect();
        all.extend(self.inline_requires.iter().cloned());
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_LIST: &str = r#"
        { "schema": 1,
          "games": [ { "id": "kcd2", "repo": "Cairn-client/mod-kcd2" }, { "id": "cp77" } ],
          "packs": [ { "id": "core", "repo": "Cairn-client/core" } ],
          "components": {
            "kcd2-game": { "kind": "steam-app", "name": "Kingdom Come: Deliverance II", "appId": 1771300 },
            "kcd2-tools": { "kind": "steam-app", "name": "Modding Tools", "appId": 2429020, "storePage": false } },
          "blocked": [ { "pack": "kcd2", "version": "0.1.0", "reason": "Saves are lost." }, { "pack": "kcd2", "max": "0.0.5", "reason": "Too old." } ],
          "notices": [ { "id": "n1", "title": "Sleep", "text": "Sleep needs both players.", "kind": "known-issue", "games": ["kcd2"] },
                       { "id": "n2", "title": "News", "text": "Hello", "until": "2026-01-01" } ] }"#;

    #[test]
    fn a_good_list_is_read() {
        let l = GameList::parse(GOOD_LIST).unwrap();
        assert_eq!(l.games.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), vec!["kcd2", "cp77"]);
        assert_eq!(l.games[0].repo, "Cairn-client/mod-kcd2");
        assert_eq!(l.games[1].repo, "");
        assert_eq!(l.repo_of_pack("core"), Some("Cairn-client/core"));
        assert_eq!(l.repo_of_pack("kcd2"), Some("Cairn-client/mod-kcd2"));
        assert_eq!(l.repo_of_pack("cp77"), None);
        assert_eq!(l.component("kcd2-tools").unwrap().app_id, 2_429_020);
        assert!(!l.component("kcd2-tools").unwrap().has_store_page);
        assert!(l.component("KCD2-GAME").unwrap().has_store_page);
    }

    #[test]
    fn a_game_can_say_its_executable_is_a_setting() {
        let l = GameList::parse(
            r#"{ "schema": 1, "games": [ { "id": "a", "repo": "o/a", "exeSetting": true }, { "id": "b", "repo": "o/b" } ] }"#,
        )
        .unwrap();
        assert!(l.games[0].exe_setting);
        assert!(!l.games[1].exe_setting);
    }

    #[test]
    fn a_bad_list_is_refused() {
        for json in [
            r#"{ "schema": 2, "games": [] }"#,
            r#"{ "games": [] }"#,
            r#"{ "schema": 1, "games": [ { "id": "../x" } ] }"#,
            r#"{ "schema": 1, "games": [ { "id": "a" }, { "id": "a" } ] }"#,
            r#"{ "schema": 1, "games": [ { "id": "a", "repo": "https://evil.test/x" } ] }"#,
            r#"{ "schema": 1, "packs": [ { "id": "core", "repo": "no-slash" } ] }"#,
            r#"{ "schema": 1, "components": { "x": { "kind": "steam-app", "name": "X" } } }"#,
            r#"{ "schema": 1, "blocked": [ { "pack": "kcd2", "reason": "neither version nor max" } ] }"#,
            r#"{ "schema": 1, "blocked": [ { "pack": "kcd2", "version": "0.1.0", "max": "0.2.0" } ] }"#,
            r#"{ "schema": 1, "notices": [ { "id": "n", "title": "" } ] }"#,
            r#"{ "schema": 1, "notices": [ { "id": "n", "title": "t", "kind": "advert" } ] }"#,
            "not json",
        ] {
            assert_eq!(GameList::parse(json), None, "{json}");
        }
    }

    #[test]
    fn a_list_that_is_too_big_is_refused() {
        let big = format!(r#"{{ "schema": 1, "notices": [] }}{}"#, " ".repeat(lim::MAX_FILE));
        assert_eq!(GameList::parse(&big), None);
    }

    #[test]
    fn blocked_versions_are_matched_by_version_or_by_range() {
        let l = GameList::parse(GOOD_LIST).unwrap();
        assert_eq!(l.block_of("kcd2", "0.1.0").unwrap().reason, "Saves are lost.");
        assert!(l.block_of("kcd2", "0.1.1").is_none());
        assert_eq!(l.block_of("kcd2", "0.0.4").unwrap().reason, "Too old.");
        assert_eq!(l.block_of("KCD2", "0.0.5").unwrap().reason, "Too old.");
        assert!(l.block_of("kcd2", "0.0.6").is_none());
        assert!(l.block_of("other", "0.1.0").is_none());
        assert!(l.block_of("kcd2", "not a version").is_none());
    }

    #[test]
    fn notices_are_current_until_their_end_and_for_the_games_the_player_has() {
        let l = GameList::parse(GOOD_LIST).unwrap();
        let now = time_util::parse_utc("2026-06-01").unwrap();
        let issue = l.notices.iter().find(|n| n.id == "n1").unwrap();
        let news = l.notices.iter().find(|n| n.id == "n2").unwrap();
        assert!(issue.is_current(Some("0.2.0"), now, Some(&|g| g == "kcd2")));
        assert!(!issue.is_current(Some("0.2.0"), now, Some(&|g| g == "cp77"))); // not for a player who does not have that game
        assert!(issue.is_current(Some("0.2.0"), now, None));
        assert!(!news.is_current(Some("0.2.0"), now, None)); // it ended on 1 January
        assert!(news.is_current(Some("0.2.0"), time_util::parse_utc("2025-12-01").unwrap(), None));
    }

    #[test]
    fn a_notice_can_be_limited_to_launcher_versions() {
        let n = Notice {
            id: "n".into(),
            title: "t".into(),
            text: "x".into(),
            kind: NEWS.into(),
            games: vec![],
            from_launcher: "0.3.0".into(),
            until_launcher: "0.4.0".into(),
            until: String::new(),
        };
        let now = time_util::now_utc();
        assert!(!n.is_current(Some("0.2.9"), now, None));
        assert!(n.is_current(Some("0.3.0"), now, None));
        assert!(n.is_current(Some("0.4.0+abc"), now, None));
        assert!(!n.is_current(Some("0.4.1"), now, None));
    }

    #[test]
    fn notice_text_has_no_markup_or_control_characters() {
        let l = GameList::parse(r#"{ "schema": 1, "notices": [ { "id": "n", "title": "<b>Hi</b>\u0007", "text": "a\nb" } ] }"#).unwrap();
        assert_eq!(l.notices[0].title, "bHi/b");
        assert_eq!(l.notices[0].text, "ab");
    }

    const GOOD_GAME: &str = r##"
        { "schema": 1, "id": "kcd2", "name": "Kingdom Come: Deliverance II", "developer": "Warhorse Studios", "franchise": "Kingdom Come",
          "steamAppId": 2429020, "storeAppId": 1771300, "exe": "KingdomCome.exe", "description": "Explore Bohemia.", "accent": "#E8C872",
          "images": { "background": "kcd2.jpg", "logo": "kcd2-logo.png", "hero": "kcd2-hero.jpg", "poster": "kcd2-poster.jpg", "screenshots": ["shots/kcd2-1.jpg", "shots/kcd2-2.jpg"] },
          "requires": [ "kcd2-game", { "id": "extra", "kind": "steam-app", "name": "Extra", "appId": 5 } ],
          "features": [ { "title": "Sessions", "items": [ { "name": "Host", "detail": "Join by code." }, { "name": "Map", "detail": "On the map.", "status": "inTest" }, { "name": "Later", "status": "planned" } ] } ],
          "content": [ "kcd2-start-saves" ] }"##;

    #[test]
    fn a_good_game_is_read() {
        let m = GameManifest::parse(GOOD_GAME).unwrap();
        assert_eq!(m.id, "kcd2");
        assert_eq!((m.steam_app_id, m.store_app_id), (2_429_020, 1_771_300));
        assert!(m.supported);
        assert_eq!(m.accent, "#E8C872");
        assert_eq!(m.images.screenshots.len(), 2);
        let states: Vec<FeatureState> = m.features[0].items.iter().map(|x| x.state).collect();
        assert_eq!(states, vec![FeatureState::Ready, FeatureState::InTest, FeatureState::Planned]);
        assert_eq!(m.content, vec!["kcd2-start-saves"]);
    }

    #[test]
    fn a_requirement_is_a_short_name_or_an_inline_definition() {
        let list =
            GameList::parse(r#"{ "schema": 1, "components": { "kcd2-game": { "kind": "steam-app", "name": "KCD2", "appId": 1771300 } } }"#)
                .unwrap();
        let m = GameManifest::parse(GOOD_GAME).unwrap();
        assert_eq!(
            m.resolve_requirements(&list).iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["KCD2", "Extra"]
        );
        // a name no definition has is left out, not invented
        assert_eq!(m.resolve_requirements(&GameList::default()).len(), 1);
    }

    #[test]
    fn the_how_to_lines_are_read_and_bounded() {
        let m = GameManifest::parse(
            r#"{ "schema": 1, "id": "a", "name": "A", "hostSteps": ["Press Host.", "  Load a save. "], "joinSteps": ["Press Join."] }"#,
        )
        .unwrap();
        assert_eq!(m.host_steps, vec!["Press Host.", "Load a save."]);
        assert_eq!(m.join_steps, vec!["Press Join."]);
        assert!(GameManifest::parse(r#"{ "schema": 1, "id": "a", "name": "A" }"#)
            .unwrap()
            .host_steps
            .is_empty());
        assert!(GameManifest::parse(r#"{ "schema": 1, "id": "a", "name": "A", "hostSteps": [1] }"#).is_none());
        assert!(
            GameManifest::parse(r#"{ "schema": 1, "id": "a", "name": "A", "joinSteps": ["1","2","3","4","5","6","7","8","9"] }"#).is_none()
        );
    }

    #[test]
    fn a_shared_requirement_is_defined_once_for_several_games() {
        let list =
            GameList::parse(r#"{ "schema": 1, "components": { "vcredist": { "kind": "steam-app", "name": "Runtime", "appId": 7 } } }"#)
                .unwrap();
        let a = GameManifest::parse(r#"{ "schema": 1, "id": "a", "name": "A", "requires": ["vcredist"] }"#).unwrap();
        let b = GameManifest::parse(r#"{ "schema": 1, "id": "b", "name": "B", "requires": ["vcredist"] }"#).unwrap();
        assert_eq!(a.resolve_requirements(&list), b.resolve_requirements(&list));
    }

    #[test]
    fn a_bad_game_is_refused() {
        for json in [
            r#"{ "schema": 1, "id": "a", "name": "A", "images": { "logo": "https://evil.test/x.png" } }"#,
            r#"{ "schema": 1, "id": "a", "name": "A", "images": { "logo": "../x.png" } }"#,
            r#"{ "schema": 1, "id": "a", "name": "A", "images": { "logo": "/etc/x.png" } }"#,
            r#"{ "schema": 1, "id": "a", "name": "A", "images": { "logo": "x.exe" } }"#,
            r#"{ "schema": 1, "id": "a", "name": "A", "images": { "screenshots": ["a:b.jpg"] } }"#,
            r#"{ "schema": 1, "id": "a", "name": "A", "accent": "red" }"#,
            r#"{ "schema": 1, "id": "a", "name": "A", "exe": "..\\evil.exe" }"#,
            r#"{ "schema": 1, "id": "a b", "name": "A" }"#,
            r#"{ "schema": 1, "id": "a", "name": "" }"#,
            r#"{ "schema": 1, "id": "a", "name": "A", "features": [ { "title": "T", "items": [ { "name": "x", "status": "great" } ] } ] }"#,
            r#"{ "schema": 1, "id": "a", "name": "A", "requires": [ 5 ] }"#,
            r#"{ "schema": 2, "id": "a", "name": "A" }"#,
        ] {
            assert_eq!(GameManifest::parse(json), None, "{json}");
        }
    }

    #[test]
    fn a_game_can_be_listed_as_not_supported_yet() {
        assert!(
            !GameManifest::parse(r#"{ "schema": 1, "id": "a", "name": "A", "supported": false }"#)
                .unwrap()
                .supported
        );
    }

    #[test]
    fn a_game_can_say_its_mod_runs_the_session_itself() {
        let json =
            |extra: &str| format!(r##"{{ "schema": 1, "id": "noita", "name": "Noita", "exe": "noita.exe", "accent": "#E8A35A"{extra} }}"##);
        assert!(GameManifest::parse(&json(r#", "ownSession": true"#)).unwrap().own_session);
        assert!(!GameManifest::parse(&json("")).unwrap().own_session);
        assert!(!GameManifest::parse(&json(r#", "ownSession": "yes""#)).unwrap().own_session);
        // only a real true counts
    }

    #[test]
    fn the_ready_check_and_the_options_ride_in_the_game_file() {
        let m = GameManifest::parse(
            r#"{ "schema": 1, "id": "a", "name": "A", "setup": [ { "kind": "steam-running" }, { "kind": "game-file", "path": "x/y.dll", "optional": true } ],
                 "options": [ { "id": "rt", "title": "Ray tracing", "requires": "nvidia-rtx" } ] }"#,
        )
        .unwrap();
        assert_eq!((m.setup.len(), m.options.len()), (2, 1));
        assert!(GameManifest::parse(r#"{ "schema": 1, "id": "a", "name": "A", "setup": [ { "kind": "run-it" } ] }"#).is_none());
        assert!(GameManifest::parse(r#"{ "schema": 1, "id": "a", "name": "A", "options": [ { "id": "x y", "title": "t" } ] }"#).is_none());
    }

    #[test]
    fn the_real_catalog_files_of_the_games_repository_are_accepted() {
        // the shape of games/noita/game.json as published
        let json = r##"{ "schema": 1, "id": "noita", "name": "Noita", "developer": "Nolla Games", "franchise": "Noita", "steamAppId": 881100, "exe": "noita.exe",
            "accent": "#E8A35A", "images": { "background": "noita.jpg", "logo": "noita-logo.png", "screenshots": ["noita-shot-1.jpg"] }, "ownSession": true,
            "setup": [ { "kind": "steam-running" }, { "kind": "game-found" } ], "hostSteps": ["a"], "joinSteps": ["b"] }"##;
        assert!(GameManifest::parse(json).is_some());
    }
}
