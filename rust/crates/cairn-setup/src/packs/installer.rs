// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Downloads a pack and puts it in the store, or refuses it and changes nothing. The size and sha256 must be what the catalog says; the
//! contract must be one this launcher understands; every file in the zip must be listed in `manifest.txt` with the listed size and hash,
//! nothing may be unlisted, nothing may land outside the pack's own folder, and the contract may only name programs the pack carries.
//! Any failure keeps the previous copy untouched.

use super::contract::{PackContract, SUPPORTED_MENU_PROTOCOL};
use super::index::PackInfo;
use super::store::{kind_folder, rel_path, PackStore, BIN_FOLDER, FILES_FOLDER, MANIFEST_NAME, PREVIOUS_SUFFIX};
use super::{PackKinds, PackResult};
use crate::http::Http;
use crate::mod_install::{parse_manifest, ManifestEntry};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// The player stopped it.
pub const CANCELLED: &str = "cancelled";

/// Downloads `pack` from `url` into the store. `progress` gets 0..1 as the bytes arrive; setting `cancel` stops it (the result is then a failure
/// whose message is [`CANCELLED`]).
pub fn download(
    pack: &PackInfo,
    url: &str,
    store: &PackStore,
    http: &dyn Http,
    progress: &mut dyn FnMut(f64),
    cancel: &AtomicBool,
) -> PackResult {
    let work = store.root.join(format!("{}.new", pack.id));
    let zip_path = store.root.join(format!("{}.download", pack.id));
    let result = download_inner(pack, url, store, http, progress, cancel, &work, &zip_path);
    clean_up(&work, &zip_path);
    result
}

#[allow(clippy::too_many_arguments)]
fn download_inner(
    pack: &PackInfo,
    url: &str,
    store: &PackStore,
    http: &dyn Http,
    progress: &mut dyn FnMut(f64),
    cancel: &AtomicBool,
    work: &Path,
    zip_path: &Path,
) -> PackResult {
    if let Err(e) = fs::create_dir_all(&store.root) {
        return PackResult::fail(format!("could not write the pack ({e})"));
    }
    clean_up(work, zip_path);

    let resp = match http.get(url) {
        Ok(r) => r,
        Err(e) => return PackResult::fail(format!("could not reach the download server ({e})")),
    };
    if !resp.is_success() {
        return PackResult::fail(format!("the download server answered {}", resp.status));
    }
    if resp.content_length.unwrap_or(pack.size as u64) != pack.size as u64 {
        return PackResult::fail("the file is not the size the catalog lists");
    }

    let mut src = resp.body;
    let mut dst = match fs::File::create(zip_path) {
        Ok(f) => f,
        Err(e) => return PackResult::fail(format!("could not write the pack ({e})")),
    };
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 81_920];
    let mut done: i64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return PackResult::fail(CANCELLED);
        }
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return PackResult::fail(format!("could not reach the download server ({e})")),
        };
        done += n as i64;
        if done > pack.size {
            return PackResult::fail("the download is larger than the catalog lists");
        }
        hash.update(&buf[..n]);
        if let Err(e) = dst.write_all(&buf[..n]) {
            return PackResult::fail(format!("could not write the pack ({e})"));
        }
        progress(done as f64 / pack.size as f64);
    }
    drop(dst);
    if done != pack.size {
        return PackResult::fail("the download ended early");
    }
    if !hex(&hash.finalize()).eq_ignore_ascii_case(&pack.sha256) {
        return PackResult::fail("the download does not match its checksum");
    }

    let extracted = extract(zip_path, work, pack);
    if !extracted.ok {
        return extracted;
    }

    let final_dir = store.dir(&pack.id);
    // what the programs wrote beside themselves (their settings and logs) survives an update; the new pack's own files win
    carry_over(
        &final_dir.join(FILES_FOLDER).join(BIN_FOLDER),
        &work.join(FILES_FOLDER).join(BIN_FOLDER),
    );
    let keep_old = store.read(&pack.id).is_some_and(|before| before.version != pack.version);
    if !replace_directory(&final_dir, work, keep_old) {
        return PackResult::fail("a program of the mod is still running: close the game, then try again");
    }
    PackResult::ok(format!("{} {} downloaded", pack.id, pack.version))
}

fn extract(zip_path: &Path, work: &Path, pack: &PackInfo) -> PackResult {
    let files = work.join(FILES_FOLDER);
    if let Err(e) = fs::create_dir_all(&files) {
        return PackResult::fail(format!("could not write the pack ({e})"));
    }
    let Ok(file) = fs::File::open(zip_path) else {
        return PackResult::fail("could not write the pack");
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return PackResult::fail("the download is not a valid pack");
    };

    let (mut manifest_text, mut contract_text): (Option<String>, Option<String>) = (None, None);
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else {
            return PackResult::fail("the download is not a valid pack");
        };
        let name = entry.name().replace('\\', "/");
        if name.ends_with('/') {
            continue;
        }
        if name == MANIFEST_NAME || name == super::contract::FILE_NAME {
            let mut text = String::new();
            if entry.read_to_string(&mut text).is_err() {
                return PackResult::fail("the download is not a valid pack");
            }
            if name == MANIFEST_NAME {
                manifest_text = Some(text);
            } else {
                contract_text = Some(text);
            }
            continue;
        }
        let Some(dest) = safe_join(&files, &name) else {
            return PackResult::fail("the pack contains a file outside its folder");
        };
        if let Some(parent) = dest.parent() {
            if fs::create_dir_all(parent).is_err() {
                return PackResult::fail("could not write the pack");
            }
        }
        let written = fs::File::create(&dest).and_then(|mut out| std::io::copy(&mut entry, &mut out));
        if let Err(e) = written {
            return PackResult::fail(format!("could not write the pack ({e})"));
        }
    }
    let Some(manifest_text) = manifest_text else {
        return PackResult::fail("the pack has no manifest");
    };
    let Some(contract_text) = contract_text else {
        return PackResult::fail("the pack has no contract (pack.json)");
    };
    let Some(contract) = PackContract::parse(&contract_text) else {
        return PackResult::fail("the pack's contract is not one this launcher understands");
    };
    if !contract.id.eq_ignore_ascii_case(&pack.id) || contract.version != pack.version || contract.kind != pack.kind {
        return PackResult::fail("the pack's contract does not match its catalog entry");
    }
    if contract.menu_protocol > SUPPORTED_MENU_PROTOCOL {
        return PackResult::fail("the mod's menu needs a newer launcher");
    }

    if fs::write(
        work.join(MANIFEST_NAME),
        format!("# pack|{}|{}\n{manifest_text}", pack.id, pack.version),
    )
    .is_err()
        || fs::write(work.join(super::contract::FILE_NAME), &contract_text).is_err()
    {
        return PackResult::fail("could not write the pack");
    }
    let entries = parse_manifest(&manifest_text);
    if entries.is_empty() {
        return PackResult::fail("the pack lists no files");
    }
    if !entries.iter().any(|e| e.kind == "MOD") {
        return PackResult::fail("the pack lists no mod files");
    }

    let mut listed: HashSet<String> = HashSet::new();
    for e in &entries {
        let rel = format!("{}/{}", kind_folder(&e.kind), e.rel.replace('\\', "/"));
        let Some(path) = safe_join(&files, &rel) else {
            return PackResult::fail("the pack lists a path outside its folder");
        };
        match verify_file(&path, e) {
            Ok(()) => {}
            Err(msg) => return PackResult::fail(msg),
        }
        listed.insert(key(&rel));
    }
    for f in walk(&files) {
        let rel = f.strip_prefix(&files).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        if !listed.contains(&key(&rel)) {
            return PackResult::fail(format!("{rel} is not listed in the pack manifest"));
        }
    }

    // the contract may only name programs the pack carries
    let carries = |program: &str| listed.contains(&key(program));
    if contract.kind == PackKinds::MOD {
        for program in [&contract.agent, &contract.relay, &contract.native] {
            if !carries(program) {
                return PackResult::fail(format!("the contract names {program}, which the pack does not carry"));
            }
        }
    }
    if contract.kind != PackKinds::CONTENT && !contract.companion.is_empty() && !carries(&contract.companion) {
        return PackResult::fail(format!("the contract names {}, which the pack does not carry", contract.companion));
    }
    if !contract.options_program.is_empty() && !carries(&contract.options_program) {
        return PackResult::fail(format!(
            "the contract names {}, which the pack does not carry",
            contract.options_program
        ));
    }
    PackResult::ok("")
}

fn verify_file(path: &Path, e: &ManifestEntry) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|_| format!("{} is missing from the pack", e.rel))?;
    if !meta.is_file() {
        return Err(format!("{} is missing from the pack", e.rel));
    }
    if meta.len() as i64 != e.size {
        return Err(format!("{} is not the listed size", e.rel));
    }
    let mut f = fs::File::open(path).map_err(|_| format!("{} is missing from the pack", e.rel))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 81_920];
    loop {
        let n = f.read(&mut buf).map_err(|_| format!("{} is missing from the pack", e.rel))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    if !hex(&hash.finalize()).eq_ignore_ascii_case(&e.sha256) {
        return Err(format!("{} does not match its checksum", e.rel));
    }
    Ok(())
}

/// `rel` under `base`, or `None` when it is absolute, has a drive, or climbs out with `..`.
fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut out = base.to_path_buf();
    for c in Path::new(&rel.replace('\\', "/")).components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (out != base).then_some(out)
}

/// Files of a pack are compared by the path they are listed under (`bin/x.exe`, `mod/x.dll`), without case, with `/`.
fn key(rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    rel.to_lowercase()
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Puts `work` where `final_dir` is. An existing copy is stopped (its programs), kept beside as `.previous` when `keep_old`, or replaced;
/// when a file is in use the files that can be replaced are, and `false` is returned only when none can.
fn replace_directory(final_dir: &Path, work: &Path, keep_old: bool) -> bool {
    if !final_dir.exists() {
        return fs::rename(work, final_dir).is_ok();
    }
    stop_programs_running_from(final_dir);
    if keep_old {
        // the old version is kept beside the new one, so a bad update can be undone
        let mut previous = final_dir.as_os_str().to_owned();
        previous.push(PREVIOUS_SUFFIX);
        let previous = PathBuf::from(previous);
        let moved = (|| {
            if previous.exists() {
                fs::remove_dir_all(&previous)?;
            }
            fs::rename(final_dir, &previous)?;
            fs::rename(work, final_dir)
        })();
        if moved.is_ok() {
            return true;
        }
        if !final_dir.exists() && previous.exists() {
            let _ = fs::rename(&previous, final_dir);
        }
    }
    for _ in 0..3 {
        let swapped = (|| {
            if final_dir.exists() {
                fs::remove_dir_all(final_dir)?;
            }
            fs::rename(work, final_dir)
        })();
        if swapped.is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    // a file is held open: delete what can be, then copy the new files over
    for f in walk(final_dir) {
        let _ = fs::remove_file(f); // in use: stays unless replaced below
    }
    for f in walk(work) {
        let dest = final_dir.join(f.strip_prefix(work).unwrap_or(&f));
        if let Some(parent) = dest.parent() {
            if fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        if fs::copy(&f, &dest).is_err() {
            return false;
        }
    }
    true
}

/// Ends any program running from inside the folder (the mod's agent or relay from a session before): a running program holds its file open.
fn stop_programs_running_from(dir: &Path) {
    use sysinfo::{ProcessesToUpdate, System};
    let Ok(root) = fs::canonicalize(dir) else { return };
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let me = std::process::id();
    for p in sys.processes().values() {
        if p.pid().as_u32() == me {
            continue;
        }
        if let Some(exe) = p.exe() {
            if fs::canonicalize(exe).is_ok_and(|e| e.starts_with(&root)) {
                p.kill();
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// Copies what the old programs wrote beside themselves (not in the new pack) into the new `bin`.
fn carry_over(old_bin: &Path, new_bin: &Path) {
    if !old_bin.is_dir() || !new_bin.is_dir() {
        return;
    }
    for f in walk(old_bin) {
        let dest = new_bin.join(f.strip_prefix(old_bin).unwrap_or(&f));
        if dest.exists() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(&f, &dest); // a log that is in use stays behind; the update goes on
    }
}

fn clean_up(work: &Path, zip_path: &Path) {
    let _ = fs::remove_dir_all(work);
    let _ = fs::remove_file(zip_path);
}

/// The path of a file of the pack as the contract names it (`bin/Agent.exe`), inside the pack's `files` folder.
pub fn resolve(files_dir: &Path, relative: &str) -> PathBuf {
    files_dir.join(rel_path(relative))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::packs::store::tests::temp;
    use std::collections::BTreeMap;
    use std::io::Cursor;

    const CONTRACT: &str = r#"{ "schema": 2, "id": "kcd2", "version": "0.1.0", "upstream": "1", "modFolder": "kdcmp", "luaPrefix": "KCD2MP_",
        "agent": "bin/Agent.exe", "relay": "bin/Relay.exe", "native": "bin/Native.dll", "brandPak": "bin/brand.pak", "replacesFolders": ["cairn"],
        "lua": { "eventPrefix": "[TEST-EVT] v1 ", "startGameModel": "Test_Model" } }"#;
    const PROGRAMS: [&str; 4] = ["Agent.exe", "Relay.exe", "Native.dll", "brand.pak"];

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    type LineFn = Box<dyn Fn(&str, &str) -> String>;

    struct Build<'a> {
        files: BTreeMap<&'a str, &'a str>,
        manifest_line: Option<LineFn>,
        with_manifest: bool,
        extra: Vec<(&'a str, &'a str)>,
        contract: Option<String>,
        with_programs: bool,
        version: &'a str,
    }

    impl<'a> Build<'a> {
        fn new(files: &[(&'a str, &'a str)]) -> Self {
            Self {
                files: files.iter().copied().collect(),
                manifest_line: None,
                with_manifest: true,
                extra: vec![],
                contract: None,
                with_programs: true,
                version: "0.2.0",
            }
        }
        fn zip(&self) -> Vec<u8> {
            let mut buf = Cursor::new(Vec::new());
            {
                let mut zip = zip::ZipWriter::new(&mut buf);
                let opts = zip::write::SimpleFileOptions::default();
                let mut add = |name: &str, text: &str| {
                    zip.start_file(name, opts).unwrap();
                    zip.write_all(text.as_bytes()).unwrap();
                };
                let line = |kind: &str, rel: &str, text: &str| match &self.manifest_line {
                    Some(f) => f(rel, text),
                    None => format!("{kind}|{rel}|{}|{}", text.len(), sha(text.as_bytes())),
                };
                let mut lines: Vec<String> = self.files.iter().map(|(k, v)| line("MOD", k, v)).collect();
                if self.with_programs {
                    lines.extend(PROGRAMS.iter().map(|p| line("BIN", p, &format!("stub {p}"))));
                }
                if self.with_manifest {
                    add("manifest.txt", &(lines.join("\n") + "\n"));
                }
                if self.contract.is_some() || self.with_programs {
                    add(
                        "pack.json",
                        &self
                            .contract
                            .clone()
                            .unwrap_or_else(|| CONTRACT.replace("\"version\": \"0.1.0\"", &format!("\"version\": \"{}\"", self.version))),
                    );
                }
                for (k, v) in &self.files {
                    add(&format!("mod/{}", k.replace('\\', "/")), v);
                }
                if self.with_programs {
                    for p in PROGRAMS {
                        add(&format!("bin/{p}"), &format!("stub {p}"));
                    }
                }
                for (k, v) in &self.extra {
                    add(k, v);
                }
                zip.finish().unwrap();
            }
            buf.into_inner()
        }
    }

    fn info_for(zip: &[u8], version: &str) -> PackInfo {
        PackInfo {
            id: "kcd2".into(),
            game: "kcd2".into(),
            version: version.into(),
            file: format!("Cairn-Pack-kcd2-{version}.zip"),
            sha256: sha(zip),
            size: zip.len() as i64,
            launcher_min: String::new(),
            upstream: String::new(),
            depends_on: vec![],
            kind: "mod".into(),
        }
    }

    const URL: &str = "https://example.test/p.zip";
    const FILES: [(&str, &str); 2] = [("mod.manifest", "manifest text"), ("Data\\kdcmp.pak", "pak bytes")];

    fn run(store: &PackStore, info: &PackInfo, body: &[u8]) -> PackResult {
        download(
            info,
            URL,
            store,
            &FakeHttp::serving(URL, body),
            &mut |_| {},
            &AtomicBool::new(false),
        )
    }

    #[test]
    fn a_good_pack_is_downloaded_verified_and_stored() {
        let root = temp("good");
        let store = PackStore::new(&root);
        let zip = Build::new(&FILES).zip();
        let mut progress = Vec::new();
        let r = download(
            &info_for(&zip, "0.2.0"),
            URL,
            &store,
            &FakeHttp::serving(URL, &zip),
            &mut |p| progress.push(p),
            &AtomicBool::new(false),
        );
        assert!(r.ok, "{}", r.message);
        assert_eq!(progress.last().copied(), Some(1.0));
        let pack = store.read("kcd2").unwrap();
        assert_eq!((pack.version.as_str(), pack.files.len()), ("0.2.0", 2 + PROGRAMS.len()));
        assert_eq!(
            fs::read_to_string(store.files_dir("kcd2").join("mod").join("Data").join("kdcmp.pak")).unwrap(),
            "pak bytes"
        );
        let contract = pack.contract.unwrap();
        assert_eq!(
            fs::read_to_string(resolve(&store.files_dir("kcd2"), &contract.agent)).unwrap(),
            "stub Agent.exe"
        );
        assert_eq!(contract.mod_folder, "kdcmp");
        assert!(!root.join("kcd2.download").exists() && !root.join("kcd2.new").exists()); // nothing half-done is left behind
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn downloading_again_replaces_the_previous_copy() {
        let root = temp("again");
        let store = PackStore::new(&root);
        let first = Build::new(&FILES).zip();
        assert!(run(&store, &info_for(&first, "0.2.0"), &first).ok);
        let mut b = Build::new(&[("mod.manifest", "new manifest")]);
        b.version = "0.3.0";
        let second = b.zip();
        let r = run(&store, &info_for(&second, "0.3.0"), &second);
        assert!(r.ok, "{}", r.message);
        let pack = store.read("kcd2").unwrap();
        assert_eq!(pack.version, "0.3.0");
        assert_eq!(pack.files.iter().filter(|f| f.kind == "MOD").count(), 1);
        assert!(!store.files_dir("kcd2").join("mod").join("Data").join("kdcmp.pak").exists()); // the old file is gone
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_update_keeps_the_earlier_version_and_it_can_be_restored() {
        let root = temp("rollback");
        let store = PackStore::new(&root);
        let first = Build::new(&FILES).zip();
        assert!(run(&store, &info_for(&first, "0.2.0"), &first).ok);
        assert!(store.read_previous("kcd2").is_none());
        let mut b = Build::new(&[("mod.manifest", "new manifest")]);
        b.version = "0.3.0";
        let second = b.zip();
        assert!(run(&store, &info_for(&second, "0.3.0"), &second).ok);
        assert_eq!(store.read_previous("kcd2").unwrap().version, "0.2.0");
        assert!(store.rollback("kcd2").ok);
        assert_eq!(store.read("kcd2").unwrap().version, "0.2.0");
        assert!(store.read_previous("kcd2").is_none()); // nothing further back to go to
        assert!(!store.rollback("kcd2").ok);
        store.remove("kcd2").unwrap();
        assert!(!store.previous_dir("kcd2").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_update_keeps_what_the_programs_wrote_beside_them_such_as_their_settings_and_logs() {
        let root = temp("carry");
        let store = PackStore::new(&root);
        let first = Build::new(&FILES).zip();
        assert!(run(&store, &info_for(&first, "0.2.0"), &first).ok);
        let bin = store.files_dir("kcd2").join(BIN_FOLDER);
        fs::write(bin.join("agent.json"), "the player's settings").unwrap();
        fs::write(bin.join("agent.log"), "a log").unwrap();
        fs::create_dir_all(bin.join("sub")).unwrap();
        fs::write(bin.join("sub").join("more.log"), "another").unwrap();
        fs::write(bin.join("Relay.exe"), "tampered, but listed: the new copy wins").unwrap();
        let mut b = Build::new(&FILES);
        b.version = "0.3.0";
        let second = b.zip();
        let r = run(&store, &info_for(&second, "0.3.0"), &second);
        assert!(r.ok, "{}", r.message);
        let nbin = store.files_dir("kcd2").join(BIN_FOLDER);
        assert_eq!(fs::read_to_string(nbin.join("agent.json")).unwrap(), "the player's settings");
        assert_eq!(fs::read_to_string(nbin.join("agent.log")).unwrap(), "a log");
        assert_eq!(fs::read_to_string(nbin.join("sub").join("more.log")).unwrap(), "another");
        assert_eq!(fs::read_to_string(nbin.join("Relay.exe")).unwrap(), "stub Relay.exe");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wrong_checksum_is_refused_and_stores_nothing() {
        let root = temp("sha");
        let store = PackStore::new(&root);
        let zip = Build::new(&FILES).zip();
        let mut info = info_for(&zip, "0.2.0");
        info.sha256 = "0".repeat(64);
        let r = run(&store, &info, &zip);
        assert!(!r.ok && r.message.contains("checksum"));
        assert!(!store.is_downloaded("kcd2"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_replacement_keeps_the_previous_copy() {
        let root = temp("keep");
        let store = PackStore::new(&root);
        let good = Build::new(&FILES).zip();
        assert!(run(&store, &info_for(&good, "0.2.0"), &good).ok);
        let mut info = info_for(&good, "0.2.0");
        info.sha256 = "1".repeat(64);
        assert!(!run(&store, &info, &good).ok);
        assert!(store.is_downloaded("kcd2"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_size_that_differs_from_the_catalog_is_refused() {
        let root = temp("size");
        let store = PackStore::new(&root);
        let zip = Build::new(&FILES).zip();
        let mut info = info_for(&zip, "0.2.0");
        info.size += 5;
        assert!(!run(&store, &info, &zip).ok);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_server_error_or_no_network_is_reported_plainly() {
        let root = temp("net");
        let store = PackStore::new(&root);
        let zip = Build::new(&FILES).zip();
        let info = info_for(&zip, "0.2.0");
        let missing = FakeHttp::default();
        let r = download(&info, URL, &store, &missing, &mut |_| {}, &AtomicBool::new(false));
        assert!(!r.ok && r.message.contains("404"), "{}", r.message);
        let down = FakeHttp {
            unreachable: true,
            ..Default::default()
        };
        let r = download(&info, URL, &store, &down, &mut |_| {}, &AtomicBool::new(false));
        assert!(!r.ok && r.message.contains("could not reach"), "{}", r.message);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_that_escapes_the_pack_folder_is_refused() {
        let root = temp("escape");
        let store = PackStore::new(&root);
        let mut b = Build::new(&FILES);
        b.extra = vec![("../../evil.txt", "x")];
        let zip = b.zip();
        let r = run(&store, &info_for(&zip, "0.2.0"), &zip);
        assert!(!r.ok);
        assert!(!root.join("evil.txt").exists() && !root.parent().unwrap().join("evil.txt").exists());
        assert!(!store.is_downloaded("kcd2"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_the_manifest_does_not_list_is_refused() {
        let root = temp("unlisted");
        let store = PackStore::new(&root);
        let mut b = Build::new(&FILES);
        b.extra = vec![("mod/Data/extra.pak", "sneaked in")];
        let zip = b.zip();
        let r = run(&store, &info_for(&zip, "0.2.0"), &zip);
        assert!(!r.ok && r.message.contains("not listed"), "{}", r.message);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_whose_content_differs_from_its_manifest_line_is_refused() {
        let root = temp("content");
        let store = PackStore::new(&root);
        let mut b = Build::new(&FILES);
        b.manifest_line = Some(Box::new(|rel, text| format!("MOD|{rel}|{}|{}", text.len(), "2".repeat(64))));
        let zip = b.zip();
        let r = run(&store, &info_for(&zip, "0.2.0"), &zip);
        assert!(!r.ok && r.message.contains("checksum"), "{}", r.message);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_pack_without_a_manifest_or_a_contract_is_refused() {
        let root = temp("parts");
        let store = PackStore::new(&root);
        let mut b = Build::new(&FILES);
        b.with_manifest = false;
        let zip = b.zip();
        assert!(!run(&store, &info_for(&zip, "0.2.0"), &zip).ok);
        let mut b = Build::new(&FILES);
        b.with_programs = false;
        let zip = b.zip();
        let r = run(&store, &info_for(&zip, "0.2.0"), &zip);
        assert!(!r.ok && r.message.contains("contract"), "{}", r.message);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_contract_that_names_a_program_the_pack_does_not_carry_is_refused() {
        let root = temp("noprog");
        let store = PackStore::new(&root);
        let mut b = Build::new(&FILES);
        b.contract = Some(
            CONTRACT
                .replace("bin/Relay.exe", "bin/Missing.exe")
                .replace("\"version\": \"0.1.0\"", "\"version\": \"0.2.0\""),
        );
        let zip = b.zip();
        let r = run(&store, &info_for(&zip, "0.2.0"), &zip);
        assert!(!r.ok && r.message.contains("Missing.exe"), "{}", r.message);
        assert!(!store.is_downloaded("kcd2"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_contract_that_does_not_match_its_catalog_entry_is_refused() {
        let root = temp("mismatch");
        let store = PackStore::new(&root);
        let mut b = Build::new(&FILES);
        b.version = "0.9.0";
        let zip = b.zip();
        let r = run(&store, &info_for(&zip, "0.2.0"), &zip);
        assert!(!r.ok && r.message.contains("does not match"), "{}", r.message);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn not_a_zip_is_refused() {
        let root = temp("junk");
        let store = PackStore::new(&root);
        let junk = b"this is not a zip file at all";
        assert!(!run(&store, &info_for(junk, "0.2.0"), junk).ok);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancelling_stops_the_download_and_leaves_nothing() {
        let root = temp("cancel");
        let store = PackStore::new(&root);
        let zip = Build::new(&FILES).zip();
        let r = download(
            &info_for(&zip, "0.2.0"),
            URL,
            &store,
            &FakeHttp::serving(URL, &zip),
            &mut |_| {},
            &AtomicBool::new(true),
        );
        assert_eq!((r.ok, r.message.as_str()), (false, CANCELLED));
        assert!(!store.is_downloaded("kcd2") && !root.join("kcd2.download").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_program_resolves_inside_the_pack_folder() {
        assert_eq!(
            resolve(Path::new("store/kcd2/files"), "bin/Agent.exe"),
            Path::new("store/kcd2/files").join("bin").join("Agent.exe")
        );
    }
}
