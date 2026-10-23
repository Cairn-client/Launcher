// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The launcher's own release notes, from GitHub's list of releases (kept on disk for when there is no network).

use cairn_setup::http::Http;
use cairn_setup::time_util;
use cairn_setup::version::Version;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub const URL: &str = "https://api.github.com/repos/Cairn-client/Launcher/releases?per_page=30";
pub const PAGE_URL: &str = "https://github.com/Cairn-client/Launcher/releases";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseNote {
    pub tag: String,
    /// When it was published, as seconds since 1970-01-01 UTC.
    pub published: Option<i64>,
    pub prerelease: bool,
    pub items: Vec<String>,
}

/// The releases, newest first; nothing when the text is not GitHub's list.
pub fn parse(json: &str) -> Vec<ReleaseNote> {
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(json) else {
        return vec![];
    };
    let mut out: Vec<(Option<Version>, ReleaseNote)> = Vec::new();
    for r in &list {
        if r.get("draft") == Some(&Value::Bool(true)) {
            continue;
        }
        let tag = r.get("tag_name").and_then(Value::as_str).unwrap_or("");
        if tag.is_empty() {
            continue;
        }
        let published = r.get("published_at").and_then(Value::as_str).and_then(time_util::parse_utc);
        let prerelease = r.get("prerelease") == Some(&Value::Bool(true));
        let body = r.get("body").and_then(Value::as_str).unwrap_or("");
        let v = Version::parse(tag.trim_start_matches(['v', 'V']));
        out.push((
            v,
            ReleaseNote {
                tag: tag.to_string(),
                published,
                prerelease,
                items: items(body),
            },
        ));
    }
    let zero = Version::parse("0.0").unwrap();
    out.sort_by(|a, b| {
        b.0.unwrap_or(zero)
            .cmp(&a.0.unwrap_or(zero))
            .then_with(|| b.1.published.cmp(&a.1.published))
    });
    out.into_iter().map(|(_, n)| n).collect()
}

/// The bullet lines of a release's text, without markdown marks.
pub fn items(body: &str) -> Vec<String> {
    body.replace('\r', "")
        .split('\n')
        .map(str::trim)
        .filter(|l| l.starts_with("- ") || l.starts_with("* "))
        .map(|l| l[2..].replace("**", "").replace('`', "").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The launcher's releases from GitHub, or the copy kept on disk when it cannot be reached.
pub fn fetch(http: &dyn Http, cache_file: Option<&Path>) -> Option<Vec<ReleaseNote>> {
    if let Some(json) = get(http, URL) {
        let notes = parse(&json);
        if !notes.is_empty() {
            if let Some(f) = cache_file {
                if let Some(dir) = f.parent() {
                    let _ = fs::create_dir_all(dir);
                }
                let _ = fs::write(f, &json);
            }
            return Some(notes);
        }
    }
    let notes = parse(&String::from_utf8_lossy(&fs::read(cache_file?).ok()?));
    (!notes.is_empty()).then_some(notes)
}

/// The latest few releases of another repository (a game's mod).
pub fn fetch_repo(http: &dyn Http, repo: &str, count: usize) -> Option<Vec<ReleaseNote>> {
    if !cairn_setup::catalog_limits::is_repo(repo) {
        return None;
    }
    let json = get(http, &format!("https://api.github.com/repos/{repo}/releases?per_page={count}"))?;
    Some(parse(&json).into_iter().take(count).collect())
}

fn get(http: &dyn Http, url: &str) -> Option<String> {
    let resp = http
        .get_with(url, Some("application/vnd.github+json"))
        .ok()
        .filter(|r| r.is_success())?;
    resp.text(2 << 20).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_setup::http::fake::FakeHttp;

    const LIST: &str = r#"[
        {"tag_name":"0.3.37","published_at":"2026-10-20T10:00:00Z","prerelease":false,"body":"- Rename project\r\n- **Fix** the `bar`\r\nnot a bullet\r\n* Star bullet"},
        {"tag_name":"0.3.38","published_at":"2026-10-23T12:17:57Z","prerelease":false,"body":"- A mod can start with arguments"},
        {"tag_name":"0.4.0-beta","published_at":"2026-10-24T00:00:00Z","prerelease":true,"body":""},
        {"tag_name":"0.3.99","draft":true,"body":"- not yet"},
        {"tag_name":"","body":"- nameless"}
    ]"#;

    #[test]
    fn releases_come_newest_first_with_their_bullet_lines() {
        let n = parse(LIST);
        assert_eq!(
            n.iter().map(|r| r.tag.as_str()).collect::<Vec<_>>(),
            vec!["0.3.38", "0.3.37", "0.4.0-beta"] // a tag that is not a plain version sorts last, as before
        );
        assert!(n[2].prerelease && n[2].items.is_empty());
        assert_eq!(n[1].items, vec!["Rename project", "Fix the bar", "Star bullet"]);
        assert_eq!(n[0].published, Some(1_792_757_877));
        assert!(parse("not json").is_empty() && parse("{}").is_empty());
    }

    #[test]
    fn the_list_is_kept_for_offline_and_used_then() {
        let d = std::env::temp_dir().join(format!("cairn-changelog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let cache = d.join("changelog-cache.json");
        let online = FakeHttp::serving(URL, LIST.as_bytes());
        assert_eq!(fetch(&online, Some(&cache)).unwrap().len(), 3);
        assert!(cache.exists());
        let offline = FakeHttp::default();
        assert_eq!(fetch(&offline, Some(&cache)).unwrap().len(), 3);
        assert!(fetch(&offline, None).is_none());
        assert!(fetch(&offline, Some(&d.join("none.json"))).is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_mods_latest_releases_are_cut_to_the_count() {
        let h = FakeHttp::serving("https://api.github.com/repos/o/mod-x/releases?per_page=2", LIST.as_bytes());
        assert_eq!(fetch_repo(&h, "o/mod-x", 2).unwrap().len(), 2);
        assert!(fetch_repo(&h, "nobody", 2).is_none());
    }
}
