// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Compares two release version strings (`Major.Minor.Patch`, optionally with a fourth build component) so a mismatch notice can say
//! who is behind. This is release versioning (the `VERSION` file), not the relay's wire protocol byte.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseVersionComparison {
    Equal,
    LocalIsOlder,
    LocalIsNewer,
    /// Either string did not parse: say "versions don't match" rather than guess a direction.
    Ambiguous,
}

pub fn compare(local: &str, remote: &str) -> ReleaseVersionComparison {
    let (Some(l), Some(r)) = (parse(local), parse(remote)) else {
        return ReleaseVersionComparison::Ambiguous;
    };
    for i in 0..4 {
        if l[i] != r[i] {
            return if l[i] < r[i] {
                ReleaseVersionComparison::LocalIsOlder
            } else {
                ReleaseVersionComparison::LocalIsNewer
            };
        }
    }
    ReleaseVersionComparison::Equal
}

fn parse(s: &str) -> Option<[i32; 4]> {
    if s.trim().is_empty() {
        return None;
    }
    let segments: Vec<&str> = s.trim().split('.').collect();
    if segments.len() < 2 || segments.len() > 4 {
        return None;
    }
    let mut parts = [0i32; 4];
    for (i, seg) in segments.iter().enumerate() {
        parts[i] = seg.trim().parse::<i32>().ok()?;
    }
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReleaseVersionComparison::*;

    #[test]
    fn compares_component_by_component() {
        assert_eq!(compare("0.3.38", "0.3.38"), Equal);
        assert_eq!(compare("0.3.37", "0.3.38"), LocalIsOlder);
        assert_eq!(compare("0.4.0", "0.3.38"), LocalIsNewer);
        assert_eq!(compare("0.3.38", "0.3.38.1"), LocalIsOlder);
        assert_eq!(compare("1.0", "1.0.0.0"), Equal);
    }

    #[test]
    fn a_string_that_does_not_parse_is_ambiguous() {
        assert_eq!(compare("", "1.0.0"), Ambiguous);
        assert_eq!(compare("1", "1.0.0"), Ambiguous);
        assert_eq!(compare("1.0.0.0.0", "1.0.0"), Ambiguous);
        assert_eq!(compare("a.b.c", "1.0.0"), Ambiguous);
        assert_eq!(compare("1.0.0-beta", "1.0.0"), Ambiguous);
    }
}
