// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The application: what the window shows and the state behind it. The pages are in `ui`; each page is another `impl` of [`CairnApp`].

use crate::app_dirs;
use crate::changelog::{self, ReleaseNote};
use crate::downloads::{DownloadItem, DownloadManager};
use crate::game_finder::GameFinder;
use crate::games::{GameProfile, GameRegistry};
use crate::home_data::RecentSessions;
use crate::mods::{self, JobEnv, Offers, RepoGame};
use crate::preflight::PreflightItem;
use crate::prefs::{GameListPrefs, GamePathStore};
use crate::settings::{migrate_stale, AppSettings, SettingsStore};
use crate::setup_list::SetupLine;
use crate::steam_service::{self, SteamSelf};
use crate::ui::images::ImageCache;
use crate::ui::theme;
use crate::update_check::{self, Check, UpdateInfo};
use cairn_setup::bridge_protocol::{BridgeIn, BridgeSaveSlot};
use cairn_setup::bridge_server::BridgeServer;
use cairn_setup::catalog_client::{CatalogClient, CatalogSnapshot};
use cairn_setup::external_install;
use cairn_setup::http::{Http, UreqHttp};
use cairn_setup::packs::index::launcher_is_enough;
use cairn_setup::packs::plan;
use cairn_setup::packs::{InstalledPack, PackKinds, PackStore};
use cairn_setup::steam_profile_status::{ProfileInfo, Status};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The version of this launcher, as the packs' `launcherMin` is compared with.
/// The release number: the `VERSION` file at the repository root, read when the launcher is built.
pub const VERSION: &str = env!("CAIRN_VERSION");

/// The pages of the top bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Home,
    Library,
    Store,
    Versions,
    Help,
    Settings,
    Profile,
}

/// What a background task tells the window.
pub enum Msg {
    Catalog(Option<CatalogSnapshot>),
    Offers(Offers),
    Changelog(Option<Vec<ReleaseNote>>),
    Steam(SteamSelf),
    ProfileInfo(Option<ProfileInfo>),
    FriendStatus(String, Status),
    UpdateChecked(Check, bool),
    UpdateProgress(f64),
    SetupLines {
        game: String,
        lines: Vec<SetupLine>,
        game_exe: Option<String>,
    },
    SetupAction {
        game: String,
        step: String,
        ok: bool,
        note: String,
    },
    OpenSetup(String),
    GameLaunched(String),
    GameExited(String),
    BridgeConnected(String),
    Bridge(BridgeIn),
    Session(cairn_setup::pack_session::SessionEvent),
    Presence(crate::presence::PresenceEvent),
    HomeFriends {
        game: String,
        friends: Vec<cairn_setup::pack_session::SessionFriend>,
    },
    GameExtras {
        game: String,
        options: HashMap<String, cairn_setup::game_options::OptionStatus>,
        saves: Option<Vec<BridgeSaveSlot>>,
    },
    OptionsApplied {
        game: String,
        results: Vec<(String, bool, bool, String)>,
    },
    SavesChanged {
        ok: bool,
        message: String,
        slots: Option<Vec<BridgeSaveSlot>>,
    },
    UpdateDownloaded(Result<std::path::PathBuf, String>),
    /// The pack of a game changed on disk (downloaded, removed, placed): forget what was read about it.
    PackChanged(String),
    Toast(String, bool),
}

/// A short note in a corner of the window.
pub struct Toast {
    pub text: String,
    pub error: bool,
    pub since: Instant,
}

pub struct CairnApp {
    pub ctx: egui::Context,
    pub view: View,
    pub settings: AppSettings,
    pub settings_store: SettingsStore,
    pub registry: GameRegistry,
    pub prefs: GameListPrefs,
    pub prefs_file: PathBuf,
    pub finder: GameFinder,
    pub images: ImageCache,
    pub http: Arc<dyn Http>,
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub selected_game: String,
    pub search: String,
    /// Whether the top bar's search shows its list of matches.
    pub search_open: bool,
    pub catalog_loading: bool,
    pub accent: egui::Color32,
    // mods
    pub store: PackStore,
    pub offers: Offers,
    pub offers_loading: bool,
    pub downloads: DownloadManager,
    installed_cache: HashMap<String, Option<InstalledPack>>,
    placed_cache: HashMap<String, (Instant, bool)>,
    pub pack_message: (String, bool),
    pub toasts: Vec<Toast>,
    pub updates_open: bool,
    /// The download speed, a sample a second, for the graph in the updates tab.
    pub speed_history: Vec<f32>,
    pub speed_at: std::time::Instant,
    // home
    pub recent: RecentSessions,
    pub release_notes: Option<Vec<ReleaseNote>>,
    pub slide: usize,
    pub slide_since: Instant,
    // store
    pub store_chosen: Option<String>,
    pub store_query: String,
    pub store_feature_tab: String,
    // settings
    pub settings_tab: String,
    pub settings_query: String,
    pub game_settings_open: Option<String>,
    pub game_settings_tab: String,
    pub confirm_uninstall: bool,
    // steam and friends
    pub steam: SteamSelf,
    pub steam_loaded: bool,
    pub profile_info: Option<ProfileInfo>,
    pub friend_status: HashMap<String, (Instant, Status)>,
    pub friends_open: bool,
    pub friends_query: String,
    // updates of the launcher
    pub update: Option<UpdateInfo>,
    pub update_checking: bool,
    pub update_last_failed: bool,
    pub update_modal: Option<UpdateModal>,
    // playing
    pub running: Option<RunningGame>,
    pub setup_lines: Vec<SetupLine>,
    pub setup_game: String,
    pub setup_checking: bool,
    pub setup_open: bool,
    pub setup_then: Option<(String, crate::play::After)>,
    pub setup_busy: Option<String>,
    pub setup_notes: HashMap<String, String>,
    pub setup_last_poll: Instant,
    pub preflight_open: bool,
    pub preflight_items: Vec<PreflightItem>,
    pub bridge: Option<BridgeServer>,
    pub bridge_game: String,
    pub bridge_saves: Vec<BridgeSaveSlot>,
    pub window_saved: (Instant, Option<crate::prefs::WindowPrefs>),
    pub option_status: HashMap<String, cairn_setup::game_options::OptionStatus>,
    pub options_busy: bool,
    pub saves: Option<Vec<BridgeSaveSlot>>,
    pub saves_loading: bool,
    pub confirm_delete_save: String,
    pub confirm_delete_all: bool,
    pub presence: Option<crate::presence::PresenceClient>,
    pub home_friends: Vec<cairn_setup::pack_session::SessionFriend>,
    pub home_friends_busy: bool,
    pub home_friends_at: Option<Instant>,
    pub presence_tried: bool,
    pub presence_sent: bool,
    pub presence_connected: bool,
    pub friends_seen: HashMap<String, cairn_protocol::presence_api::PresenceFriend>,
    pub pending_invite: Option<cairn_protocol::presence_api::PresenceInvited>,
    pub ingame: Option<crate::ingame::InGame>,
    pub tray: Option<crate::winapi::Tray>,
    pub tray_tried: bool,
    pub tray_events: Option<std::sync::mpsc::Receiver<crate::winapi::TrayEvent>>,
    pub confirm_exit: bool,
    pub quit_ok: bool,
    pub tour_step: usize,
    pub tour_steam: (Instant, bool),
    pub session: Option<cairn_setup::pack_session::PackSession>,
    pub session_game: String,
    pub session_view: cairn_setup::pack_session::SessionView,
    pub session_ui: crate::session::SessionUi,
}

/// A game the launcher started and that has not closed yet.
#[derive(Debug, Clone)]
pub struct RunningGame {
    pub game_id: String,
    pub since: Instant,
}

/// The window that offers the launcher's own update.
#[derive(Debug, Clone)]
pub struct UpdateModal {
    /// `offer`, `busy`, `error`, `current` or `unreachable`.
    pub stage: String,
    pub text: String,
    pub progress: Option<f64>,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl CairnApp {
    pub fn new(ctx: egui::Context) -> Self {
        let http: Arc<dyn Http> = Arc::new(UreqHttp::new(&format!("Cairn-launcher/{VERSION}")));
        let mut settings_store = SettingsStore::new(settings_path(), Some(migrate_stale));
        let mut problems = Vec::new();
        let settings = settings_store.load(&mut problems);
        let prefs_file = app_dirs::list_prefs_file();
        let prefs = GameListPrefs::load(&prefs_file);
        let registry = GameRegistry::new();
        let selected_game = registry.all().first().map(|g| g.id.clone()).unwrap_or_default();
        let (tx, rx) = channel();
        let accent = theme::accent_of(&settings.accent_color).0;
        theme::install(&ctx, accent);
        let view = match std::env::var("CAIRN_VIEW").unwrap_or_else(|_| settings.start_page.clone()).as_str() {
            "library" => View::Library,
            "store" => View::Store,
            "versions" => View::Versions,
            "help" => View::Help,
            "settings" => View::Settings,
            "profile" => View::Profile,
            _ => View::Home,
        };
        let store = PackStore::new(app_dirs::packs_root());
        let env = Arc::new(JobEnv {
            http: http.clone(),
            store: store.clone(),
            start_save_root: start_save_root(&settings),
        });
        let repaint = ctx.clone();
        let downloads = DownloadManager::new(mods::runner(env), move || repaint.request_repaint());
        let mut app = Self {
            images: ImageCache::new(http.clone(), app_dirs::local_data().join("cache").join("img")),
            finder: GameFinder::new(GamePathStore::load(app_dirs::game_paths_file())),
            ctx,
            view,
            settings,
            settings_store,
            registry,
            prefs,
            prefs_file,
            http,
            tx,
            rx,
            selected_game,
            search: String::new(),
            search_open: false,
            catalog_loading: false,
            accent,
            store,
            offers: Offers::default(),
            offers_loading: false,
            downloads,
            installed_cache: HashMap::new(),
            placed_cache: HashMap::new(),
            pack_message: (String::new(), false),
            toasts: Vec::new(),
            updates_open: std::env::var_os("CAIRN_OPEN_UPDATES").is_some(),
            speed_history: Vec::new(),
            speed_at: std::time::Instant::now(),
            recent: RecentSessions::load(app_dirs::local_data().join("recent.json")),
            release_notes: None,
            slide: 0,
            slide_since: Instant::now(),
            store_chosen: std::env::var("CAIRN_STORE_GAME").ok(),
            store_query: String::new(),
            store_feature_tab: String::new(),
            settings_tab: "general".into(),
            settings_query: String::new(),
            game_settings_open: None,
            game_settings_tab: "settings".into(),
            confirm_uninstall: false,
            steam: SteamSelf::default(),
            steam_loaded: false,
            profile_info: None,
            friend_status: HashMap::new(),
            friends_open: false,
            friends_query: String::new(),
            update: None,
            update_checking: false,
            update_last_failed: false,
            update_modal: None,
            running: None,
            setup_lines: Vec::new(),
            setup_game: String::new(),
            setup_checking: false,
            setup_open: false,
            setup_then: None,
            setup_busy: None,
            setup_notes: HashMap::new(),
            setup_last_poll: Instant::now(),
            preflight_open: false,
            preflight_items: Vec::new(),
            bridge: None,
            bridge_game: String::new(),
            bridge_saves: Vec::new(),
            window_saved: (Instant::now(), None),
            option_status: HashMap::new(),
            options_busy: false,
            saves: None,
            saves_loading: false,
            confirm_delete_save: String::new(),
            confirm_delete_all: false,
            presence: None,
            home_friends: Vec::new(),
            home_friends_busy: false,
            home_friends_at: None,
            presence_tried: false,
            presence_sent: false,
            presence_connected: false,
            friends_seen: HashMap::new(),
            pending_invite: None,
            ingame: None,
            tray: None,
            tray_tried: false,
            tray_events: None,
            confirm_exit: std::env::var_os("CAIRN_SHOW_QUIT").is_some(),
            quit_ok: false,
            tour_step: 0,
            tour_steam: (Instant::now() - Duration::from_secs(10), false),
            session: None,
            session_game: String::new(),
            session_view: cairn_setup::pack_session::SessionView::idle(),
            session_ui: Default::default(),
        };
        for p in problems {
            app.toast(&p, true);
        }
        let _ = app.store.remove_broken(); // a pack with holes is downloaded again
        app.refresh_catalog_and_packs();
        app.load_changelog();
        app.load_steam();
        app.check_for_update(false);
        app
    }

    // ---- background work ------------------------------------------------------------------------------------------------

    /// At start, and from "Check for updates": the games first (the mod repositories come from them), then what each mod offers.
    pub fn refresh_catalog_and_packs(&mut self) {
        self.load_catalog();
    }

    /// Reads the catalog on a worker thread.
    pub fn load_catalog(&mut self) {
        if self.catalog_loading {
            return;
        }
        self.catalog_loading = true;
        let (tx, http, ctx) = (self.tx.clone(), self.http.clone(), self.ctx.clone());
        let beta: Vec<String> = self
            .settings
            .mod_channels
            .iter()
            .filter(|(_, c)| c.as_str() == "beta")
            .map(|(g, _)| g.to_lowercase())
            .collect();
        std::thread::spawn(move || {
            let client = CatalogClient::new(app_dirs::catalog_cache(), std::env::var("CAIRN_GAMES_URL").ok().as_deref(), None)
                .with_follows_beta(move |g| beta.contains(&g.to_lowercase()));
            let _ = tx.send(Msg::Catalog(client.load(http.as_ref())));
            ctx.request_repaint();
        });
    }

    /// Reads who is signed in to Steam, and then the player's own profile page.
    pub fn load_steam(&mut self) {
        let (tx, http, ctx, friend_status) = (self.tx.clone(), self.http.clone(), self.ctx.clone(), self.settings.friend_status);
        std::thread::spawn(move || {
            let me = steam_service::read_self();
            let id = me.id().to_string();
            let _ = tx.send(Msg::Steam(me));
            ctx.request_repaint();
            if friend_status && !id.is_empty() {
                let _ = tx.send(Msg::ProfileInfo(steam_service::own_profile(http.as_ref(), &id)));
                ctx.request_repaint();
            }
        });
    }

    /// Asks what the friends are doing (the ones not asked in the last minute), a few at a time.
    pub fn load_friend_statuses(&mut self) {
        if !self.settings.friend_status {
            return;
        }
        let mut ids: Vec<String> = Vec::new();
        if let Some(local) = &self.steam.local {
            for n in &local.named {
                if self
                    .friend_status
                    .get(&n.id)
                    .is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(60))
                {
                    ids.push(n.id.clone());
                }
            }
        }
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            // mark them asked, so the next frame does not ask again
            self.friend_status.insert(
                id.clone(),
                (
                    Instant::now(),
                    Status {
                        state: "loading".into(),
                        game: String::new(),
                        message: String::new(),
                    },
                ),
            );
        }
        let queue = Arc::new(std::sync::Mutex::new(ids));
        for _ in 0..4 {
            let (queue, tx, http, ctx) = (queue.clone(), self.tx.clone(), self.http.clone(), self.ctx.clone());
            std::thread::spawn(move || loop {
                let Some(id) = queue.lock().unwrap().pop() else { break };
                let status = steam_service::friend_status(http.as_ref(), &id);
                let _ = tx.send(Msg::FriendStatus(id, status));
                ctx.request_repaint();
            });
        }
    }

    /// Looks for a newer launcher; `open` shows the answer in a window.
    pub fn check_for_update(&mut self, open: bool) {
        if self.update_checking || self.update_modal.as_ref().is_some_and(|m| m.stage == "busy") {
            return;
        }
        self.update_checking = true;
        let (tx, http, ctx) = (self.tx.clone(), self.http.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::UpdateChecked(update_check::check(http.as_ref(), VERSION), open));
            ctx.request_repaint();
        });
    }

    /// Reads the launcher's release notes on a worker thread.
    pub fn load_changelog(&mut self) {
        let (tx, http, ctx) = (self.tx.clone(), self.http.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let notes = changelog::fetch(http.as_ref(), Some(&app_dirs::local_data().join("changelog-cache.json")));
            let _ = tx.send(Msg::Changelog(notes));
            ctx.request_repaint();
        });
    }

    /// Asks every mod repository what it offers, on a worker thread.
    pub fn load_offers(&mut self) {
        if self.offers_loading {
            return;
        }
        self.offers_loading = true;
        let games: Vec<RepoGame> = self
            .registry
            .all()
            .into_iter()
            .filter(|g| !g.mod_repo.is_empty())
            .map(|g| RepoGame {
                id: g.id.clone(),
                repo: g.mod_repo.clone(),
                channel: self.settings.channel_of(&g.id).to_string(),
            })
            .collect();
        let (tx, http, ctx, catalog) = (
            self.tx.clone(),
            self.http.clone(),
            self.ctx.clone(),
            self.registry.catalog().clone(),
        );
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Offers(mods::fetch_offers(http.as_ref(), &games, &catalog)));
            ctx.request_repaint();
        });
    }

    /// Takes what the background tasks have finished.
    fn receive(&mut self) {
        self.start_presence();
        self.run_tray();
        let was_running = self.running.as_ref().map(|r| r.game_id.clone());
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Catalog(snapshot) => {
                    self.catalog_loading = false;
                    if let Some(s) = snapshot {
                        self.registry.apply(&s);
                        self.finder.forget_all();
                        self.installed_cache.clear();
                        if self.registry.get(&self.selected_game).is_none() {
                            self.selected_game = self.registry.all().first().map(|g| g.id.clone()).unwrap_or_default();
                        }
                    }
                    self.load_offers();
                }
                Msg::Changelog(n) => self.release_notes = n,
                Msg::Steam(s) => {
                    self.steam = s;
                    self.steam_loaded = true;
                }
                Msg::ProfileInfo(i) => self.profile_info = i,
                Msg::FriendStatus(id, st) => {
                    self.friend_status.insert(id, (Instant::now(), st));
                }
                Msg::UpdateChecked(check, open) => {
                    self.update_checking = false;
                    self.update_last_failed = check == Check::Unreachable;
                    match check {
                        Check::Newer(info) => {
                            self.update = Some(info);
                            if open {
                                self.open_update_modal("offer", "");
                            }
                        }
                        Check::Current => {
                            self.update = None;
                            if open {
                                self.open_update_modal("current", "");
                            }
                        }
                        Check::Unreachable => {
                            if open {
                                self.open_update_modal("unreachable", "");
                            }
                        }
                    }
                }
                Msg::GameExtras { game, options, saves } => {
                    if self.game_settings_open.as_deref().is_some_and(|g| g.eq_ignore_ascii_case(&game)) {
                        for (id, st) in &options {
                            self.settings
                                .game_options
                                .entry(game.clone())
                                .or_default()
                                .insert(id.clone(), st.on);
                        }
                        self.option_status = options;
                        self.saves = saves;
                    }
                }
                Msg::OptionsApplied { game, results } => self.on_options_applied(&game, results),
                Msg::SavesChanged { ok, message, slots } => self.on_saves_changed(ok, message, slots),
                Msg::HomeFriends { friends, .. } => {
                    self.home_friends_busy = false;
                    self.home_friends = friends;
                }
                Msg::Presence(e) => self.on_presence(e),
                Msg::Session(ev) => self.on_session_event(ev),
                Msg::SetupLines { game, lines, game_exe } => {
                    self.setup_checking = false;
                    if game.eq_ignore_ascii_case(&self.setup_game) {
                        self.setup_lines = lines;
                    }
                    if let Some(exe) = game_exe {
                        self.adopt_detected_game_path(&game, &exe);
                    }
                }
                Msg::SetupAction { game, step, ok, note } => {
                    self.setup_busy = None;
                    if !note.is_empty() {
                        self.setup_notes.insert(format!("{game}:{step}"), note);
                    }
                    if !ok {
                        self.toast(&self.setup_notes.get(&format!("{game}:{step}")).cloned().unwrap_or_default(), true);
                    }
                    self.refresh_setup(&game);
                }
                Msg::OpenSetup(game) => {
                    self.setup_open = true;
                    self.refresh_setup(&game);
                }
                Msg::GameLaunched(id) => {
                    self.running = Some(RunningGame {
                        game_id: id,
                        since: Instant::now(),
                    })
                }
                Msg::GameExited(id) => {
                    if let Some(r) = self.running.take_if(|r| r.game_id.eq_ignore_ascii_case(&id)) {
                        let minutes = (r.since.elapsed().as_secs() / 60) as i32;
                        self.recent.add_playtime(&id, minutes);
                    }
                }
                Msg::BridgeConnected(game) => {
                    self.bridge_game = game.clone();
                    if game.is_empty() {
                        if let Some(g) = self.ingame.as_mut() {
                            g.bridge_gone();
                        }
                    }
                    if game.is_empty() {
                        self.bridge_saves.clear();
                    } else if let Some(b) = &self.bridge {
                        b.send(&cairn_setup::bridge_protocol::ask_saves());
                    }
                }
                Msg::Bridge(msg) => match msg {
                    BridgeIn::Toast { text, error } => self.toast(&text, error),
                    BridgeIn::Saves(slots) => self.bridge_saves = slots,
                    BridgeIn::Menu(m) => {
                        let state = crate::menu_nav::MenuState::from_bridge(&self.bridge_game, &m);
                        if let Some(g) = self.ingame.as_mut() {
                            g.on_menu(state);
                        }
                    }
                    BridgeIn::Notice { show, text } => {
                        if let Some(g) = self.ingame.as_mut() {
                            g.on_notice(show, text);
                        }
                    }
                    BridgeIn::Subtitle { seconds, text } => {
                        if let Some(g) = self.ingame.as_mut() {
                            g.on_subtitle(seconds, text);
                        }
                    }
                    BridgeIn::Hello { .. } => {}
                },
                Msg::UpdateProgress(p) => {
                    if let (Some(m), Some(u)) = (self.update_modal.as_mut(), self.update.as_ref()) {
                        m.progress = Some(p);
                        m.text = format!("Downloading {}: {}%", u.tag, (p * 100.0) as i32);
                    }
                }
                Msg::UpdateDownloaded(result) => match result {
                    Ok(path) => {
                        if let Some(m) = self.update_modal.as_mut() {
                            m.text = "Installing... the launcher closes and opens again by itself.".into();
                            m.progress = Some(1.0);
                        }
                        match update_check::start_installer(&path) {
                            Ok(()) => self.ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                            Err(e) => self.update_failed(&format!("The update could not be started: {e}")),
                        }
                    }
                    Err(why) => self.update_failed(&format!(
                        "The update could not be installed: {why}. You can download it by hand from the release page."
                    )),
                },
                Msg::Offers(o) => {
                    self.offers_loading = false;
                    self.offers = o;
                }
                Msg::PackChanged(id) => {
                    self.installed_cache.remove(&id.to_lowercase());
                    self.placed_cache.remove(&id.to_lowercase());
                }
                Msg::Toast(text, error) => self.toast(&text, error),
            }
        }
        self.toasts
            .retain(|t| t.since.elapsed() < Duration::from_secs(if t.error { 9 } else { 5 }));
        // downloads finishing change what is installed
        for item in self.downloads.snapshot() {
            if item.is_finished()
                && self.installed_cache.contains_key(&item.game_id.to_lowercase())
                && item.finished.is_some_and(|f| f.elapsed() < Duration::from_secs(2))
            {
                self.installed_cache.remove(&item.game_id.to_lowercase());
                self.placed_cache.remove(&item.game_id.to_lowercase());
            }
        }
        let now_running = self.running.as_ref().map(|r| r.game_id.clone());
        if now_running != was_running || (self.presence_connected && !self.presence_sent) {
            self.presence_sent = self.presence_connected;
            self.publish_presence();
        }
    }

    pub fn toast(&mut self, text: &str, error: bool) {
        if text.is_empty() {
            return;
        }
        self.toasts.push(Toast {
            text: text.to_string(),
            error,
            since: Instant::now(),
        });
        self.ctx.request_repaint();
    }

    pub fn set_pack_message(&mut self, text: &str, error: bool) {
        self.pack_message = (text.to_string(), error);
        self.toast(text, error);
    }

    pub fn save_prefs(&self) {
        self.prefs.save(&self.prefs_file);
    }

    // ---- what the games are ---------------------------------------------------------------------------------------------

    pub fn selected_profile(&self) -> Option<&GameProfile> {
        self.registry.get(&self.selected_game)
    }

    /// The game is on this PC.
    pub fn is_game_found(&mut self, g: &GameProfile) -> bool {
        let found = self.finder.find(g, &self.settings.game_path);
        self.registry.mark_installed(&g.id, found.as_deref().and_then(|p| p.to_str()));
        found.is_some()
    }

    pub fn game_folder_of(&mut self, g: &GameProfile) -> Option<PathBuf> {
        self.finder.find(g, &self.settings.game_path)
    }

    /// The mod pack on this PC (read once, until a download or a removal changes it).
    pub fn installed_pack_of(&mut self, game_id: &str) -> Option<InstalledPack> {
        let key = game_id.to_lowercase();
        if !self.installed_cache.contains_key(&key) {
            let p = self.store.read(&key).or_else(|| self.store.read(game_id));
            self.installed_cache.insert(key.clone(), p);
        }
        self.installed_cache[&key].clone()
    }

    pub fn is_mod_downloaded(&mut self, g: &GameProfile) -> bool {
        self.installed_pack_of(&g.id).is_some()
    }

    /// The game's mod is an external one: it runs the session itself. Its downloaded pack says so, or, before any download, the catalog's entry.
    pub fn is_external_mod(&mut self, g: &GameProfile) -> bool {
        if g.own_session {
            return true;
        }
        let kind = self
            .installed_pack_of(&g.id)
            .and_then(|p| p.contract)
            .map(|c| c.kind)
            .or_else(|| self.offers.of(&g.id).map(|o| o.kind.clone()));
        kind.as_deref() == Some(PackKinds::EXTERNAL)
    }

    /// The mod is really usable: downloaded and, for an external mod, placed in the game's folder.
    pub fn is_mod_installed(&mut self, g: &GameProfile) -> bool {
        if !self.is_mod_downloaded(g) {
            return false;
        }
        if self.is_external_mod(g) {
            let key = g.id.to_lowercase();
            if let Some((at, v)) = self.placed_cache.get(&key) {
                if at.elapsed() < Duration::from_secs(3) {
                    return *v;
                }
            }
            let placed = self
                .game_folder_of(g)
                .is_some_and(|folder| external_install::is_placed(&folder, &self.store, &g.id, None));
            self.placed_cache.insert(key, (Instant::now(), placed));
            return placed;
        }
        true
    }

    /// A supported game is not playable until its mod is downloaded.
    pub fn needs_mod(&mut self, g: &GameProfile) -> bool {
        g.is_supported && !self.is_mod_installed(g)
    }

    // ---- doing things with mods -----------------------------------------------------------------------------------------

    /// Queues a game's mod (its dependencies first). Works on a worker thread: what it depends on is asked of the network.
    pub fn download_pack(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        self.prefs.add_to_library(&game.id);
        self.save_prefs();
        if self.downloads.item_of(&game.id).is_some() {
            return;
        }
        let Some(info) = self.offers.of(&game.id).cloned() else {
            let why = self
                .offers
                .blocked
                .get(&game.id.to_lowercase())
                .map(|w| format!("The newest {} mod ({w}) is on hold, so it is not offered.", game.display_name));
            self.set_pack_message(&why.unwrap_or_else(|| "No mod is available for this game yet.".into()), true);
            return;
        };
        let catalog = self.registry.catalog().clone();
        let Some(url) = mods::url_for(&catalog, &info) else {
            self.set_pack_message("No mod is available for this game yet.", true);
            return;
        };
        if !launcher_is_enough(Some(&info.launcher_min), Some(VERSION)) {
            self.set_pack_message(
                &format!("This mod needs Cairn {} or newer. Update the launcher first.", info.launcher_min),
                true,
            );
            return;
        }
        let place_in = if info.kind == PackKinds::EXTERNAL {
            self.game_folder_of(&game)
        } else {
            None
        };
        let from_version = self.installed_pack_of(&game.id).map(|p| p.version);
        self.pack_message = (String::new(), false);
        let (store, http, downloads, tx, ctx) = (
            self.store.clone(),
            self.http.clone(),
            self.downloads.clone(),
            self.tx.clone(),
            self.ctx.clone(),
        );
        std::thread::spawn(move || {
            // the catalog's answer for everything the pack depends on, the dependencies' own included
            let mut known: HashMap<String, cairn_setup::packs::PackInfo> = HashMap::new();
            prefetch_dependencies(http.as_ref(), &catalog, &info, &mut known, 0);
            let plan = plan::plan(&info, &|id| store.read(id), &|id| known.get(&id.to_lowercase()).cloned());
            if let Some(problem) = plan.problem {
                let _ = tx.send(Msg::Toast(format!("Could not download {}: {problem}.", game.display_name), true));
                ctx.request_repaint();
                return;
            }
            for dep in plan.order.iter().filter(|p| !p.id.eq_ignore_ascii_case(&info.id)) {
                if let Some(dep_url) = mods::url_for(&catalog, dep) {
                    downloads.enqueue(DownloadItem::new(
                        &dep.id,
                        &dep.id,
                        store.read(&dep.id).map(|p| p.version),
                        dep.clone(),
                        dep_url,
                    ));
                }
            }
            let mut item = DownloadItem::new(&game.id, &game.display_name, from_version, info, url);
            item.place_in = place_in;
            downloads.enqueue(item);
            let _ = tx.send(Msg::PackChanged(game.id.clone()));
            ctx.request_repaint();
        });
    }

    /// Takes the mod out of the game's folder and deletes the downloaded copy.
    pub fn remove_pack(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        if self.downloads.item_of(&game.id).is_some_and(|i| i.is_active()) {
            return;
        }
        if self.is_external_mod(&game) {
            if let Some(folder) = self.game_folder_of(&game) {
                let r = external_install::unplace(&folder, &self.store, &game.id, None);
                if !r.ok {
                    self.set_pack_message(&format!("Could not remove the mod from the game ({}).", r.message), true);
                    return;
                }
            }
        }
        match self.store.remove(&game.id) {
            Ok(()) => self.set_pack_message(&format!("{} mod removed.", game.display_name), false),
            Err(e) => self.set_pack_message(&format!("Could not remove the mod ({e}). Close the game and try again."), true),
        }
        self.installed_cache.remove(&game.id.to_lowercase());
        self.placed_cache.remove(&game.id.to_lowercase());
    }

    /// Puts the version before the last update back.
    pub fn rollback_pack(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        let r = self.store.rollback(&game.id);
        self.installed_cache.remove(&game.id.to_lowercase());
        self.placed_cache.remove(&game.id.to_lowercase());
        if r.ok {
            if self.is_external_mod(&game) {
                if let Some(folder) = self.game_folder_of(&game) {
                    let placed = external_install::place(&folder, &self.store, &game.id, None);
                    if !placed.ok {
                        self.set_pack_message(&format!("Could not install the mod: {}.", placed.message), true);
                        return;
                    }
                }
            }
            if let Some(v) = self.installed_pack_of(&game.id).map(|p| p.version) {
                self.settings.held_back_mods.insert(game.id.clone(), v);
                let _ = self.settings_store.save(&self.settings, Some(&["HeldBackMods"]));
            }
        }
        self.set_pack_message(&r.message, !r.ok);
    }

    /// Puts a downloaded external mod into the game's folder (the fallback when the automatic placing failed).
    pub fn install_mod(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
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
        let r = external_install::place(&folder, &self.store, &game.id, None);
        self.placed_cache.remove(&game.id.to_lowercase());
        self.set_pack_message(
            &if r.ok {
                String::new()
            } else {
                format!("Could not install the mod: {}.", r.message)
            },
            !r.ok,
        );
    }

    /// What has an update on offer: the games whose mod is downloaded and the catalog has a newer version of.
    pub fn mod_updates(&mut self) -> Vec<(GameProfile, cairn_setup::packs::PackInfo)> {
        let games: Vec<GameProfile> = self.registry.all().into_iter().cloned().collect();
        let mut out = Vec::new();
        for g in games {
            let installed = self.installed_pack_of(&g.id);
            if let Some(update) = cairn_setup::packs::index::update_for(installed.as_ref(), self.offers.of(&g.id)).cloned() {
                out.push((g, update));
            }
        }
        out
    }

    /// Takes the settings the player changed and writes them.
    pub fn write_settings(&mut self, why: &str) {
        let r = self.settings_store.save(&self.settings, None);
        if !r.ok() {
            self.toast(&format!("Could not save the settings ({}).", r.result.why), true);
        } else if r.wrote() {
            eprintln!("settings: written ({why}): {}", r.set.join(", "));
        }
    }

    /// The accent colour: always the player's own, never a game's.
    pub fn accent_for(&self, g: Option<&GameProfile>) -> egui::Color32 {
        let _ = g;
        self.accent
    }
}

fn prefetch_dependencies(
    http: &dyn Http,
    catalog: &cairn_setup::catalog::GameList,
    pack: &cairn_setup::packs::PackInfo,
    known: &mut HashMap<String, cairn_setup::packs::PackInfo>,
    depth: usize,
) {
    if depth > 4 {
        return;
    }
    for dep in &pack.depends_on {
        if known.contains_key(&dep.id.to_lowercase()) {
            continue;
        }
        let Some(repo) = catalog.repo_of_pack(&dep.id) else { continue };
        let Some(offered) = plan::fetch_stable(http, repo).and_then(|i| i.packs.into_iter().find(|p| p.id.eq_ignore_ascii_case(&dep.id)))
        else {
            continue;
        };
        if catalog.block_of(&offered.id, &offered.version).is_some() {
            continue;
        }
        known.insert(dep.id.to_lowercase(), offered.clone());
        prefetch_dependencies(http, catalog, &offered, known, depth + 1);
    }
}

fn settings_path() -> PathBuf {
    // an installed launcher keeps settings.json in its own folder; started from elsewhere it uses the folder it was started in
    let own = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from));
    match own {
        Some(dir) if dir.join("install-manifest.txt").is_file() => dir.join("settings.json"),
        _ => PathBuf::from("settings.json"),
    }
}

/// Where a content pack of start saves is copied: `StartSavePath` of the settings, relative to the launcher's folder.
fn start_save_root(s: &AppSettings) -> PathBuf {
    let p = PathBuf::from(&s.start_save_path);
    if p.is_absolute() {
        return p;
    }
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&p)))
        .unwrap_or(p)
}

impl CairnApp {
    /// The notification-area icon: open Cairn, exit, and a notice when the window is not in front.
    fn run_tray(&mut self) {
        if !self.tray_tried {
            self.tray_tried = true;
            let (tx, rx) = std::sync::mpsc::channel();
            self.tray = crate::winapi::Tray::start(tx);
            self.tray_events = self.tray.as_ref().map(|_| rx);
        }
        let events: Vec<_> = self.tray_events.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for e in events {
            match e {
                crate::winapi::TrayEvent::Open => self.show_window(),
                crate::winapi::TrayEvent::Library => {
                    self.show_window();
                    self.view = View::Library;
                }
                crate::winapi::TrayEvent::Store => {
                    self.show_window();
                    self.view = View::Store;
                }
                crate::winapi::TrayEvent::Settings => {
                    self.show_window();
                    self.view = View::Settings;
                }
                crate::winapi::TrayEvent::CheckUpdates => {
                    self.show_window();
                    self.updates_open = true;
                    self.refresh_catalog_and_packs();
                }
                crate::winapi::TrayEvent::ReportProblem => crate::open::open_url("https://github.com/Cairn-client/Launcher/issues/new"),
                crate::winapi::TrayEvent::Exit => {
                    self.quit_ok = true;
                    self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// Brings the window back from the tray or the taskbar and to the front.
    fn show_window(&self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// A Windows notice from the tray icon, only when the launcher is not the window in front.
    pub fn tray_notice(&self, title: &str, text: &str) {
        if !self.settings.tray_notices || self.ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
            return;
        }
        if let Some(t) = &self.tray {
            t.notify(title, text);
        }
    }

    /// Remembers the window's size, place and maximised state for the next start (at most once a second, and only when it changed).
    fn track_window(&mut self, ctx: &egui::Context) {
        if self.window_saved.0.elapsed() < Duration::from_secs(1) {
            return;
        }
        let (inner, outer, maximized) = ctx.input(|i| {
            let v = i.viewport();
            (v.inner_rect, v.outer_rect, v.maximized.unwrap_or(false))
        });
        let (Some(inner), Some(outer)) = (inner, outer) else { return };
        let mut w = self.window_saved.1.clone().unwrap_or_default();
        w.maximized = maximized;
        if !maximized {
            w.width = inner.width() as i32;
            w.height = inner.height() as i32;
            w.x = outer.min.x as i32;
            w.y = outer.min.y as i32;
        }
        if self.window_saved.1.as_ref() != Some(&w) {
            if self.window_saved.1.is_some() {
                w.save(&crate::app_dirs::window_file());
            }
            self.window_saved.1 = Some(w);
        }
        self.window_saved.0 = Instant::now();
    }
}

impl eframe::App for CairnApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive();
        self.top_bar(ui);
        match self.view {
            View::Library => self.library(ui),
            View::Home => self.home(ui),
            View::Store => self.store(ui),
            View::Settings => self.settings_page(ui),
            View::Versions => self.versions_page(ui),
            View::Help => self.help_page(ui),
            View::Profile => self.profile_page(ui),
        }
        self.game_settings_window(ui.ctx());
        self.update_window(ui.ctx());
        self.ready_check_windows(ui.ctx());
        self.session_windows(ui.ctx());
        self.invite_window(ui.ctx());
        self.tick_ingame(ui.ctx());
        self.updates_drawer(ui.ctx());
        self.tour_window(ui.ctx());
        self.exit_window(ui.ctx());
        self.track_window(ui.ctx());
        self.poll_setup();
        self.continue_after_setup();
        self.friends_panel(ui.ctx());
        self.toasts_ui(ui);
        crate::dev::screenshot_tick(ui.ctx());
        if self.downloads.busy() || !self.toasts.is_empty() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
}
