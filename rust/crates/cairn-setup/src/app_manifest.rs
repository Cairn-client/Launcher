// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Steam's `appmanifest_<id>.acf`: where a Steam app is installed and whether it is being downloaded. The account that owns it
//! (`LastOwner`) is never read.

use crate::vdf;
use std::fs;
use std::path::{Path, PathBuf};

// Steam's EAppState bits, as they appear in "StateFlags".
pub const FLAG_UPDATE_REQUIRED: i64 = 2;
pub const FLAG_FULLY_INSTALLED: i64 = 4;
pub const FLAG_FILES_MISSING: i64 = 32;
pub const FLAG_FILES_CORRUPT: i64 = 128;
pub const FLAG_UPDATE_RUNNING: i64 = 256;
pub const FLAG_UPDATE_PAUSED: i64 = 512;
pub const FLAG_UPDATE_STARTED: i64 = 1024;
pub const FLAG_UNINSTALLING: i64 = 2048;
pub const FLAG_VALIDATING: i64 = 131_072;
pub const FLAG_ADDING_FILES: i64 = 262_144;
pub const FLAG_PREALLOCATING: i64 = 524_288;
pub const FLAG_DOWNLOADING: i64 = 1_048_576;
pub const FLAG_STAGING: i64 = 2_097_152;
pub const FLAG_COMMITTING: i64 = 4_194_304;

const BUSY_MASK: i64 = FLAG_UPDATE_RUNNING
    | FLAG_UPDATE_STARTED
    | FLAG_VALIDATING
    | FLAG_ADDING_FILES
    | FLAG_PREALLOCATING
    | FLAG_DOWNLOADING
    | FLAG_STAGING
    | FLAG_COMMITTING
    | FLAG_UNINSTALLING;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppManifest {
    pub app_id: u32,
    pub library_root: PathBuf,
    pub install_dir: String,
    pub state_flags: i64,
    pub bytes_to_download: i64,
    pub bytes_downloaded: i64,
    pub bytes_to_stage: i64,
    pub bytes_staged: i64,
}

impl AppManifest {
    /// `<library>/steamapps/common/<installdir>`.
    pub fn install_path(&self) -> PathBuf {
        self.library_root.join("steamapps").join("common").join(&self.install_dir)
    }

    pub fn fully_installed(&self) -> bool {
        self.state_flags & FLAG_FULLY_INSTALLED != 0
    }

    /// Steam is downloading, staging, validating or removing it.
    pub fn busy(&self) -> bool {
        self.state_flags & BUSY_MASK != 0
    }

    pub fn paused(&self) -> bool {
        self.state_flags & FLAG_UPDATE_PAUSED != 0
    }

    /// 0..1 from Steam's counters (the download is nine tenths, staging the last tenth); `None` when it reports no work.
    pub fn progress(&self) -> Option<f64> {
        if self.bytes_to_download > 0 && self.bytes_downloaded < self.bytes_to_download {
            return Some((self.bytes_downloaded as f64 / self.bytes_to_download as f64).clamp(0.0, 1.0) * 0.9);
        }
        if self.bytes_to_stage > 0 {
            return Some(0.9 + (self.bytes_staged as f64 / self.bytes_to_stage as f64).clamp(0.0, 1.0) * 0.1);
        }
        if self.bytes_to_download > 0 {
            return Some(0.9);
        }
        None
    }

    pub fn path_for(library_root: &Path, app_id: u32) -> PathBuf {
        library_root.join("steamapps").join(format!("appmanifest_{app_id}.acf"))
    }

    /// The manifest of the app in that library; `None` when there is none or it cannot be read.
    pub fn try_read(library_root: &Path, app_id: u32) -> Option<Self> {
        let text = fs::read(Self::path_for(library_root, app_id)).ok()?;
        Self::parse(library_root, app_id, &String::from_utf8_lossy(&text))
    }

    /// `None` when the text has no `installdir`.
    pub fn parse(library_root: &Path, app_id: u32, text: &str) -> Option<Self> {
        let pairs = vdf::pairs(text);
        let install_dir = vdf::value(&pairs, "installdir")?.to_string();
        if install_dir.trim().is_empty() {
            return None;
        }
        let num = |key: &str| vdf::value(&pairs, key).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        Some(Self {
            app_id,
            library_root: library_root.to_path_buf(),
            install_dir,
            state_flags: num("StateFlags"),
            bytes_to_download: num("BytesToDownload"),
            bytes_downloaded: num("BytesDownloaded"),
            bytes_to_stage: num("BytesToStage"),
            bytes_staged: num("BytesStaged"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The shape of a real appmanifest, with the account id replaced.
    const REAL: &str = "\"AppState\"\n{\n\t\"appid\"\t\t\"2429020\"\n\t\"universe\"\t\t\"1\"\n\t\"name\"\t\t\"Kingdom Come: Deliverance II Modding tools\"\n\
        \t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"KCD2Mod\"\n\t\"SizeOnDisk\"\t\t\"16570292908\"\n\t\"LastOwner\"\t\t\"76561190000000001\"\n\
        \t\"BytesToDownload\"\t\t\"8850914320\"\n\t\"BytesDownloaded\"\t\t\"8850914320\"\n\t\"BytesToStage\"\t\t\"16570292908\"\n\t\"BytesStaged\"\t\t\"16570292908\"\n\
        \t\"InstalledDepots\"\n\t{\n\t\t\"2429021\"\n\t\t{\n\t\t\t\"manifest\"\t\t\"2246809240466786225\"\n\t\t\t\"size\"\t\t\"16570292908\"\n\t\t}\n\t}\n}\n";

    #[test]
    fn an_installed_manifest_reads_as_installed_and_idle() {
        let m = AppManifest::parse(Path::new("lib"), 2_429_020, REAL).unwrap();
        assert_eq!(m.install_dir, "KCD2Mod");
        assert_eq!(m.install_path(), Path::new("lib").join("steamapps").join("common").join("KCD2Mod"));
        assert!(m.fully_installed());
        assert!(!m.busy());
    }

    #[test]
    fn a_download_in_progress_shows_progress_from_the_counters() {
        let text = REAL
            .replace("\"StateFlags\"\t\t\"4\"", "\"StateFlags\"\t\t\"1026\"")
            .replace("\"BytesDownloaded\"\t\t\"8850914320\"", "\"BytesDownloaded\"\t\t\"4425457160\"")
            .replace("\"BytesStaged\"\t\t\"16570292908\"", "\"BytesStaged\"\t\t\"0\"");
        let m = AppManifest::parse(Path::new("L"), 2_429_020, &text).unwrap();
        assert!(m.busy());
        assert!(!m.fully_installed());
        assert!((m.progress().unwrap() - 0.45).abs() < 0.005);
    }

    #[test]
    fn a_manifest_without_an_install_dir_is_not_a_manifest() {
        assert!(AppManifest::parse(Path::new("L"), 1, "\"AppState\" { \"appid\" \"1\" }").is_none());
    }

    #[test]
    fn the_account_id_is_never_read() {
        let m = AppManifest::parse(Path::new("L"), 2_429_020, REAL).unwrap();
        assert!(!format!("{m:?}").contains("76561190000000001"));
    }

    #[test]
    fn a_missing_file_is_none() {
        assert!(AppManifest::try_read(Path::new("/no/such/library"), 1).is_none());
    }
}
