// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The pack store: `%LOCALAPPDATA%\Cairn\packs\<id>\` holds a downloaded pack's `manifest.txt`, `pack.json` and `files\mod`, `files\bin`; the version
//! before the last update is kept beside it as `<id>.previous`, so a bad update can be undone.

use super::contract::{PackContract, FILE_NAME as CONTRACT_FILE};
use super::PackResult;
use crate::mod_install::{read_manifest, ManifestEntry};
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST_NAME: &str = "manifest.txt";
pub const FILES_FOLDER: &str = "files";
pub const MOD_FOLDER: &str = "mod";
pub const BIN_FOLDER: &str = "bin";
pub const PREVIOUS_SUFFIX: &str = ".previous";

/// A pack that is in the store.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledPack {
    pub id: String,
    pub version: String,
    pub files: Vec<ManifestEntry>,
    pub contract: Option<PackContract>,
}

/// The store, rooted at a folder (tests use a temporary one).
#[derive(Debug, Clone)]
pub struct PackStore {
    pub root: PathBuf,
}

impl PackStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `%LOCALAPPDATA%\Cairn\packs` (elsewhere, for development, a folder under the user's data directory).
    pub fn default_root() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
            .unwrap_or_else(std::env::temp_dir);
        base.join("Cairn").join("packs")
    }

    pub fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }
    pub fn files_dir(&self, id: &str) -> PathBuf {
        self.dir(id).join(FILES_FOLDER)
    }
    pub fn manifest_path(&self, id: &str) -> PathBuf {
        self.dir(id).join(MANIFEST_NAME)
    }
    pub fn contract_path(&self, id: &str) -> PathBuf {
        self.dir(id).join(CONTRACT_FILE)
    }
    pub fn previous_dir(&self, id: &str) -> PathBuf {
        let mut name = self.dir(id).into_os_string();
        name.push(PREVIOUS_SUFFIX);
        PathBuf::from(name)
    }

    /// Where a manifest entry's file lies: `files\mod\<rel>` for MOD, `files\bin\<rel>` for BIN.
    pub fn entry_path(&self, id: &str, e: &ManifestEntry) -> PathBuf {
        self.files_dir(id).join(kind_folder(&e.kind)).join(rel_path(&e.rel))
    }

    /// The downloaded pack, or `None` when there is none (or its manifest cannot be read).
    pub fn read(&self, id: &str) -> Option<InstalledPack> {
        read_in(&self.dir(id), id)
    }

    pub fn read_previous(&self, id: &str) -> Option<InstalledPack> {
        read_in(&self.previous_dir(id), id)
    }

    pub fn read_contract(&self, id: &str) -> Option<PackContract> {
        read_contract_in(&self.dir(id))
    }

    pub fn is_downloaded(&self, id: &str) -> bool {
        self.read(id).is_some()
    }

    /// How many files of the downloaded pack's manifest are not in the store (0 for a pack that is whole, or not downloaded). An installer that
    /// swept the folder, or an antivirus, leaves a pack with holes.
    pub fn missing_files(&self, id: &str) -> usize {
        self.read(id)
            .map_or(0, |p| p.files.iter().filter(|e| !self.entry_path(id, e).exists()).count())
    }

    /// Removes every pack with holes (the next start downloads it again); returns their ids. A pack whose files are in use stays as it is.
    pub fn remove_broken(&self) -> Vec<String> {
        let mut removed = Vec::new();
        let Ok(entries) = fs::read_dir(&self.root) else { return removed };
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if !entry.path().is_dir() || id.to_lowercase().ends_with(PREVIOUS_SUFFIX) {
                continue;
            }
            if self.missing_files(&id) > 0 && fs::remove_dir_all(entry.path()).is_ok() {
                removed.push(id);
            }
        }
        removed
    }

    /// Removes the pack and the version kept before its last update.
    pub fn remove(&self, id: &str) -> std::io::Result<()> {
        for dir in [self.dir(id), self.previous_dir(id)] {
            if dir.exists() {
                fs::remove_dir_all(dir)?;
            }
        }
        Ok(())
    }

    /// Goes back to the version before the last update.
    pub fn rollback(&self, id: &str) -> PackResult {
        let Some(old) = self.read_previous(id) else {
            return PackResult::fail("there is no earlier version to go back to");
        };
        let dir = self.dir(id);
        let result = (|| {
            if dir.exists() {
                fs::remove_dir_all(&dir)?;
            }
            fs::rename(self.previous_dir(id), &dir)
        })();
        match result {
            Ok(()) => PackResult::ok(format!("{id} {} restored", old.version)),
            Err(e) => PackResult::fail(format!(
                "a program of the mod is still running: close the game, then try again ({e})"
            )),
        }
    }
}

pub fn kind_folder(kind: &str) -> &'static str {
    if kind.eq_ignore_ascii_case("BIN") {
        BIN_FOLDER
    } else {
        MOD_FOLDER
    }
}

/// A manifest path (written with `/` or `\`) as a path of this machine.
pub fn rel_path(rel: &str) -> PathBuf {
    rel.split(['/', '\\']).filter(|p| !p.is_empty()).collect()
}

fn read_in(dir: &Path, id: &str) -> Option<InstalledPack> {
    let path = dir.join(MANIFEST_NAME);
    let text = fs::read(&path).ok()?;
    let text = String::from_utf8_lossy(&text);
    let version = text
        .lines()
        .find_map(|line| {
            let f: Vec<&str> = line.trim().split('|').collect();
            (f.len() >= 3 && f[0] == "# pack").then(|| f[2].to_string())
        })
        .unwrap_or_default();
    let files = read_manifest(&path).ok()?;
    (!files.is_empty() && !version.is_empty()).then(|| InstalledPack {
        id: id.to_string(),
        version,
        files,
        contract: read_contract_in(dir),
    })
}

fn read_contract_in(dir: &Path) -> Option<PackContract> {
    PackContract::parse(&String::from_utf8_lossy(&fs::read(dir.join(CONTRACT_FILE)).ok()?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("cairn-store-{name}-{}-{:?}", std::process::id(), std::thread::current().id()).replace(['(', ')', ' '], ""));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Writes a pack the way the installer leaves it.
    pub fn put(store: &PackStore, dir: &Path, id: &str, version: &str, files: &[(&str, &str, &str)]) {
        fs::create_dir_all(dir).unwrap();
        let mut manifest = format!("# pack|{id}|{version}\n");
        for (kind, rel, text) in files {
            let path = dir.join(FILES_FOLDER).join(kind_folder(kind)).join(rel_path(rel));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            manifest.push_str(&format!("{kind}|{rel}|{}|{}\n", text.len(), "0".repeat(64)));
        }
        fs::write(dir.join(MANIFEST_NAME), manifest).unwrap();
        let _ = store;
    }

    #[test]
    fn a_stored_pack_is_read_with_its_files_and_contract() {
        let root = temp("read");
        let store = PackStore::new(&root);
        put(
            &store,
            &store.dir("noita"),
            "noita",
            "1.0.0",
            &[("MOD", "a/b.lua", "x"), ("BIN", "proxy.exe", "yy")],
        );
        fs::write(
            store.contract_path("noita"),
            r#"{ "schema": 2, "id": "noita", "version": "1.0.0", "kind": "external", "installDir": "mods/x" }"#,
        )
        .unwrap();
        let p = store.read("noita").unwrap();
        assert_eq!((p.version.as_str(), p.files.len()), ("1.0.0", 2));
        assert_eq!(p.contract.unwrap().install_dir, "mods/x");
        assert!(store.is_downloaded("noita"));
        assert_eq!(store.missing_files("noita"), 0);
        assert!(store
            .entry_path("noita", &p.files[1])
            .ends_with(Path::new("files").join("bin").join("proxy.exe")));
        assert!(store.read("other").is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_pack_with_holes_is_found_and_removed() {
        let root = temp("holes");
        let store = PackStore::new(&root);
        put(&store, &store.dir("a"), "a", "1.0.0", &[("MOD", "x.dll", "1")]);
        put(&store, &store.dir("b"), "b", "1.0.0", &[("MOD", "y.dll", "1")]);
        fs::remove_file(store.files_dir("b").join("mod").join("y.dll")).unwrap();
        assert_eq!(store.missing_files("b"), 1);
        assert_eq!(store.remove_broken(), vec!["b"]);
        assert!(store.is_downloaded("a") && !store.dir("b").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_version_before_the_last_update_can_be_restored() {
        let root = temp("rollback");
        let store = PackStore::new(&root);
        put(&store, &store.dir("a"), "a", "2.0.0", &[("MOD", "x.dll", "new")]);
        put(&store, &store.previous_dir("a"), "a", "1.0.0", &[("MOD", "x.dll", "old")]);
        assert_eq!(store.read_previous("a").unwrap().version, "1.0.0");
        let r = store.rollback("a");
        assert_eq!((r.ok, r.message.as_str()), (true, "a 1.0.0 restored"));
        assert_eq!(store.read("a").unwrap().version, "1.0.0");
        assert!(store.read_previous("a").is_none());
        assert!(!store.rollback("a").ok);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_a_pack_takes_the_previous_version_too() {
        let root = temp("remove");
        let store = PackStore::new(&root);
        put(&store, &store.dir("a"), "a", "2.0.0", &[("MOD", "x.dll", "n")]);
        put(&store, &store.previous_dir("a"), "a", "1.0.0", &[("MOD", "x.dll", "o")]);
        store.remove("a").unwrap();
        assert!(!store.dir("a").exists() && !store.previous_dir("a").exists());
        store.remove("never-there").unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_manifest_without_a_header_or_files_is_not_a_pack() {
        let root = temp("bad");
        let store = PackStore::new(&root);
        fs::create_dir_all(store.dir("a")).unwrap();
        fs::write(store.manifest_path("a"), "MOD|x|1|aa\n").unwrap();
        assert!(store.read("a").is_none());
        fs::write(store.manifest_path("a"), "# pack|a|1.0.0\n").unwrap();
        assert!(store.read("a").is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
