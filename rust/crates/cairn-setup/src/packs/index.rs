// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! A mod repository's catalog of its latest pack (`pack-index.json`, attached to every release), and where its files are downloaded from.

use super::contract::{parse_dependencies, PackDependency, PackKinds};
use super::store::InstalledPack;
use crate::catalog_limits as lim;
use crate::version::{launcher_core, Version};
use serde_json::Value;

pub const INDEX_FILE_NAME: &str = "pack-index.json";
pub const CURRENT_SCHEMA: i64 = 2;

/// One pack as its repository's catalog lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInfo {
    pub id: String,
    pub game: String,
    pub version: String,
    pub file: String,
    pub sha256: String,
    pub size: i64,
    pub launcher_min: String,
    pub upstream: String,
    pub depends_on: Vec<PackDependency>,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackIndex {
    pub schema: i64,
    pub packs: Vec<PackInfo>,
}

impl PackIndex {
    /// The game's mod (not a content pack) for that game.
    pub fn for_game(&self, game_id: &str) -> Option<&PackInfo> {
        self.packs
            .iter()
            .find(|p| PackKinds::is_game_mod(&p.kind) && p.game.eq_ignore_ascii_case(game_id))
    }

    /// `None` for anything that is not a catalog this launcher understands. Ids, versions and file names come from the network: only plain
    /// names are accepted, so nothing can point outside the pack store.
    pub fn parse(json: &str) -> Option<Self> {
        let p: Value = serde_json::from_str(json).ok()?;
        if p.get("schema")?.as_i64()? != CURRENT_SCHEMA {
            return None;
        }
        let req = |name: &str| p.get(name)?.as_str().map(str::to_string);
        let opt = |name: &str| lim::str_of(&p, name);
        let kind = if opt("kind").is_empty() {
            PackKinds::MOD.to_string()
        } else {
            opt("kind")
        };
        let info = PackInfo {
            id: req("id")?,
            game: req("game")?,
            version: req("version")?,
            file: req("file")?,
            sha256: req("sha256")?.to_lowercase(),
            size: p.get("size")?.as_i64()?,
            launcher_min: opt("launcherMin"),
            upstream: opt("upstream"),
            depends_on: parse_dependencies(&p)?,
            kind,
        };
        let ok = is_safe_name(&info.id)
            && is_safe_name(&info.game)
            && lim::is_version(&info.version)
            && is_safe_file(&info.file)
            && info.sha256.len() == 64
            && info.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            && info.size > 0
            && PackKinds::is_known(&info.kind);
        ok.then(|| Self {
            schema: CURRENT_SCHEMA,
            packs: vec![info],
        })
    }
}

fn is_safe_name(s: &str) -> bool {
    lim::is_id(s)
}

fn is_safe_file(s: &str) -> bool {
    !s.is_empty() && s.len() <= 120 && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.')) && !s.contains("..")
}

/// Where a mod repository's catalog is: `https://github.com/<repo>/releases/latest/download/pack-index.json`.
pub fn index_url(repo: &str) -> Option<String> {
    lim::is_repo(repo).then(|| format!("https://github.com/{repo}/releases/latest/download/{INDEX_FILE_NAME}"))
}

/// Where a pack's zip is: `https://github.com/<repo>/releases/download/<version>/<file>`.
pub fn pack_url(repo: &str, pack: &PackInfo) -> Option<String> {
    lim::is_repo(repo).then(|| format!("https://github.com/{repo}/releases/download/{}/{}", pack.version, pack.file))
}

/// The newer pack to offer: the installed one is older than the catalog's latest.
pub fn update_for<'a>(installed: Option<&InstalledPack>, latest: Option<&'a PackInfo>) -> Option<&'a PackInfo> {
    let (installed, latest) = (installed?, latest?);
    let (a, b) = (Version::parse(&installed.version)?, Version::parse(&latest.version)?);
    (b > a).then_some(latest)
}

/// Whether this launcher is new enough for the pack (its `launcherMin`); true when the pack names none.
pub fn launcher_is_enough(launcher_min: Option<&str>, launcher_version: Option<&str>) -> bool {
    let Some(min) = launcher_min.filter(|m| !m.trim().is_empty()) else {
        return true;
    };
    match (Version::parse(min), Version::parse(&launcher_core(launcher_version))) {
        (Some(need), Some(have)) => have >= need,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(extra: &str) -> String {
        format!(
            r#"{{ "schema": 2, "id": "noita", "game": "noita", "version": "1.2.3", "file": "Cairn-Pack-noita-1.2.3.zip", "sha256": "{}", "size": 1234{extra} }}"#,
            "AB".repeat(32)
        )
    }

    #[test]
    fn a_catalog_is_read_and_its_checksum_lower_cased() {
        let i = PackIndex::parse(&index(r#", "launcherMin": "0.3.24", "upstream": "v1""#)).unwrap();
        let p = i.for_game("NOITA").unwrap();
        assert_eq!(
            (p.version.as_str(), p.size, p.launcher_min.as_str(), p.kind.as_str()),
            ("1.2.3", 1234, "0.3.24", "mod")
        );
        assert_eq!(p.sha256, "ab".repeat(32));
        assert!(i.for_game("other").is_none());
    }

    #[test]
    fn a_catalog_can_name_what_it_depends_on_and_its_kind() {
        let i = PackIndex::parse(&index(r#", "kind": "external", "dependsOn": [ { "id": "core", "min": "1.0.0" } ]"#)).unwrap();
        assert_eq!(i.packs[0].depends_on.len(), 1);
        assert!(i.for_game("noita").is_some());
        let content = PackIndex::parse(&index(r#", "kind": "content""#)).unwrap();
        assert!(content.for_game("noita").is_none()); // content is not a game's mod
    }

    #[test]
    fn a_bad_catalog_is_refused() {
        for json in [
            index("").replace("\"schema\": 2", "\"schema\": 1"),
            index("").replace("noita-1.2.3.zip", "../x.zip"),
            index("").replace("\"1.2.3\"", "\"1.2\""),
            index("").replace("\"id\": \"noita\"", "\"id\": \"a b\""),
            index("").replace("1234", "0"),
            index("").replace(&"AB".repeat(32), "ab"),
            index("").replace(&"AB".repeat(32), &"zz".repeat(32)),
            index(r#", "kind": "weird""#),
            index(r#", "dependsOn": "core""#),
            "not json".to_string(),
            "{}".to_string(),
        ] {
            assert!(PackIndex::parse(&json).is_none(), "{json}");
        }
    }

    #[test]
    fn urls_follow_the_repository() {
        let p = PackIndex::parse(&index("")).unwrap().packs.remove(0);
        assert_eq!(
            index_url("Cairn-client/mod-noita").unwrap(),
            "https://github.com/Cairn-client/mod-noita/releases/latest/download/pack-index.json"
        );
        assert_eq!(
            pack_url("Cairn-client/mod-noita", &p).unwrap(),
            "https://github.com/Cairn-client/mod-noita/releases/download/1.2.3/Cairn-Pack-noita-1.2.3.zip"
        );
        assert!(index_url("https://evil.test/x").is_none());
        assert!(pack_url("nobody", &p).is_none());
    }

    #[test]
    fn an_update_is_offered_only_for_a_newer_version() {
        let latest = PackIndex::parse(&index("")).unwrap().packs.remove(0);
        let installed = |v: &str| InstalledPack {
            id: "noita".into(),
            version: v.into(),
            files: vec![],
            contract: None,
        };
        assert!(update_for(Some(&installed("1.2.2")), Some(&latest)).is_some());
        assert!(update_for(Some(&installed("1.2.3")), Some(&latest)).is_none());
        assert!(update_for(Some(&installed("1.10.0")), Some(&latest)).is_none());
        assert!(update_for(None, Some(&latest)).is_none());
        assert!(update_for(Some(&installed("garbage")), Some(&latest)).is_none());
    }

    #[test]
    fn the_launcher_must_be_new_enough() {
        assert!(launcher_is_enough(None, Some("0.1.0")));
        assert!(launcher_is_enough(Some(""), None));
        assert!(launcher_is_enough(Some("0.3.24"), Some("0.3.24")));
        assert!(launcher_is_enough(Some("0.3.24"), Some("0.3.38+abc")));
        assert!(!launcher_is_enough(Some("0.3.24"), Some("0.3.23")));
        assert!(!launcher_is_enough(Some("0.3.24"), None));
        assert!(!launcher_is_enough(Some("nonsense"), Some("0.3.38")));
    }
}
