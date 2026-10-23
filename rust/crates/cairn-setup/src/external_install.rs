// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Places an external mod in the game's own folder (or the player's local application data), in the folder its contract names
//! (`installDir`). Cairn writes a marker file there listing what it put there, so it only ever updates or removes its own files, and never
//! touches a folder of the same name that the player (or the mod's own installer) made.

use crate::mod_install::ManifestEntry;
use crate::packs::contract::{is_game_relative_dir, is_game_relative_program, is_relative_dir, GAME_ROOT, INSTALL_ROOT_LOCAL_APP_DATA};
use crate::packs::store::rel_path;
use crate::packs::{PackContract, PackKinds, PackResult, PackStore};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MARKER_NAME: &str = ".cairn-pack";

/// What the marker says is placed: the pack's id and version and the files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub id: String,
    pub version: String,
    pub files: Vec<String>,
}

/// The pack's contract when it is an external one; `None` for anything else (no pack, or a mod that uses the Modding Tools).
pub fn contract_of(store: &PackStore, pack_id: &str) -> Option<PackContract> {
    store.read_contract(pack_id).filter(|c| c.kind == PackKinds::EXTERNAL)
}

/// The folder the mod goes to: inside the game's folder, or inside the player's local application data folder when the contract says so;
/// `None` when the contract's folder would leave its root. `local_app_data` is for tests; it is `%LOCALAPPDATA%` otherwise.
pub fn target_dir(game_root: &Path, contract: &PackContract, local_app_data: Option<&Path>) -> Option<PathBuf> {
    let app_data = contract.install_root == INSTALL_ROOT_LOCAL_APP_DATA;
    if !contract.install_root.is_empty() && !app_data {
        return None;
    }
    let base: PathBuf = if app_data {
        match local_app_data {
            Some(p) => p.to_path_buf(),
            None => std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?,
        }
    } else {
        game_root.to_path_buf()
    };
    let dir_ok = if app_data {
        contract.install_dir != GAME_ROOT && is_relative_dir(&contract.install_dir, 6)
    } else {
        is_game_relative_dir(&contract.install_dir)
    };
    if !dir_ok {
        return None;
    }
    if contract.install_dir == GAME_ROOT {
        return Some(base); // the game's own folder
    }
    Some(base.join(rel_path(&contract.install_dir)))
}

/// The program of the game's folder the mod starts the game with (its contract's `launch`), when the contract names one and it is there.
pub fn launch_path(game_root: &Path, store: &PackStore, pack_id: &str) -> Option<PathBuf> {
    let c = contract_of(store, pack_id).filter(|c| !c.launch.is_empty() && is_game_relative_program(&c.launch))?;
    let exe = game_root.join(rel_path(&c.launch));
    exe.is_file().then_some(exe)
}

/// The arguments the launch program is started with (the contract's `launchArgs`); none when the contract names none.
pub fn launch_arguments(store: &PackStore, pack_id: &str) -> Vec<String> {
    contract_of(store, pack_id)
        .filter(|c| !c.launch.is_empty())
        .map(|c| c.launch_args)
        .unwrap_or_default()
}

pub fn read_marker(target: &Path) -> Option<Marker> {
    let text = fs::read(target.join(MARKER_NAME)).ok()?;
    let text = String::from_utf8_lossy(&text);
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next()?.split('|').collect();
    if head.len() < 3 || head[0] != "# pack" {
        return None;
    }
    Some(Marker {
        id: head[1].to_string(),
        version: head[2].to_string(),
        files: lines.filter(|l| !l.is_empty()).map(str::to_string).collect(),
    })
}

/// Whether the downloaded version is the one in the game's folder, with all its files.
pub fn is_placed(game_root: &Path, store: &PackStore, pack_id: &str, local_app_data: Option<&Path>) -> bool {
    let (Some(c), Some(pack)) = (contract_of(store, pack_id), store.read(pack_id)) else {
        return false;
    };
    let Some(target) = target_dir(game_root, &c, local_app_data) else {
        return false;
    };
    let Some(m) = read_marker(&target) else { return false };
    if m.id != pack_id || m.version != pack.version {
        return false;
    }
    pack.files
        .iter()
        .filter(|f| f.kind == "MOD")
        .all(|f| fs::metadata(target.join(rel_path(&f.rel))).is_ok_and(|md| md.is_file() && md.len() as i64 == f.size))
}

/// Puts the downloaded mod's files in the game's folder, verifying them before and after.
pub fn place(game_root: &Path, store: &PackStore, pack_id: &str, local_app_data: Option<&Path>) -> PackResult {
    let Some(c) = contract_of(store, pack_id) else {
        return PackResult::fail("this mod does not install into the game's folder");
    };
    let Some(pack) = store.read(pack_id) else {
        return PackResult::fail("the mod has not been downloaded yet");
    };
    if !game_root.is_dir() {
        return PackResult::fail("the game's folder was not found");
    }
    let Some(target) = target_dir(game_root, &c, local_app_data) else {
        return PackResult::fail("the mod names a folder outside the places it may use");
    };
    let mods: Vec<&ManifestEntry> = pack.files.iter().filter(|e| e.kind == "MOD").collect();
    if mods.is_empty() {
        return PackResult::fail("the mod has not been downloaded yet");
    }
    match place_files(&c, &pack.version, store, pack_id, &target, &mods) {
        Ok(r) => r,
        Err(e) => PackResult::fail(if e.kind() == std::io::ErrorKind::PermissionDenied {
            "no permission to write the game's folder"
        } else {
            "a file is in use (is the game running?)"
        }),
    }
}

fn place_files(
    c: &PackContract,
    version: &str,
    store: &PackStore,
    pack_id: &str,
    target: &Path,
    mods: &[&ManifestEntry],
) -> std::io::Result<PackResult> {
    for e in mods {
        if let Err(why) = matches(&store.entry_path(pack_id, e), e) {
            return Ok(PackResult::fail(format!("the downloaded copy of {} is {why}", e.rel)));
        }
    }
    let marker = read_marker(target);
    let at_root = c.install_dir == GAME_ROOT;
    if let Some(old) = marker.as_ref().filter(|m| m.id != pack_id) {
        let _ = old;
        return Ok(PackResult::fail(if at_root {
            "the game's folder already holds another mod's files".to_string()
        } else {
            format!("{} belongs to another mod", c.install_dir)
        }));
    }
    if !at_root && target.is_dir() && marker.is_none() && fs::read_dir(target)?.next().is_some() {
        return Ok(PackResult::fail(format!(
            "a folder named {} is already in the game and Cairn did not put it there; remove it first",
            c.install_dir
        )));
    }
    if at_root {
        // in the game's own folder there is no folder to call foreign: a file of the game, or of the player, with the same name and other bytes is theirs and stays
        let ours: HashSet<String> = marker.iter().flat_map(|m| m.files.iter()).map(|f| norm_key(f)).collect();
        for e in mods {
            let existing = target.join(rel_path(&e.rel));
            if existing.is_file() && !ours.contains(&norm_key(&e.rel)) && matches(&existing, e).is_err() {
                return Ok(PackResult::fail(format!(
                    "{} is already in the game and is not Cairn's; remove it first",
                    e.rel
                )));
            }
        }
    }

    fs::create_dir_all(target)?;
    // the files an earlier version placed and this one no longer has
    let keep: HashSet<String> = mods.iter().map(|m| norm_key(&m.rel)).collect();
    if let Some(prev) = &marker {
        for rel in prev.files.iter().filter(|f| !keep.contains(&norm_key(f))) {
            delete_inside(target, rel);
        }
    }
    for e in mods {
        let dest = target.join(rel_path(&e.rel));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut tmp = dest.clone().into_os_string();
        tmp.push(".cairn-new");
        let tmp = PathBuf::from(tmp);
        fs::copy(store.entry_path(pack_id, e), &tmp)?;
        if let Ok(md) = fs::metadata(&dest) {
            let mut perms = md.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = fs::set_permissions(&dest, perms);
        }
        fs::rename(&tmp, &dest)?;
    }
    for e in mods {
        if let Err(why) = matches(&target.join(rel_path(&e.rel)), e) {
            return Ok(PackResult::fail(format!("{} is {why} after copying", e.rel)));
        }
    }
    // the marker is written last: a folder with a marker is a complete one
    let listing: Vec<String> = mods.iter().map(|m| slash(&m.rel)).collect();
    fs::write(
        target.join(MARKER_NAME),
        format!("# pack|{pack_id}|{version}\n{}\n", listing.join("\n")),
    )?;
    Ok(PackResult::ok(format!("placed {} file(s)", mods.len())))
}

/// Takes the placed files out again, and the folders that emptied; a file the mod wrote itself, or a folder that is not Cairn's, stays.
pub fn unplace(game_root: &Path, store: &PackStore, pack_id: &str, local_app_data: Option<&Path>) -> PackResult {
    let Some(c) = contract_of(store, pack_id) else {
        return PackResult::ok("nothing to remove");
    };
    let Some(target) = target_dir(game_root, &c, local_app_data) else {
        return PackResult::ok("nothing to remove");
    };
    if !target.is_dir() {
        return PackResult::ok("nothing to remove");
    }
    let Some(m) = read_marker(&target).filter(|m| m.id == pack_id) else {
        return PackResult::ok("the folder is not Cairn's, so it stays");
    };
    for rel in &m.files {
        delete_inside(&target, rel);
    }
    if let Err(e) = fs::remove_file(target.join(MARKER_NAME)) {
        return PackResult::fail(unplace_failure(&e));
    }
    if c.install_dir == GAME_ROOT {
        for rel in &m.files {
            prune_empty_parents(&target, rel); // the game's own folder is never removed, nor a folder that holds anything else
        }
    } else {
        remove_empty_dirs(&target);
    }
    PackResult::ok(format!("removed {} file(s)", m.files.len()))
}

fn unplace_failure(e: &std::io::Error) -> &'static str {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "no permission to change the game's folder"
    } else {
        "a file is in use (is the game running?)"
    }
}

/// The pack's own program for options, saves and setup, when the contract names one and it is in the store.
pub fn options_program_path(store: &PackStore, pack_id: &str) -> Option<PathBuf> {
    let c = store
        .read_contract(pack_id)
        .filter(|c| !c.options_program.is_empty() && c.kind != PackKinds::CONTENT)?;
    let path = crate::packs::installer::resolve(&store.files_dir(pack_id), &c.options_program);
    path.is_file().then_some(path)
}

/// The pack's companion program (started with the game), when the contract names one and it is in the store.
pub fn companion_path(store: &PackStore, pack_id: &str) -> Option<PathBuf> {
    let c = store
        .read_contract(pack_id)
        .filter(|c| !c.companion.is_empty() && c.kind != PackKinds::CONTENT)?;
    let path = crate::packs::installer::resolve(&store.files_dir(pack_id), &c.companion);
    path.is_file().then_some(path)
}

fn slash(rel: &str) -> String {
    rel.replace('\\', "/")
}

fn norm_key(rel: &str) -> String {
    slash(rel).to_lowercase()
}

fn delete_inside(target: &Path, rel: &str) {
    let path = target.join(rel_path(rel));
    if !path.starts_with(target) || !path.is_file() {
        return;
    }
    if let Ok(md) = fs::metadata(&path) {
        let mut perms = md.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        let _ = fs::set_permissions(&path, perms);
    }
    let _ = fs::remove_file(path);
}

/// From the folder a removed file was in, up to (not including) the mod's folder: each one that is now empty goes.
fn prune_empty_parents(target: &Path, rel: &str) {
    let mut dir = target.join(rel_path(rel)).parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        if d == target || !d.starts_with(target) || !d.is_dir() || fs::read_dir(&d).is_ok_and(|mut r| r.next().is_some()) {
            break;
        }
        if fs::remove_dir(&d).is_err() {
            break;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
}

fn remove_empty_dirs(dir: &Path) -> bool {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                remove_empty_dirs(&e.path());
            }
        }
    }
    if fs::read_dir(dir).is_ok_and(|mut r| r.next().is_some()) {
        return false;
    }
    fs::remove_dir(dir).is_ok()
}

fn matches(path: &Path, e: &ManifestEntry) -> Result<(), &'static str> {
    let md = fs::metadata(path).ok().filter(|m| m.is_file()).ok_or("missing")?;
    if md.len() as i64 != e.size {
        return Err("not the listed size");
    }
    let mut f = fs::File::open(path).map_err(|_| "missing")?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 81_920];
    loop {
        let n = f.read(&mut buf).map_err(|_| "missing")?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    let hex: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if hex.eq_ignore_ascii_case(&e.sha256) {
        Ok(())
    } else {
        Err("not the listed bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packs::store::tests::temp;

    struct Fixture {
        root: PathBuf,
        game: PathBuf,
        store: PackStore,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root = temp(name);
            let game = root.join("game");
            fs::create_dir_all(&game).unwrap();
            Self {
                store: PackStore::new(root.join("packs")),
                game,
                root,
            }
        }

        /// A downloaded external pack, as the installer leaves it.
        fn download(&self, version: &str, install_dir: &str, extra_contract: &str, mods: &[(&str, &str)], bins: &[(&str, &str)]) {
            let dir = self.store.dir("noita");
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            let mut manifest = format!("# pack|noita|{version}\n");
            for (kind, files) in [("MOD", mods), ("BIN", bins)] {
                for (rel, text) in files {
                    let path = self
                        .store
                        .files_dir("noita")
                        .join(if kind == "MOD" { "mod" } else { "bin" })
                        .join(rel_path(rel));
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(&path, text).unwrap();
                    let sha: String = Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
                    manifest.push_str(&format!("{kind}|{rel}|{}|{sha}\n", text.len()));
                }
            }
            fs::write(dir.join("manifest.txt"), manifest).unwrap();
            fs::write(
                dir.join("pack.json"),
                format!(r#"{{ "schema": 2, "id": "noita", "version": "{version}", "kind": "external", "installDir": "{install_dir}"{extra_contract} }}"#),
            )
            .unwrap();
        }
    }

    const FILES: [(&str, &str); 2] = [("init.lua", "print('a')"), ("data/x.xml", "<x/>")];

    #[test]
    fn a_downloaded_external_mod_is_placed_in_the_games_folder_and_nowhere_else() {
        let f = Fixture::new("ei1");
        f.download("1.0.0", "mods/quant.ew", "", &FILES, &[]);
        assert!(!is_placed(&f.game, &f.store, "noita", None));
        let r = place(&f.game, &f.store, "noita", None);
        assert!(r.ok, "{}", r.message);
        assert!(is_placed(&f.game, &f.store, "noita", None));
        assert_eq!(
            fs::read_to_string(f.game.join("mods").join("quant.ew").join("init.lua")).unwrap(),
            "print('a')"
        );
        assert_eq!(
            fs::read_to_string(f.game.join("mods").join("quant.ew").join("data").join("x.xml")).unwrap(),
            "<x/>"
        );
        assert_eq!(fs::read_dir(&f.game).unwrap().count(), 1); // only mods/
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn an_update_replaces_the_files_and_drops_the_ones_the_new_version_no_longer_has() {
        let f = Fixture::new("ei2");
        f.download("1.0.0", "mods/quant.ew", "", &FILES, &[]);
        assert!(place(&f.game, &f.store, "noita", None).ok);
        f.download("1.1.0", "mods/quant.ew", "", &[("init.lua", "print('b')")], &[]);
        assert!(!is_placed(&f.game, &f.store, "noita", None)); // the placed copy is the old version
        assert!(place(&f.game, &f.store, "noita", None).ok);
        assert!(is_placed(&f.game, &f.store, "noita", None));
        let dir = f.game.join("mods").join("quant.ew");
        assert_eq!(fs::read_to_string(dir.join("init.lua")).unwrap(), "print('b')");
        assert!(!dir.join("data").join("x.xml").exists());
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn a_folder_the_player_made_is_never_overwritten_or_removed() {
        let f = Fixture::new("ei3");
        let theirs = f.game.join("mods").join("quant.ew");
        fs::create_dir_all(&theirs).unwrap();
        fs::write(theirs.join("init.lua"), "the player's own").unwrap();
        f.download("1.0.0", "mods/quant.ew", "", &FILES, &[]);
        let r = place(&f.game, &f.store, "noita", None);
        assert!(!r.ok && r.message.contains("already in the game"), "{}", r.message);
        assert_eq!(fs::read_to_string(theirs.join("init.lua")).unwrap(), "the player's own");
        assert!(unplace(&f.game, &f.store, "noita", None).ok);
        assert_eq!(fs::read_to_string(theirs.join("init.lua")).unwrap(), "the player's own");
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn removing_takes_out_only_what_cairn_placed_and_the_folders_it_leaves_empty() {
        let f = Fixture::new("ei4");
        f.download("1.0.0", "mods/quant.ew", "", &FILES, &[]);
        place(&f.game, &f.store, "noita", None);
        let dir = f.game.join("mods").join("quant.ew");
        let extra = dir.join("player-settings.txt");
        fs::write(&extra, "kept").unwrap();
        assert!(unplace(&f.game, &f.store, "noita", None).ok);
        assert!(!dir.join("init.lua").exists() && !dir.join(MARKER_NAME).exists());
        assert_eq!(fs::read_to_string(&extra).unwrap(), "kept"); // something the mod wrote itself stays
        fs::remove_file(&extra).unwrap();
        place(&f.game, &f.store, "noita", None);
        assert!(unplace(&f.game, &f.store, "noita", None).ok);
        assert!(!dir.exists());
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn a_pack_that_is_not_an_external_mod_is_not_placed_by_this() {
        let f = Fixture::new("ei5");
        assert!(!place(&f.game, &f.store, "nothing", None).ok);
        assert!(unplace(&f.game, &f.store, "nothing", None).ok);
        assert!(!is_placed(&f.game, &f.store, "nothing", None));
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn a_companion_program_is_found_in_the_pack_store() {
        let f = Fixture::new("ei6");
        f.download(
            "1.0.0",
            "mods/quant.ew",
            r#", "companion": "bin/proxy.exe""#,
            &FILES,
            &[("proxy.exe", "proxy!!")],
        );
        let path = companion_path(&f.store, "noita").unwrap();
        assert!(path.starts_with(f.store.files_dir("noita")));
        assert_eq!(fs::read_to_string(path).unwrap(), "proxy!!");
        f.download("1.0.0", "mods/quant.ew", "", &FILES, &[]);
        assert!(companion_path(&f.store, "noita").is_none());
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn a_mod_for_the_games_own_folder_is_placed_beside_the_games_files() {
        let f = Fixture::new("ei7");
        fs::write(f.game.join("game.exe"), "the game").unwrap();
        f.download(
            "1.0.0",
            ".",
            "",
            &[("winhttp.dll", "loader"), ("BepInEx/plugins/mp/mp.dll", "plugin")],
            &[],
        );
        let r = place(&f.game, &f.store, "noita", None);
        assert!(r.ok, "{}", r.message);
        assert!(is_placed(&f.game, &f.store, "noita", None));
        assert_eq!(fs::read_to_string(f.game.join("winhttp.dll")).unwrap(), "loader");
        assert_eq!(
            fs::read_to_string(f.game.join("BepInEx").join("plugins").join("mp").join("mp.dll")).unwrap(),
            "plugin"
        );
        assert_eq!(fs::read_to_string(f.game.join("game.exe")).unwrap(), "the game");
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn in_the_games_own_folder_a_file_that_is_not_cairns_is_never_overwritten() {
        let f = Fixture::new("ei8");
        fs::write(f.game.join("winhttp.dll"), "somebody else's loader").unwrap();
        f.download("1.0.0", ".", "", &[("winhttp.dll", "loader"), ("mod.dll", "mod")], &[]);
        let r = place(&f.game, &f.store, "noita", None);
        assert!(!r.ok && r.message.contains("winhttp.dll"), "{}", r.message);
        assert_eq!(fs::read_to_string(f.game.join("winhttp.dll")).unwrap(), "somebody else's loader");
        assert!(!f.game.join("mod.dll").exists());
        // a file that is exactly the pack's is adopted
        fs::write(f.game.join("winhttp.dll"), "loader").unwrap();
        assert!(place(&f.game, &f.store, "noita", None).ok);
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn removing_from_the_games_own_folder_takes_only_the_listed_files_and_the_folders_that_emptied() {
        let f = Fixture::new("ei9");
        fs::write(f.game.join("game.exe"), "the game").unwrap();
        f.download(
            "1.0.0",
            ".",
            "",
            &[("winhttp.dll", "loader"), ("BepInEx/plugins/mp/mp.dll", "plugin")],
            &[],
        );
        place(&f.game, &f.store, "noita", None);
        fs::create_dir_all(f.game.join("BepInEx").join("config")).unwrap();
        fs::write(f.game.join("BepInEx").join("config").join("mine.cfg"), "kept").unwrap();
        assert!(unplace(&f.game, &f.store, "noita", None).ok);
        assert!(!f.game.join("winhttp.dll").exists() && !f.game.join("BepInEx").join("plugins").exists());
        assert!(f.game.join("BepInEx").join("config").join("mine.cfg").exists()); // a folder that holds anything else stays
        assert!(f.game.join("game.exe").exists() && f.game.exists());
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn a_mod_for_the_local_application_data_goes_there_and_not_in_the_game() {
        let f = Fixture::new("ei10");
        let appdata = f.root.join("appdata");
        fs::create_dir_all(&appdata).unwrap();
        f.download(
            "1.0.0",
            "Colossal Order/Cities_Skylines/Addons/Mods/CSM",
            r#", "installRoot": "local-appdata""#,
            &[("CSM.dll", "dll")],
            &[],
        );
        let r = place(&f.game, &f.store, "noita", Some(&appdata));
        assert!(r.ok, "{}", r.message);
        assert!(is_placed(&f.game, &f.store, "noita", Some(&appdata)));
        assert!(appdata
            .join("Colossal Order")
            .join("Cities_Skylines")
            .join("Addons")
            .join("Mods")
            .join("CSM")
            .join("CSM.dll")
            .exists());
        assert_eq!(fs::read_dir(&f.game).unwrap().count(), 0);
        assert!(unplace(&f.game, &f.store, "noita", Some(&appdata)).ok);
        assert!(!appdata
            .join("Colossal Order")
            .join("Cities_Skylines")
            .join("Addons")
            .join("Mods")
            .join("CSM")
            .exists());
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn a_tampered_download_is_not_placed() {
        let f = Fixture::new("ei11");
        f.download("1.0.0", "mods/quant.ew", "", &FILES, &[]);
        fs::write(f.store.files_dir("noita").join("mod").join("init.lua"), "evil").unwrap();
        let r = place(&f.game, &f.store, "noita", None);
        assert!(!r.ok && r.message.contains("init.lua"), "{}", r.message);
        assert!(!f.game.join("mods").exists());
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn one_mod_cannot_take_over_the_folder_of_another() {
        let f = Fixture::new("ei12");
        f.download("1.0.0", "mods/quant.ew", "", &FILES, &[]);
        assert!(place(&f.game, &f.store, "noita", None).ok);
        // a different pack that names the same folder
        let other = f.store.dir("other");
        fs::create_dir_all(&other).unwrap();
        let text = "x";
        let sha: String = Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        fs::create_dir_all(f.store.files_dir("other").join("mod")).unwrap();
        fs::write(f.store.files_dir("other").join("mod").join("a.lua"), text).unwrap();
        fs::write(other.join("manifest.txt"), format!("# pack|other|1.0.0\nMOD|a.lua|1|{sha}\n")).unwrap();
        fs::write(
            other.join("pack.json"),
            r#"{ "schema": 2, "id": "other", "version": "1.0.0", "kind": "external", "installDir": "mods/quant.ew" }"#,
        )
        .unwrap();
        let r = place(&f.game, &f.store, "other", None);
        assert!(!r.ok && r.message.contains("belongs to another mod"), "{}", r.message);
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn the_launch_program_is_found_only_once_the_mod_placed_it() {
        let f = Fixture::new("ei13");
        let exe = "coremod/core/host/startup/DeadCellsModding.exe";
        f.download(
            "1.0.0",
            ".",
            &format!(r#", "launch": "{exe}", "launchArgs": ["-MOD:x"]"#),
            &[(exe, "the launcher")],
            &[],
        );
        assert!(launch_path(&f.game, &f.store, "noita").is_none()); // not placed yet
        assert!(place(&f.game, &f.store, "noita", None).ok);
        assert_eq!(
            launch_path(&f.game, &f.store, "noita").unwrap(),
            f.game
                .join("coremod")
                .join("core")
                .join("host")
                .join("startup")
                .join("DeadCellsModding.exe")
        );
        assert_eq!(launch_arguments(&f.store, "noita"), vec!["-MOD:x"]);
        f.download("1.0.0", ".", "", &[("a.dll", "x")], &[]);
        assert!(launch_path(&f.game, &f.store, "noita").is_none() && launch_arguments(&f.store, "noita").is_empty());
        let _ = fs::remove_dir_all(&f.root);
    }

    #[test]
    fn the_options_program_is_found_in_the_store() {
        let f = Fixture::new("ei14");
        f.download(
            "1.0.0",
            "mods/x",
            r#", "optionsProgram": "bin/tool.exe""#,
            &FILES,
            &[("tool.exe", "tool")],
        );
        assert!(options_program_path(&f.store, "noita")
            .unwrap()
            .ends_with(Path::new("bin").join("tool.exe")));
        assert!(options_program_path(&f.store, "other").is_none());
        let _ = fs::remove_dir_all(&f.root);
    }
}
