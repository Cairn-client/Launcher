// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The list of files a pack or installer carries: `KIND|path|size|sha256`, one per line.

use std::fs;
use std::io;
use std::path::Path;

pub const MANIFEST_NAME: &str = "install-manifest.txt";
pub const VERDICT_NAME: &str = "install-verify.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// `MOD` or `BIN`, upper-cased.
    pub kind: String,
    pub rel: String,
    pub size: i64,
    /// Lower-cased.
    pub sha256: String,
}

pub fn read_manifest(path: &Path) -> io::Result<Vec<ManifestEntry>> {
    Ok(parse_manifest(&String::from_utf8_lossy(&fs::read(path)?)))
}

/// Lines that are blank, comments (`#`), short, or without a numeric size are skipped.
pub fn parse_manifest(text: &str) -> Vec<ManifestEntry> {
    let mut list = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 4 || f[1].is_empty() {
            continue;
        }
        let Ok(size) = f[2].parse::<i64>() else { continue };
        list.push(ManifestEntry {
            kind: f[0].to_uppercase(),
            rel: f[1].to_string(),
            size,
            sha256: f[3].to_lowercase(),
        });
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_gives_its_entries_and_skips_the_rest() {
        let e =
            parse_manifest("# pack|x|1.0.0\nmod|a\\b.dll|12|ABCD\n\nBIN|agent.exe|7|ef\nbroken\nMOD||1|aa\nMOD|c|nan|aa\n  MOD|d|1|AA  \n");
        assert_eq!(e.len(), 3);
        assert_eq!(
            e[0],
            ManifestEntry {
                kind: "MOD".into(),
                rel: "a\\b.dll".into(),
                size: 12,
                sha256: "abcd".into()
            }
        );
        assert_eq!(e[1].kind, "BIN");
        assert_eq!(e[2].rel, "d");
    }
}
