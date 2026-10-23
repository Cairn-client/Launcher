// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Where Steam is installed, and which folders hold its libraries. The registry lookup sits behind [`SetupHost`] so tests can use a fake.

use crate::app_manifest::AppManifest;
use crate::vdf;
use std::path::{Path, PathBuf};

/// What the setup detection needs from the machine.
pub trait SetupHost {
    /// Steam's install folder from the registry (the current user's `SteamPath`, then the machine's `InstallPath`); `None` when Steam is not installed.
    fn steam_root_from_registry(&self) -> Option<PathBuf>;
}

/// The real machine.
pub struct RealSetupHost;

impl SetupHost for RealSetupHost {
    #[cfg(windows)]
    fn steam_root_from_registry(&self) -> Option<PathBuf> {
        use winreg::enums::*;
        use winreg::RegKey;
        let usable = |raw: String| {
            let p = normalize(Some(&raw));
            (!p.is_empty() && Path::new(&p).is_dir()).then(|| PathBuf::from(p))
        };
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam") {
            if let Some(p) = key.get_value::<String, _>("SteamPath").ok().and_then(usable) {
                return Some(p);
            }
        }
        for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
            if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SOFTWARE\Valve\Steam", KEY_READ | view) {
                if let Some(p) = key.get_value::<String, _>("InstallPath").ok().and_then(usable) {
                    return Some(p);
                }
            }
        }
        None
    }

    #[cfg(not(windows))]
    fn steam_root_from_registry(&self) -> Option<PathBuf> {
        None
    }
}

/// A path as the setup files write it: trimmed, on Windows forward slashes made backslashes, no trailing separator (a drive root keeps its one).
pub fn normalize(path: Option<&str>) -> String {
    let Some(path) = path else { return String::new() };
    if path.trim().is_empty() {
        return String::new();
    }
    // Windows paths use backslashes; anywhere else (the tests on a development machine) the path stays as it is
    let sep = if cfg!(windows) { '\\' } else { '/' };
    let mut p = if cfg!(windows) {
        path.trim().replace('/', "\\")
    } else {
        path.trim().to_string()
    };
    while p.chars().count() > 3 && p.ends_with(sep) {
        p.pop();
    }
    p
}

/// The same place, compared without case and without a trailing separator.
pub fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase(); // compared the Windows way on every platform
    norm(a) == norm(b)
}

/// A Steam install: its root and every library folder (the root first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamInstall {
    pub root: Option<PathBuf>,
    pub libraries: Vec<PathBuf>,
}

impl SteamInstall {
    pub fn found(&self) -> bool {
        self.root.is_some()
    }

    /// Steam's root from `root_override` when given (and it exists), else from the registry; then the libraries named in `libraryfolders.vdf`.
    pub fn locate(host: &dyn SetupHost, root_override: Option<&str>) -> Self {
        let root = match root_override {
            Some(r) => {
                let p = normalize(Some(r));
                (!p.is_empty() && Path::new(&p).is_dir()).then(|| PathBuf::from(p))
            }
            None => host.steam_root_from_registry(),
        };
        let Some(root) = root else {
            return Self {
                root: None,
                libraries: Vec::new(),
            };
        };

        let mut libs = vec![root.clone()];
        let vdf_path = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(bytes) = std::fs::read(&vdf_path) {
            for pair in vdf::pairs(&String::from_utf8_lossy(&bytes)) {
                if !pair.key.eq_ignore_ascii_case("path") {
                    continue;
                }
                let lib = normalize(Some(&pair.value));
                if lib.is_empty() || libs.iter().any(|l| same_path(&l.to_string_lossy(), &lib)) {
                    continue;
                }
                if Path::new(&lib).is_dir() {
                    libs.push(PathBuf::from(lib));
                }
            }
        }
        Self {
            root: Some(root),
            libraries: libs,
        }
    }

    /// The manifest of an app in the first library that has one.
    pub fn manifest(&self, app_id: u32) -> Option<AppManifest> {
        self.libraries.iter().find_map(|lib| AppManifest::try_read(lib, app_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fake(Option<PathBuf>);
    impl SetupHost for Fake {
        fn steam_root_from_registry(&self) -> Option<PathBuf> {
            self.0.clone()
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cairn-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn paths_are_normalized() {
        if cfg!(windows) {
            assert_eq!(normalize(Some("  C:/Games/Steam/ ")), "C:\\Games\\Steam");
            assert_eq!(normalize(Some("C:\\")), "C:\\");
        } else {
            assert_eq!(normalize(Some("  /games/steam/ ")), "/games/steam");
            assert_eq!(normalize(Some("/")), "/");
        }
        assert_eq!(normalize(Some("   ")), "");
        assert_eq!(normalize(None), "");
        assert!(same_path("C:\\Games\\", "c:/games"));
        assert!(!same_path("C:\\Games", "C:\\Game"));
    }

    #[test]
    fn no_steam_means_not_found() {
        let s = SteamInstall::locate(&Fake(None), None);
        assert!(!s.found() && s.libraries.is_empty());
        assert!(!SteamInstall::locate(&Fake(None), Some("/no/such/steam")).found());
    }

    #[test]
    fn libraries_come_from_libraryfolders_and_only_existing_ones() {
        let root = temp("steam-root");
        let lib2 = temp("steam-lib2");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        let q = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            root.join("steamapps").join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n \"0\" {{ \"path\" \"{}\" }}\n \"1\" {{ \"path\" \"{}\" }}\n \"2\" {{ \"path\" \"{}\" }}\n}}\n",
                q(&root),
                q(&lib2),
                q(&root.join("gone"))
            ),
        )
        .unwrap();
        let s = SteamInstall::locate(&Fake(Some(root.clone())), None);
        assert_eq!(s.libraries, vec![root.clone(), lib2.clone()]);
        // an app manifest is found in the second library
        fs::create_dir_all(lib2.join("steamapps")).unwrap();
        fs::write(
            lib2.join("steamapps").join("appmanifest_5.acf"),
            "\"AppState\" { \"installdir\" \"Game\" \"StateFlags\" \"4\" }",
        )
        .unwrap();
        let m = s.manifest(5).unwrap();
        assert_eq!(m.install_dir, "Game");
        assert_eq!(m.library_root, lib2);
        assert!(s.manifest(6).is_none());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&lib2);
    }

    #[test]
    fn a_malformed_library_file_degrades_to_the_root() {
        let root = temp("steam-bad");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::write(root.join("steamapps").join("libraryfolders.vdf"), "{{{ \"path\" oops").unwrap();
        let s = SteamInstall::locate(&Fake(Some(root.clone())), None);
        assert_eq!(s.libraries, vec![root.clone()]);
        let _ = fs::remove_dir_all(&root);
    }
}
