// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! `System.Version` as the C# side compared it: two to four numeric components, a missing one is smaller than a zero.

use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    parts: [i64; 4],
}

impl Version {
    /// `1.2`, `1.2.3` or `1.2.3.4` (each component a non-negative number); anything else is `None`.
    pub fn parse(s: &str) -> Option<Self> {
        let segments: Vec<&str> = s.trim().split('.').collect();
        if !(2..=4).contains(&segments.len()) {
            return None;
        }
        let mut parts = [-1i64; 4];
        for (i, seg) in segments.iter().enumerate() {
            if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            parts[i] = seg.parse::<i32>().ok()? as i64;
        }
        Some(Self { parts })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts.cmp(&other.parts)
    }
}

/// The launcher's own version without a build suffix: `0.3.38+abc` and `0.3.38-beta` both give `0.3.38`.
pub fn launcher_core(version: Option<&str>) -> String {
    version.unwrap_or("").split(['+', '-']).next().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_dotnet() {
        let v = |s: &str| Version::parse(s).unwrap();
        assert!(v("0.3.9") < v("0.3.10"));
        assert!(v("1.2") < v("1.2.0"));
        assert!(v("1.2.0") < v("1.2.0.0"));
        assert_eq!(v("1.2.3"), v("1.2.3"));
        assert!(Version::parse("1").is_none());
        assert!(Version::parse("1.2.3.4.5").is_none());
        assert!(Version::parse("1.x").is_none());
        assert!(Version::parse("1.-2").is_none());
        assert!(Version::parse("").is_none());
    }

    #[test]
    fn the_launcher_version_loses_its_suffix() {
        assert_eq!(launcher_core(Some("0.3.38+abc")), "0.3.38");
        assert_eq!(launcher_core(Some("0.3.38-beta")), "0.3.38");
        assert_eq!(launcher_core(None), "");
    }
}
