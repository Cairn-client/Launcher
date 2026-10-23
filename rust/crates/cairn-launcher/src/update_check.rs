// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Looking for a newer launcher, and installing it. Nothing is downloaded or installed by itself: the player presses "Check for updates", sees
//! what is new, and presses "Update now". The installer is fetched from this repository's own address, checked against the SHA-256 GitHub lists
//! for it, and run silently over this install.

use cairn_setup::http::Http;
use cairn_setup::version::Version;
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

const RELEASES_URL: &str = "https://api.github.com/repos/Cairn-client/Launcher/releases?per_page=8";
const DOWNLOAD_PREFIX: &str = "https://github.com/Cairn-client/Launcher/releases/download/";
/// The newest release publishes this file; a plain download is not held to the API's 60 requests an hour for one address.
const MANIFEST_URL: &str = "https://github.com/Cairn-client/Launcher/releases/latest/download/update.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub tag: String,
    pub page_url: String,
    pub download_url: Option<String>,
    pub sha256: Option<String>,
    pub size: i64,
    pub notes: String,
}

/// The version of a release tag or of the running launcher (`v0.3.1`, `0.3.1+abc`: the numbers before any suffix); `None` when it is not one.
pub fn normalize(s: &str) -> Option<Version> {
    let t = s.trim().trim_start_matches(['v', 'V']);
    let t = t.split(['+', '-']).next().unwrap_or("");
    Version::parse(t)
}

/// The newest release in GitHub's list that is newer than `current`, with its installer.
pub fn newer(releases_json: &str, current: &str) -> Option<UpdateInfo> {
    let cur = normalize(current)?;
    let list: Value = serde_json::from_str(releases_json).ok()?;
    let sha_in_body = Regex::new("sha256 `([0-9a-fA-F]{64})`").ok()?;
    let mut best: Option<(Version, UpdateInfo)> = None;
    for r in list.as_array()? {
        if r.get("draft") == Some(&Value::Bool(true)) {
            continue;
        }
        let Some(tag) = r.get("tag_name").and_then(Value::as_str) else {
            continue;
        };
        let Some(v) = normalize(tag) else { continue };
        if best.as_ref().is_some_and(|(b, _)| v <= *b) {
            continue;
        }
        let page = r.get("html_url").and_then(Value::as_str).unwrap_or("").to_string();
        let body = r.get("body").and_then(Value::as_str).unwrap_or("");
        let (mut url, mut sha, mut size) = (None, None, 0i64);
        for a in r.get("assets").and_then(Value::as_array).into_iter().flatten() {
            let name = a.get("name").and_then(Value::as_str).unwrap_or("");
            if !name.starts_with("Cairn-Setup-") || !name.ends_with(".exe") {
                continue;
            }
            url = a.get("browser_download_url").and_then(Value::as_str).map(str::to_string);
            size = a.get("size").and_then(Value::as_i64).unwrap_or(0);
            sha = a
                .get("digest")
                .and_then(Value::as_str)
                .filter(|d| d.to_lowercase().starts_with("sha256:"))
                .map(|d| d[7..].to_lowercase());
            break;
        }
        sha = sha.or_else(|| sha_in_body.captures(body).map(|c| c[1].to_lowercase()));
        // only this repository's own files
        if url
            .as_ref()
            .is_some_and(|u| !u.to_lowercase().starts_with(&DOWNLOAD_PREFIX.to_lowercase()))
        {
            url = None;
        }
        best = Some((
            v,
            UpdateInfo {
                tag: tag.to_string(),
                page_url: page,
                download_url: url,
                sha256: sha,
                size,
                notes: short_notes(body),
            },
        ));
    }
    best.filter(|(v, _)| *v > cur).map(|(_, i)| i)
}

/// The release notes for a message box: no checksum line, no markdown marks, at most ~700 characters.
pub fn short_notes(body: &str) -> String {
    let keep: Vec<String> = body
        .replace('\r', "")
        .split('\n')
        .filter(|l| !l.to_lowercase().contains("sha256"))
        .map(|l| l.replace("**", "").replace('`', "").trim_start_matches(['#', ' ']).to_string())
        .filter(|l| !l.trim().is_empty())
        .collect();
    let s = keep.join("\n");
    if s.chars().count() <= 700 {
        s
    } else {
        format!("{}...", s.chars().take(700).collect::<String>().trim_end())
    }
}

/// What a look for an update found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Newer(UpdateInfo),
    Current,
    /// GitHub could not be asked (offline, rate limit): "nothing new" was not an answer.
    Unreachable,
}

/// Asks GitHub: the manifest of the newest release first, then the API.
pub fn check(http: &dyn Http, current: &str) -> Check {
    let manifest = http
        .get_with(MANIFEST_URL, Some("application/vnd.github+json"))
        .ok()
        .filter(|r| r.is_success())
        .and_then(|r| r.text(1 << 20).ok())
        .filter(|t| {
            serde_json::from_str::<Value>(t)
                .ok()
                .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                .unwrap_or(false)
        });
    let json = match manifest {
        Some(m) => m,
        None => match http
            .get_with(RELEASES_URL, Some("application/vnd.github+json"))
            .ok()
            .filter(|r| r.is_success())
            .and_then(|r| r.text(4 << 20).ok())
        {
            Some(j) => j,
            None => return Check::Unreachable,
        },
    };
    match newer(&json, current) {
        Some(i) => Check::Newer(i),
        None => Check::Current,
    }
}

/// Downloads the installer into the temp folder and checks it; `Ok(path)` or why not.
pub fn download(info: &UpdateInfo, http: &dyn Http, progress: &mut dyn FnMut(f64), cancel: &AtomicBool) -> Result<PathBuf, String> {
    let (Some(url), Some(want)) = (&info.download_url, &info.sha256) else {
        return Err("this release has no installer file with a checksum".into());
    };
    let dir = std::env::temp_dir().join("Cairn-update");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!(
        "Cairn-Setup-{}.exe",
        info.tag
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .collect::<String>()
    ));
    let result = (|| {
        let resp = http.get(url).map_err(|e| format!("the download failed ({e})"))?;
        if !resp.is_success() {
            return Err(format!("the download server answered {}", resp.status));
        }
        let total = resp.content_length.map(|n| n as i64).unwrap_or(info.size).max(1);
        let mut src = resp.body;
        let mut dst = fs::File::create(&file).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let (mut buf, mut got) = (vec![0u8; 256 * 1024], 0i64);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let n = src.read(&mut buf).map_err(|e| format!("the download failed ({e})"))?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            dst.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            got += n as i64;
            progress((got as f64 / total as f64).min(1.0));
        }
        let have: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if !have.eq_ignore_ascii_case(want) {
            return Err("the downloaded file does not match its checksum".into());
        }
        Ok(file.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&file);
    }
    result
}

/// Runs the installer silently over this install; it replaces the launcher's files and opens the new one. The caller closes the launcher.
pub fn start_installer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new(path)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/RELAUNCH=1"])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_setup::http::fake::FakeHttp;

    fn release(tag: &str, sha: &str, url_tag: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/Cairn-client/Launcher/releases/tag/{tag}","body":"- A change\n- **Bold** `code`\nsha256 `{sha}`","assets":[{{"name":"Cairn-Setup-{tag}.exe","size":1000,"browser_download_url":"https://github.com/Cairn-client/Launcher/releases/download/{url_tag}/Cairn-Setup-{tag}.exe"}}]}}"#
        )
    }

    #[test]
    fn versions_read_the_numbers_before_any_suffix() {
        assert_eq!(normalize("v0.3.38"), Version::parse("0.3.38"));
        assert_eq!(normalize("0.3.38+abc123"), Version::parse("0.3.38"));
        assert_eq!(normalize("0.3.38-beta"), Version::parse("0.3.38"));
        assert!(normalize("nonsense").is_none() && normalize("").is_none());
    }

    #[test]
    fn the_newest_release_newer_than_this_one_is_offered_with_its_installer() {
        let sha = "ab".repeat(32);
        let json = format!(
            "[{},{},{}]",
            release("0.3.37", &sha, "0.3.37"),
            release("0.3.40", &sha, "0.3.40"),
            release("0.3.38", &sha, "0.3.38")
        );
        let i = newer(&json, "0.3.38").unwrap();
        assert_eq!((i.tag.as_str(), i.size), ("0.3.40", 1000));
        assert_eq!(i.sha256.as_deref(), Some(sha.as_str())); // from the notes, when GitHub gave no digest
        assert!(i.download_url.unwrap().ends_with("0.3.40/Cairn-Setup-0.3.40.exe"));
        assert_eq!(i.notes, "- A change\n- Bold code");
        assert!(newer(&json, "0.3.40").is_none() && newer(&json, "0.4.0+x").is_none());
        assert!(newer(&json, "garbage").is_none() && newer("not json", "0.1.0").is_none());
    }

    #[test]
    fn an_installer_from_another_address_is_not_used_and_a_digest_beats_the_notes() {
        let json = format!(
            "[{}]",
            release("0.5.0", &"cd".repeat(32), "0.5.0")
                .replace("https://github.com/Cairn-client/Launcher/releases/download/", "https://evil.test/")
        );
        assert!(newer(&json, "0.1.0").unwrap().download_url.is_none());
        let with_digest = r#"[{"tag_name":"0.5.0","assets":[{"name":"Cairn-Setup-0.5.0.exe","size":5,"digest":"sha256:ABCDEF","browser_download_url":"https://github.com/cairn-client/launcher/releases/download/0.5.0/Cairn-Setup-0.5.0.exe"}]}]"#;
        let i = newer(with_digest, "0.1.0").unwrap();
        assert_eq!(i.sha256.as_deref(), Some("abcdef"));
        assert!(i.download_url.is_some()); // the old lower-case spelling of the repository is accepted too
        assert!(newer(r#"[{"tag_name":"0.5.0","draft":true}]"#, "0.1.0").is_none());
    }

    #[test]
    fn a_check_uses_the_manifest_then_the_api_and_says_when_it_could_not_ask() {
        let sha = "ab".repeat(32);
        let h = FakeHttp::serving(MANIFEST_URL, format!("[{}]", release("0.9.0", &sha, "0.9.0")).as_bytes());
        assert!(matches!(check(&h, "0.3.38"), Check::Newer(i) if i.tag == "0.9.0"));
        assert_eq!(check(&h, "0.9.0"), Check::Current);
        let api = FakeHttp::serving(RELEASES_URL, format!("[{}]", release("0.8.0", &sha, "0.8.0")).as_bytes());
        assert!(matches!(check(&api, "0.3.38"), Check::Newer(i) if i.tag == "0.8.0"));
        let empty_manifest = FakeHttp::serving(MANIFEST_URL, b"[]");
        empty_manifest.put(RELEASES_URL, 200, format!("[{}]", release("0.8.0", &sha, "0.8.0")).as_bytes());
        assert!(matches!(check(&empty_manifest, "0.1.0"), Check::Newer(_)));
        assert_eq!(check(&FakeHttp::default(), "0.3.38"), Check::Unreachable);
    }

    #[test]
    fn the_installer_is_downloaded_checked_and_a_wrong_one_is_deleted() {
        let body = b"pretend installer".to_vec();
        let sha: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
        let url = "https://github.com/Cairn-client/Launcher/releases/download/9.9.9/Cairn-Setup-9.9.9.exe";
        let h = FakeHttp::serving(url, &body);
        let info = UpdateInfo {
            tag: "9.9.9".into(),
            page_url: String::new(),
            download_url: Some(url.into()),
            sha256: Some(sha),
            size: body.len() as i64,
            notes: String::new(),
        };
        let mut seen = 0.0;
        let path = download(&info, &h, &mut |p| seen = p, &AtomicBool::new(false)).unwrap();
        assert_eq!((fs::read(&path).unwrap(), seen), (body.clone(), 1.0));
        let _ = fs::remove_file(&path);
        let wrong = UpdateInfo {
            sha256: Some("0".repeat(64)),
            ..info.clone()
        };
        assert_eq!(
            download(&wrong, &h, &mut |_| {}, &AtomicBool::new(false)).unwrap_err(),
            "the downloaded file does not match its checksum"
        );
        assert!(!std::env::temp_dir().join("Cairn-update").join("Cairn-Setup-9.9.9.exe").exists());
        assert!(download(
            &UpdateInfo {
                sha256: None,
                ..info.clone()
            },
            &h,
            &mut |_| {},
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(download(&info, &h, &mut |_| {}, &AtomicBool::new(true)).unwrap_err(), "cancelled");
        assert!(download(&info, &FakeHttp::default(), &mut |_| {}, &AtomicBool::new(false))
            .unwrap_err()
            .contains("404"));
    }
}
