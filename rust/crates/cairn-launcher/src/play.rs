// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Playing: the ready check and the setup checklist, starting a game (and its mod's program), and noticing when it closes.

use crate::app::{CairnApp, Msg};
use crate::games::GameProfile;
use crate::preflight::{self, Fix, PreflightInput};
use crate::runtime;
use crate::setup_list::{self, LineAction};
use cairn_setup::bridge_protocol::PIPE_ENVIRONMENT_VARIABLE;
use cairn_setup::bridge_server::BridgeServer;
use cairn_setup::catalog::GameManifest;
use cairn_setup::external_install;
use cairn_setup::game_setup::{self, GameSetupHost, SetupRule};
use cairn_setup::pack_setup;
use cairn_setup::packs::PackKinds;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// What the catalog's checks ask of this PC, found out before the checks run.
struct Facts {
    steam_running: bool,
    game_found: bool,
    mod_installed: bool,
    apps: HashSet<u32>,
    folder: Option<PathBuf>,
    free_mb: Option<i64>,
}

impl GameSetupHost for Facts {
    fn steam_running(&self) -> bool {
        self.steam_running
    }
    fn game_found(&self) -> bool {
        self.game_found
    }
    fn mod_installed(&self) -> bool {
        self.mod_installed
    }
    fn steam_app_installed(&self, app_id: u32) -> bool {
        self.apps.contains(&app_id)
    }
    fn game_file_exists(&self, relative_path: &str) -> bool {
        self.folder
            .as_ref()
            .is_some_and(|f| f.join(cairn_setup::packs::store::rel_path(relative_path)).exists())
    }
    fn free_disk_mb(&self) -> Option<i64> {
        self.free_mb
    }
}

/// Free space in MB on the drive that holds `path`.
pub fn free_disk_mb(path: &Path) -> Option<i64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let full = std::fs::canonicalize(path).ok()?;
    disks
        .list()
        .iter()
        .filter(|d| full.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| (d.available_space() / (1024 * 1024)) as i64)
}

impl CairnApp {
    // ---- the setup checklist ---------------------------------------------------------------------------------------------

    /// The pack program of a game when its pack runs the game's setup (`setup` in its contract).
    fn setup_program(&self, game_id: &str) -> Option<PathBuf> {
        self.store.read_contract(game_id).filter(|c| c.setup)?;
        external_install::options_program_path(&self.store, game_id)
    }

    /// Looks at this PC for the game's checklist on a worker thread: the pack's own program when it has one, else the catalog's checks.
    pub fn refresh_setup(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        if self.setup_checking && game.id.eq_ignore_ascii_case(&self.setup_game) {
            return;
        }
        if !game.id.eq_ignore_ascii_case(&self.setup_game) {
            self.setup_lines.clear();
            self.setup_notes.clear();
        }
        self.setup_game = game.id.clone();
        self.setup_checking = true;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        if let Some(program) = self.setup_program(&game.id) {
            let exe = self
                .finder
                .exe_of(&game, &self.settings.game_path)
                .map(|p| p.to_string_lossy().into_owned());
            std::thread::spawn(move || {
                let cancel = AtomicBool::new(false);
                // the first run of a program right after an update is often slow (the new files are being scanned): one more try
                let report = pack_setup::check(&program, exe.as_deref(), None, &cancel).or_else(|| {
                    std::thread::sleep(Duration::from_millis(1500));
                    pack_setup::check(&program, exe.as_deref(), None, &cancel)
                });
                let (lines, game_exe) = match report {
                    Some(r) => (setup_list::from_report(&r), r.game_exe),
                    None => (Vec::new(), None),
                };
                let _ = tx.send(Msg::SetupLines {
                    game: game.id,
                    lines,
                    game_exe,
                });
                ctx.request_repaint();
            });
            return;
        }
        let rules: Vec<SetupRule> = if game.setup.is_empty() {
            game_setup::defaults()
        } else {
            game.setup.clone()
        };
        let found = self.is_game_found(&game);
        let folder = self.game_folder_of(&game);
        let mod_installed = self.is_mod_installed(&game);
        let mut apps = HashSet::new();
        for r in &rules {
            if r.kind == "steam-app" && self.finder.steam().manifest(r.app_id).is_some_and(|m| m.fully_installed()) {
                apps.insert(r.app_id);
            }
        }
        std::thread::spawn(move || {
            let facts = Facts {
                steam_running: runtime::steam_running(),
                game_found: found,
                mod_installed,
                apps,
                free_mb: folder.as_deref().and_then(free_disk_mb),
                folder,
            };
            let results = game_setup::evaluate(&rules, &facts);
            let _ = tx.send(Msg::SetupLines {
                game: game.id,
                lines: setup_list::from_results(&results),
                game_exe: None,
            });
            ctx.request_repaint();
        });
    }

    /// While the checklist is open it is read again every two seconds.
    pub fn poll_setup(&mut self) {
        if !self.setup_open || self.setup_checking || self.setup_busy.is_some() || self.setup_last_poll.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.setup_last_poll = Instant::now();
        let game = self.setup_game.clone();
        if !game.is_empty() {
            self.refresh_setup(&game);
        }
        self.ctx.request_repaint_after(Duration::from_secs(2));
    }

    /// Opens "Before you play" for a game.
    pub fn open_setup(&mut self, game_id: &str) {
        self.setup_open = true;
        self.setup_game = game_id.to_string();
        self.setup_checking = false;
        self.refresh_setup(game_id);
    }

    /// A button of the checklist: a pack's action runs on a worker thread; the launcher's own fixes are done here.
    pub fn do_setup_action(&mut self, game_id: &str, step_id: &str, action: LineAction) {
        match action {
            LineAction::Fix(fix) => {
                self.apply_fix(game_id, fix);
                self.refresh_setup(game_id);
            }
            LineAction::Pack(word) => {
                let Some(program) = self.setup_program(game_id) else { return };
                let game = self.registry.get(game_id).cloned();
                let exe = game
                    .as_ref()
                    .and_then(|g| self.finder.exe_of(g, &self.settings.game_path))
                    .map(|p| p.to_string_lossy().into_owned());
                self.setup_busy = Some(step_id.to_string());
                let (tx, ctx, id, step) = (self.tx.clone(), self.ctx.clone(), game_id.to_string(), step_id.to_string());
                std::thread::spawn(move || {
                    let cancel = AtomicBool::new(false);
                    let r = pack_setup::do_action(&program, &word, exe.as_deref(), &mut |_| {}, &mut |_| {}, None, &cancel);
                    let _ = tx.send(Msg::SetupAction {
                        game: id,
                        step,
                        ok: r.ok,
                        note: r.message,
                    });
                    ctx.request_repaint();
                });
            }
        }
    }

    /// A game path that the pack's setup found is taken when the settings have none (a game whose executable is a setting).
    pub fn adopt_detected_game_path(&mut self, game_id: &str, exe: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        if !game.exe_is_setting || !Path::new(exe).is_file() || cairn_setup::setup_host::same_path(&self.settings.game_path, exe) {
            return;
        }
        self.settings.game_path = exe.to_string();
        if self.settings_store.is_unset_on_disk("GamePath") {
            let _ = self.settings_store.save(&self.settings, Some(&["GamePath"]));
        } else {
            // a path the player set that does not work now is replaced for this run only
            self.settings_store.accept(&self.settings, &["GamePath"]);
        }
        self.finder.forget(&game.id);
    }

    // ---- the ready check ---------------------------------------------------------------------------------------------------

    /// What the ready check says about a game right now.
    pub fn preflight_for(&mut self, game: &GameProfile) -> Vec<preflight::PreflightItem> {
        let blocked = self
            .installed_pack_of(&game.id)
            .and_then(|p| self.registry.catalog().block_of(&p.id, &p.version).map(|b| b.reason.clone()));
        let input = PreflightInput {
            supported: game.is_supported,
            game_found: self.is_game_found(game),
            mod_installed: self.is_mod_installed(game),
            setup_ok: self.setup_game.eq_ignore_ascii_case(&game.id)
                && !self.setup_lines.is_empty()
                && setup_list::all_done(&self.setup_lines),
            steam_running: runtime::steam_running(),
            port_free: !self.settings.host_port.is_positive()
                || runtime::port_is_free(self.settings.host_port as u16)
                || self.is_external_mod(game),
            game_running: self.running.as_ref().is_some_and(|r| r.game_id.eq_ignore_ascii_case(&game.id)),
            newer_launcher: self.update.as_ref().map(|u| u.tag.clone()),
            mod_blocked: blocked,
        };
        preflight::build(&input)
    }

    /// Opens the ready check for the selected game.
    pub fn open_ready_check(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        if !self.setup_game.eq_ignore_ascii_case(&game.id) {
            self.refresh_setup(&game.id);
        }
        self.preflight_items = self.preflight_for(&game);
        self.preflight_open = true;
    }

    /// What a button of the ready check does, then the check runs again.
    pub fn apply_fix(&mut self, game_id: &str, fix: Fix) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        match fix {
            Fix::StartSteam => {
                crate::open::open_url("steam://open/main");
                self.toast("Starting Steam...", false);
            }
            Fix::InstallMod | Fix::UpdateMod => {
                self.download_pack(&game.id);
            }
            Fix::PlaceMod => self.install_mod(&game.id),
            Fix::LocateGame => self.locate_game(&game),
            Fix::UpdateLauncher => self.check_for_update(true),
        }
        if self.preflight_open {
            self.preflight_items = self.preflight_for(&game);
        }
    }

    /// Lets the player point Cairn at a game it did not find (not on Steam, or in another library).
    pub fn locate_game(&mut self, game: &GameProfile) {
        let mut dialog = rfd::FileDialog::new().set_title(format!("Locate {}", game.display_name));
        if !game.default_exe_name.is_empty() {
            dialog = dialog.add_filter(format!("{} ({})", game.display_name, game.default_exe_name), &["exe"]);
        }
        let Some(picked) = dialog.pick_file() else { return };
        let Some(folder) = cairn_setup::catalog_limits::is_id(&game.id)
            .then(|| crate::prefs::GamePathStore::resolve_install_folder(picked.to_str(), &game.default_exe_name))
            .flatten()
        else {
            self.set_pack_message(
                &format!("That is not {}. Pick the game's own executable.", game.default_exe_name),
                true,
            );
            return;
        };
        if !self.finder.paths.set(&game.id, folder.to_str()) {
            self.set_pack_message("Could not remember that folder (the Cairn data folder is not writable).", true);
        }
        self.finder.forget(&game.id);
        self.registry.mark_installed(&game.id, folder.to_str());
        self.installed_cache_clear(&game.id);
    }

    pub fn installed_cache_clear(&mut self, game_id: &str) {
        let _ = self.tx.send(Msg::PackChanged(game_id.to_string()));
    }

    // ---- starting a game ---------------------------------------------------------------------------------------------------

    /// The bridge a game's mod connects to (one per launcher run): its name goes to the mod's own program in the environment.
    pub fn ensure_bridge(&mut self) -> Option<String> {
        if self.bridge.is_none() {
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            let (tx2, ctx2) = (tx.clone(), ctx.clone());
            let server = BridgeServer::start(
                move |m| {
                    let _ = tx.send(Msg::Bridge(m));
                    ctx.request_repaint();
                },
                move |g| {
                    let _ = tx2.send(Msg::BridgeConnected(g.to_string()));
                    ctx2.request_repaint();
                },
            );
            match server {
                Ok(s) => self.bridge = Some(s),
                Err(e) => {
                    self.toast(&format!("The mod's connection to the launcher could not start ({e})."), true);
                    return None;
                }
            }
        }
        self.bridge.as_ref().map(|b| b.pipe_name().to_string())
    }

    /// Play: starts the mod's companion program (when it has one and it is not running), then the game; the mod takes it from there.
    pub fn play_external(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        if !self.is_external_mod(&game) {
            return;
        }
        if !self.is_mod_installed(&game) {
            self.set_pack_message("Install the mod before you play.", true);
            return;
        }
        if self.running.is_some() {
            self.set_pack_message(&format!("{} is already running.", game.display_name), false);
            return;
        }
        let Some(folder) = self.game_folder_of(&game) else {
            self.set_pack_message(
                &format!(
                    "{} was not found on this PC. Make sure it is installed, then locate it.",
                    game.display_name
                ),
                true,
            );
            return;
        };
        // a mod with a setup list of its own is checked first: what is missing is shown and done there
        if self.setup_program(&game.id).is_some()
            && !(self.setup_game.eq_ignore_ascii_case(&game.id) && setup_list::all_done(&self.setup_lines) && !self.setup_lines.is_empty())
        {
            self.open_setup(&game.id);
            return;
        }
        let contract = self.store.read_contract(&game.id);
        let companion = external_install::companion_path(&self.store, &game.id);
        let pipe = companion.as_ref().and_then(|_| self.ensure_bridge());
        let launch = external_install::launch_path(&folder, &self.store, &game.id);
        let launch_args = external_install::launch_arguments(&self.store, &game.id);
        let wants_launch = contract.as_ref().is_some_and(|c| !c.launch.is_empty());
        let companion_starts_game = companion.is_some() && contract.as_ref().is_some_and(|c| c.companion_starts_game);
        let exe_stem = Path::new(&game.default_exe_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let g = game.clone();
        self.recent.record(&game.id, "host", cairn_setup::time_util::now_utc(), "");
        std::thread::spawn(move || {
            let say = |text: String, error: bool| {
                let _ = tx.send(Msg::Toast(text, error));
                ctx.request_repaint();
            };
            if let Some(c) = &companion {
                if !runtime::find_process_of(c).is_some() {
                    let mut cmd = Command::new(c);
                    cmd.current_dir(c.parent().unwrap_or(Path::new(".")));
                    if let Some(p) = &pipe {
                        cmd.env(PIPE_ENVIRONMENT_VARIABLE, p);
                    }
                    if let Err(e) = cmd.spawn() {
                        say(format!("The mod's own program did not start ({e})."), true);
                        return;
                    }
                }
            }
            if companion_starts_game {
                return; // its own program hosts or joins, and then starts the game
            }
            let started = if wants_launch {
                match &launch {
                    Some(exe) => Command::new(exe).args(&launch_args).current_dir(exe.parent().unwrap_or(Path::new("."))).spawn().map(|_| ()).map_err(|e| e.to_string()),
                    None => Err("The mod's launch program is not in the game's folder yet. Open Check setup (or press Play again) so the launcher can finish the setup.".into()),
                }
            } else if g.steam_app_id != 0 {
                crate::open::open_url(&format!("steam://rungameid/{}", g.steam_app_id));
                Ok(())
            } else {
                let exe = folder.join(&g.default_exe_name);
                Command::new(&exe)
                    .current_dir(&folder)
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            };
            match started {
                Ok(()) => {
                    let _ = tx.send(Msg::GameLaunched(g.id.clone()));
                    ctx.request_repaint();
                    watch_game(&g.id, &exe_stem, &tx, &ctx);
                }
                Err(e) => say(format!("{} did not start ({e}).", g.display_name), true),
            }
        });
    }
}

/// Waits for the game's process to show up (it starts through Steam, so there is no process to hold), then for it to end.
fn watch_game(game_id: &str, exe_stem: &str, tx: &std::sync::mpsc::Sender<Msg>, ctx: &eframe::egui::Context) {
    if exe_stem.is_empty() {
        let _ = tx.send(Msg::GameExited(game_id.to_string()));
        return;
    }
    let mut seen = false;
    for _ in 0..240 {
        std::thread::sleep(Duration::from_secs(1));
        if runtime::find_process(exe_stem).is_some() {
            seen = true;
            break;
        }
    }
    while seen && runtime::find_process(exe_stem).is_some() {
        std::thread::sleep(Duration::from_secs(2));
    }
    let _ = tx.send(Msg::GameExited(game_id.to_string()));
    ctx.request_repaint();
}

/// Whether a mod's contract is an external one (convenience for callers that hold only the kind).
pub fn is_external_kind(kind: &str) -> bool {
    kind == PackKinds::EXTERNAL
}

/// A manifest check used by the catalog tests of this module.
#[allow(dead_code)]
fn parse_manifest(json: &str) -> Option<GameManifest> {
    GameManifest::parse(json)
}
