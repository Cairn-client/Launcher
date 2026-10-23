// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! What to download for a pack, and from which channel: its dependencies first (only when missing or too old), content copied without
//! overwriting, and the stable or beta release of a mod.

use super::index::{index_url, PackIndex, PackInfo};
use super::store::InstalledPack;
use crate::catalog_limits as lim;
use crate::http::Http;
use crate::version::Version;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const MAX_DEPTH: usize = 4;

/// The packs to download, in order, or why it cannot be done.
#[derive(Debug, Clone, PartialEq)]
pub struct PackPlan {
    pub order: Vec<PackInfo>,
    pub problem: Option<String>,
}

impl PackPlan {
    pub fn ok(&self) -> bool {
        self.problem.is_none()
    }
}

/// `installed` says what the store has, `offered` what the catalogs offer by pack id.
pub fn plan(pack: &PackInfo, installed: &dyn Fn(&str) -> Option<InstalledPack>, offered: &dyn Fn(&str) -> Option<PackInfo>) -> PackPlan {
    let mut order: Vec<PackInfo> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut path: Vec<String> = Vec::new();
    match visit(pack, 0, installed, offered, &mut order, &mut seen, &mut path) {
        None => PackPlan { order, problem: None },
        Some(problem) => PackPlan {
            order: Vec::new(),
            problem: Some(problem),
        },
    }
}

fn visit(
    p: &PackInfo,
    depth: usize,
    installed: &dyn Fn(&str) -> Option<InstalledPack>,
    offered: &dyn Fn(&str) -> Option<PackInfo>,
    order: &mut Vec<PackInfo>,
    seen: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Option<String> {
    if depth > MAX_DEPTH {
        return Some(format!("{} depends on too many packs", p.id));
    }
    if path.iter().any(|x| x.eq_ignore_ascii_case(&p.id)) {
        return Some(format!("{} depends on itself", p.id));
    }
    path.push(p.id.clone());
    for dep in &p.depends_on {
        if installed(&dep.id).is_some_and(|i| is_enough(&i.version, &dep.min)) || seen.contains(&dep.id.to_lowercase()) {
            continue;
        }
        let Some(o) = offered(&dep.id) else {
            return Some(format!("{} needs {} {}, which is not available", p.id, dep.id, dep.min));
        };
        if !is_enough(&o.version, &dep.min) {
            return Some(format!(
                "{} needs {} {}, and only {} is available",
                p.id, dep.id, dep.min, o.version
            ));
        }
        if let Some(sub) = visit(&o, depth + 1, installed, offered, order, seen, path) {
            return Some(sub);
        }
    }
    path.pop();
    if seen.insert(p.id.to_lowercase()) {
        order.push(p.clone());
    }
    None
}

fn is_enough(version: &str, min: &str) -> bool {
    matches!((Version::parse(version), Version::parse(min)), (Some(have), Some(need)) if have >= need)
}

/// Copies the files of `source` into `target` that `target` does not have; what the player has is never overwritten. Returns how many were copied.
pub fn copy_missing(source: &Path, target: &Path) -> usize {
    if !source.is_dir() {
        return 0;
    }
    let mut copied = 0;
    let mut stack = vec![source.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let Ok(rel) = p.strip_prefix(source) else { continue };
            let dest = target.join(rel);
            if dest.exists() {
                continue;
            }
            if let Some(parent) = dest.parent() {
                if fs::create_dir_all(parent).is_err() {
                    continue;
                }
            }
            if fs::copy(&p, &dest).is_ok() {
                copied += 1;
            }
        }
    }
    copied
}

pub const STABLE: &str = "stable";
pub const BETA: &str = "beta";

pub fn is_known_channel(channel: Option<&str>) -> bool {
    matches!(channel, Some(STABLE | BETA))
}

/// The newest release tag (a bare `X.Y.Z`, pre-releases included) of the repository, from GitHub's release list.
pub fn newest_tag(http: &dyn Http, repo: &str) -> Option<String> {
    if !lim::is_repo(repo) {
        return None;
    }
    let resp = http
        .get_with(
            &format!("https://api.github.com/repos/{repo}/releases?per_page=10"),
            Some("application/vnd.github+json"),
        )
        .ok()?;
    if !resp.is_success() {
        return None;
    }
    let list: Value = serde_json::from_str(&resp.text(1 << 20).ok()?).ok()?;
    list.as_array()?
        .iter()
        .filter(|rel| rel.get("draft") != Some(&Value::Bool(true)))
        .filter_map(|rel| rel.get("tag_name").and_then(Value::as_str))
        .find(|tag| lim::is_version(tag))
        .map(str::to_string)
}

/// The catalog of the newest release (pre-releases included) of a mod repository.
pub fn fetch_beta(http: &dyn Http, repo: &str) -> Option<PackIndex> {
    let tag = newest_tag(http, repo)?;
    let resp = http
        .get(&format!(
            "https://github.com/{repo}/releases/download/{tag}/{}",
            super::index::INDEX_FILE_NAME
        ))
        .ok()?;
    if !resp.is_success() {
        return None;
    }
    PackIndex::parse(&resp.text(1 << 20).ok()?)
}

/// The catalog of the latest stable release of a mod repository.
pub fn fetch_stable(http: &dyn Http, repo: &str) -> Option<PackIndex> {
    let resp = http.get(&index_url(repo)?).ok()?;
    if !resp.is_success() {
        return None;
    }
    PackIndex::parse(&resp.text(1 << 20).ok()?)
}

/// Which release to offer: the beta only when the player follows beta and it is newer than the stable one.
pub fn choose<'a>(channel: Option<&str>, stable: Option<&'a PackInfo>, beta: Option<&'a PackInfo>) -> Option<&'a PackInfo> {
    let Some(beta) = beta.filter(|_| channel == Some(BETA)) else {
        return stable;
    };
    let Some(stable) = stable else { return Some(beta) };
    match (Version::parse(&beta.version), Version::parse(&stable.version)) {
        (Some(b), Some(s)) if b > s => Some(beta),
        _ => Some(stable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::packs::contract::PackDependency;
    use crate::packs::store::tests::temp;
    use std::collections::HashMap;

    fn pack(id: &str, version: &str, deps: &[(&str, &str)]) -> PackInfo {
        PackInfo {
            id: id.into(),
            game: "g".into(),
            version: version.into(),
            file: format!("{id}.zip"),
            sha256: "a".repeat(64),
            size: 10,
            launcher_min: String::new(),
            upstream: String::new(),
            depends_on: deps
                .iter()
                .map(|(i, m)| PackDependency {
                    id: i.to_string(),
                    min: m.to_string(),
                })
                .collect(),
            kind: "mod".into(),
        }
    }
    fn have(id: &str, version: &str) -> InstalledPack {
        InstalledPack {
            id: id.into(),
            version: version.into(),
            files: vec![],
            contract: None,
        }
    }
    fn ids(plan: &PackPlan) -> Vec<&str> {
        plan.order.iter().map(|p| p.id.as_str()).collect()
    }

    #[test]
    fn a_pack_with_no_dependencies_is_alone() {
        let p = plan(&pack("mod", "1.0.0", &[]), &|_| None, &|_| None);
        assert!(p.ok());
        assert_eq!(ids(&p), vec!["mod"]);
    }

    #[test]
    fn what_it_depends_on_comes_first_and_only_when_missing_or_too_old() {
        let m = pack("mod", "1.0.0", &[("core", "1.2.0"), ("saves", "1.0.0")]);
        let offered: HashMap<&str, PackInfo> = [("core", pack("core", "1.3.0", &[])), ("saves", pack("saves", "1.0.0", &[]))].into();
        let off = |id: &str| offered.get(id).cloned();
        assert_eq!(ids(&plan(&m, &|_| None, &off)), vec!["core", "saves", "mod"]);
        let core_old = |id: &str| {
            Some(if id == "core" {
                have("core", "1.1.0")
            } else {
                have("saves", "1.0.0")
            })
        };
        assert_eq!(ids(&plan(&m, &core_old, &off)), vec!["core", "mod"]);
        let enough = |id: &str| {
            Some(if id == "core" {
                have("core", "1.2.0")
            } else {
                have("saves", "2.0.0")
            })
        };
        assert_eq!(ids(&plan(&m, &enough, &off)), vec!["mod"]);
    }

    #[test]
    fn a_dependency_of_a_dependency_is_followed_and_shared_ones_are_once() {
        let offered: HashMap<&str, PackInfo> = [
            ("a", pack("a", "1.0.0", &[("base", "1.0.0")])),
            ("b", pack("b", "1.0.0", &[("base", "1.0.0")])),
            ("base", pack("base", "1.0.0", &[])),
        ]
        .into();
        let p = plan(&pack("mod", "1.0.0", &[("a", "1.0.0"), ("b", "1.0.0")]), &|_| None, &|id| {
            offered.get(id).cloned()
        });
        assert_eq!(ids(&p), vec!["base", "a", "b", "mod"]);
    }

    #[test]
    fn a_dependency_that_cannot_be_had_is_a_problem() {
        let missing = plan(&pack("mod", "1.0.0", &[("core", "1.0.0")]), &|_| None, &|_| None);
        assert!(!missing.ok() && missing.problem.as_deref().unwrap().contains("core") && missing.order.is_empty());
        let too_old = plan(&pack("mod", "1.0.0", &[("core", "2.0.0")]), &|_| None, &|id| {
            Some(pack(id, "1.5.0", &[]))
        });
        assert!(!too_old.ok() && too_old.problem.as_deref().unwrap().contains("1.5.0"));
    }

    #[test]
    fn a_loop_of_dependencies_is_a_problem_not_a_hang() {
        let offered: HashMap<&str, PackInfo> = [
            ("a", pack("a", "1.0.0", &[("b", "1.0.0")])),
            ("b", pack("b", "1.0.0", &[("a", "1.0.0")])),
        ]
        .into();
        let p = plan(&pack("a", "1.0.0", &[("b", "1.0.0")]), &|_| None, &|id| offered.get(id).cloned());
        assert!(!p.ok() && p.problem.as_deref().unwrap().contains("itself"));
    }

    #[test]
    fn content_is_copied_but_never_overwrites_what_the_player_has() {
        let root = temp("content");
        let (src, dst) = (root.join("src"), root.join("dst"));
        fs::create_dir_all(src.join("knight")).unwrap();
        fs::create_dir_all(dst.join("knight")).unwrap();
        fs::write(src.join("knight").join("a.whs"), "new").unwrap();
        fs::write(src.join("knight").join("b.whs"), "new").unwrap();
        fs::write(dst.join("knight").join("a.whs"), "mine").unwrap();
        assert_eq!(copy_missing(&src, &dst), 1);
        assert_eq!(fs::read_to_string(dst.join("knight").join("a.whs")).unwrap(), "mine");
        assert_eq!(fs::read_to_string(dst.join("knight").join("b.whs")).unwrap(), "new");
        assert_eq!(copy_missing(&src, &dst), 0);
        assert_eq!(copy_missing(&root.join("nothing"), &dst), 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_beta_channel_follows_the_newest_release_and_stable_only_the_stable_one() {
        let v = |s: &str| pack("m", s, &[]);
        let version = |c: Option<&str>, s: &str, b: &str| choose(c, Some(&v(s)), Some(&v(b))).unwrap().version.clone();
        assert_eq!(version(Some(BETA), "0.1.0", "0.2.0"), "0.2.0");
        assert_eq!(version(Some(STABLE), "0.1.0", "0.2.0"), "0.1.0");
        assert_eq!(version(Some(BETA), "0.1.0", "0.1.0"), "0.1.0");
        assert_eq!(version(Some(BETA), "0.3.0", "0.2.0"), "0.3.0"); // a beta that is older than stable is never offered
        assert!(choose(Some(BETA), None, None).is_none());
        assert_eq!(choose(Some(BETA), None, Some(&v("0.2.0"))).unwrap().version, "0.2.0");
        assert_eq!(choose(Some(BETA), Some(&v("0.1.0")), None).unwrap().version, "0.1.0");
        assert!(choose(Some(STABLE), None, Some(&v("0.2.0"))).is_none());
        assert!(
            is_known_channel(Some("beta"))
                && is_known_channel(Some("stable"))
                && !is_known_channel(Some("nightly"))
                && !is_known_channel(None)
        );
    }

    #[test]
    fn the_newest_tag_skips_drafts_and_odd_tags() {
        let h = FakeHttp::serving(
            "https://api.github.com/repos/o/r/releases?per_page=10",
            br#"[ {"tag_name":"0.4.0","draft":true}, {"tag_name":"v0.3.0"}, {"tag_name":"0.3.1"}, {"tag_name":"0.3.0"} ]"#,
        );
        assert_eq!(newest_tag(&h, "o/r").as_deref(), Some("0.3.1"));
        assert_eq!(newest_tag(&h, "nobody"), None);
        assert_eq!(newest_tag(&FakeHttp::default(), "o/r"), None);
    }

    #[test]
    fn the_beta_catalog_is_the_one_of_the_newest_release() {
        let h = FakeHttp::serving(
            "https://api.github.com/repos/o/r/releases?per_page=10",
            br#"[ {"tag_name":"0.3.1"} ]"#,
        );
        let index = format!(
            r#"{{ "schema": 2, "id": "m", "game": "m", "version": "0.3.1", "file": "m.zip", "sha256": "{}", "size": 5 }}"#,
            "a".repeat(64)
        );
        h.put(
            "https://github.com/o/r/releases/download/0.3.1/pack-index.json",
            200,
            index.as_bytes(),
        );
        assert_eq!(fetch_beta(&h, "o/r").unwrap().packs[0].version, "0.3.1");
        h.put(
            "https://github.com/o/r/releases/latest/download/pack-index.json",
            200,
            index.replace("0.3.1", "0.2.0").as_bytes(),
        );
        assert_eq!(fetch_stable(&h, "o/r").unwrap().packs[0].version, "0.2.0");
        assert!(fetch_stable(&FakeHttp::default(), "o/r").is_none());
    }
}
