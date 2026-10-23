// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The limits every catalog file is held to: plain ids, short text without markup, pictures that are plain file names.

use serde_json::Value;

pub const MAX_FILE: usize = 256 * 1024;

pub fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// Strict `MAJOR.MINOR.PATCH` with no leading zeros.
pub fn is_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (*p == "0" || !p.starts_with('0')))
}

/// A picture is a plain file name looked up beside the file that names it, never a URL.
pub fn is_image_path(s: &str) -> bool {
    if s.is_empty() || s.len() > 100 || s.starts_with('/') || s.contains("..") || s.contains(':') || s.contains('\\') {
        return false;
    }
    if !s
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'/'))
    {
        return false;
    }
    let ext = s.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
}

/// Short text without control characters or markup (`<` and `>` are dropped), cut to `max` characters.
pub fn text(s: &str, max: usize) -> String {
    let clean: String = s.chars().filter(|c| !c.is_control() && *c != '<' && *c != '>').collect();
    let clean = clean.trim();
    if clean.chars().count() <= max {
        clean.to_string()
    } else {
        clean.chars().take(max).collect::<String>().trim_end().to_string()
    }
}

/// `repo` is `owner/name` of a GitHub repository.
pub fn is_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else { return false };
    let owner_ok = (1..=39).contains(&owner.len())
        && owner.as_bytes()[0].is_ascii_alphanumeric()
        && owner.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-');
    let name_ok = (1..=100).contains(&name.len()) && name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'));
    owner_ok && name_ok
}

/// The elements of an array property; none when it is missing or not an array.
pub fn items<'a>(parent: &'a Value, name: &str) -> impl Iterator<Item = &'a Value> {
    parent.get(name).and_then(Value::as_array).into_iter().flatten()
}

/// A string property, or "" when it is missing or not a string.
pub fn str_of(e: &Value, name: &str) -> String {
    e.get(name).and_then(Value::as_str).unwrap_or("").to_string()
}

/// A `true` property.
pub fn flag(e: &Value, name: &str) -> bool {
    e.get(name) == Some(&Value::Bool(true))
}

pub fn u32_of(e: &Value, name: &str) -> Option<u32> {
    e.get(name).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_versions_pictures_and_repos() {
        assert!(is_id("dead-cells") && is_id("a_1") && !is_id("") && !is_id("a b") && !is_id(&"x".repeat(41)));
        assert!(is_version("0.3.38") && !is_version("0.3") && !is_version("01.2.3") && !is_version("1.2.3-beta") && !is_version("v1.2.3"));
        assert!(is_image_path("a.jpg") && is_image_path("shots/b-1.PNG") && !is_image_path("../a.jpg") && !is_image_path("http://x/a.jpg"));
        assert!(!is_image_path("a.gif") && !is_image_path("/a.jpg") && !is_image_path("a\\b.jpg"));
        assert!(
            is_repo("Cairn-client/mod-kcd2")
                && is_repo("a/b.c_d")
                && !is_repo("nobody")
                && !is_repo("-a/b")
                && !is_repo("a/b c")
                && !is_repo("a/")
        );
    }

    #[test]
    fn text_loses_markup_and_is_cut() {
        assert_eq!(text("  <b>Hi</b>\u{7} there  ", 80), "bHi/b there");
        assert_eq!(text("abcdef", 3), "abc");
        assert_eq!(text("ab  cd", 3), "ab");
    }
}
