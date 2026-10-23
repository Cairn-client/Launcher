// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Host and Join for a game whose pack runs the session (`session` in its contract): the pack's program starts the relay, the game and the
//! plugin, and the launcher shows what it reports.

use crate::app::{CairnApp, Msg};
use crate::games::GameProfile;
use cairn_setup::external_install;
use cairn_setup::pack_session::{PackSession, SessionEvent, SessionFriends, SessionView};
use std::path::{Path, PathBuf};

/// What the Host / Join windows hold.
#[derive(Debug, Clone, Default)]
pub struct SessionUi {
    /// `Some(true)`: the host window is open; `Some(false)`: the join window.
    pub open: Option<bool>,
    pub address: String,
    pub steam_code: String,
    pub friends: Option<SessionFriends>,
    pub friends_busy: bool,
    /// A problem or message the program reported: title, text, next step.
    pub message: Option<(String, String, String)>,
}

fn resolve_against_launcher(p: &str) -> String {
    if p.trim().is_empty() {
        return String::new();
    }
    let path = Path::new(p);
    if path.is_absolute() {
        return p.to_string();
    }
    let base: PathBuf = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    base.join(path).to_string_lossy().into_owned()
}

impl CairnApp {
    /// The pack's program when the game's pack runs its session.
    pub fn session_program(&self, game_id: &str) -> Option<PathBuf> {
        self.store.read_contract(game_id).filter(|c| c.session)?;
        external_install::options_program_path(&self.store, game_id).filter(|p| p.is_file())
    }

    pub fn is_session_game(&self, game_id: &str) -> bool {
        self.session_program(game_id).is_some()
    }

    fn ensure_session(&mut self, game: &GameProfile) -> bool {
        if self.session.as_ref().is_some_and(|s| s.alive()) {
            if self.session_game.eq_ignore_ascii_case(&game.id) || self.running.is_some() {
                return true;
            }
            self.release_session();
        }
        let Some(program) = self.session_program(&game.id) else {
            self.toast(
                "This mod is too old for this launcher: update it from the game's page, then try again.",
                true,
            );
            return false;
        };
        let Some(s) = PackSession::start(&program) else {
            self.toast("The multiplayer helper did not start.", true);
            return false;
        };
        if let Some(rx) = s.take_events() {
            let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                for ev in rx {
                    if tx.send(Msg::Session(ev)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            });
        }
        self.session_game = game.id.clone();
        self.session = Some(s);
        true
    }

    /// The player's settings, sent before a command that uses them.
    fn configure_session(&mut self, game: &GameProfile) {
        let _ = self.settings_store.refresh(&mut self.settings);
        let st = &self.settings;
        let exe = self
            .finder
            .exe_of(game, &st.game_path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let save = std::env::var("CAIRN_START_SAVE").unwrap_or_else(|_| resolve_against_launcher(&st.start_save_path));
        if let Some(s) = &self.session {
            s.configure(
                &exe,
                st.host_port,
                st.server_info_port,
                st.version_ipc_port,
                st.steam_app_id,
                st.host_allow_steam,
                st.relay_reuse,
                st.connect_gate,
                st.inject_delay_seconds,
                st.menu_takeover,
                &resolve_against_launcher(&st.agent_path),
                &resolve_against_launcher(&st.relay_path),
                &resolve_against_launcher(&st.dll_path),
                &save,
            );
        }
    }

    /// Host: the relay is started, then the host window shows how friends join.
    pub fn open_host(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        if !self.ensure_session(&game) {
            return;
        }
        self.configure_session(&game);
        if !self.session_view.host.relay_up {
            if let Some(s) = &self.session {
                s.relay();
            }
        }
        self.session_ui.open = Some(true);
        self.session_ui.message = None;
    }

    pub fn open_join(&mut self, game_id: &str) {
        let Some(game) = self.registry.get(game_id).cloned() else { return };
        if !self.ensure_session(&game) {
            return;
        }
        self.session_ui.open = Some(false);
        self.session_ui.message = None;
    }

    /// Starts the game for a launch: the host joins their own relay; a joiner uses the address or Steam code typed.
    pub fn session_launch(&mut self, hosting: bool) {
        let Some(game) = self.registry.get(&self.session_game).cloned() else {
            return;
        };
        self.configure_session(&game);
        let (ip, port) = if hosting {
            ("127.0.0.1".to_string(), self.settings.host_port)
        } else {
            split_address(self.session_ui.address.trim(), self.settings.host_port)
        };
        let code = (!hosting && !self.session_ui.steam_code.trim().is_empty()).then(|| self.session_ui.steam_code.trim().to_string());
        if !hosting && code.is_none() && ip.is_empty() {
            self.toast("Type the host's address or Steam code first.", true);
            return;
        }
        self.session_ui.message = None;
        if let Some(s) = &self.session {
            s.launch(&ip, port, code.as_deref(), 0, false);
        }
    }

    pub fn session_connect(&mut self) {
        if let Some(game) = self.registry.get(&self.session_game).cloned() {
            self.configure_session(&game);
        }
        if let Some(s) = &self.session {
            s.connect();
        }
    }

    pub fn session_find_friends(&mut self) {
        let Some(game) = self.registry.get(&self.session_game).cloned() else {
            return;
        };
        self.configure_session(&game);
        self.session_ui.friends_busy = true;
        if let Some(s) = &self.session {
            s.ask_friends();
        }
    }

    /// Ends the program (its relay goes with it).
    pub fn release_session(&mut self) {
        if let Some(s) = self.session.take() {
            s.relay_stop();
        }
        self.session_view = SessionView::idle();
        self.session_ui.open = None;
    }

    pub fn on_session_event(&mut self, ev: SessionEvent) {
        match ev {
            SessionEvent::State(v) => self.session_view = v,
            SessionEvent::Problem(p) => self.session_ui.message = Some((p.title, p.text, p.next)),
            SessionEvent::Message { title, text, next } => self.session_ui.message = Some((title, text, next)),
            SessionEvent::Error(t) => self.toast(&t, true),
            SessionEvent::Toast(t) => self.toast(&t, false),
            SessionEvent::VersionMismatch(t) => self.session_ui.message = Some(("VERSION MISMATCH".into(), t, String::new())),
            SessionEvent::Refused { text, .. } => self.session_ui.message = Some(("VERSION MISMATCH".into(), text, String::new())),
            SessionEvent::Friends(f) => {
                self.session_ui.friends_busy = false;
                self.session_ui.friends = Some(f);
            }
            SessionEvent::Game(g) => {
                let id = self.session_game.clone();
                self.running = Some(crate::app::RunningGame {
                    game_id: id.clone(),
                    since: std::time::Instant::now(),
                });
                self.recent
                    .record(&id, if g.hosting { "host" } else { "join" }, cairn_setup::time_util::now_utc(), "");
            }
            SessionEvent::GameExit => {
                let id = self.session_game.clone();
                let _ = self.tx.send(Msg::GameExited(id));
                self.release_session();
            }
            SessionEvent::Exited => {
                if self.session.as_ref().is_some_and(|s| !s.alive()) {
                    self.session = None;
                    if self.session_view.stage != "idle" {
                        self.toast(
                            "The multiplayer helper stopped unexpectedly. Try again; if it keeps failing, send the logs with Report a bug.",
                            true,
                        );
                    }
                    self.session_view = SessionView::idle();
                }
            }
            SessionEvent::Log(_) => {}
        }
    }
}

/// `host:port` (or just a host) into its parts; the default port when none is given.
pub fn split_address(text: &str, default_port: i32) -> (String, i32) {
    if let Some((h, p)) = text.rsplit_once(':') {
        if !h.contains(':') {
            if let Ok(p) = p.parse::<i32>() {
                return (h.to_string(), p);
            }
        }
    }
    (text.to_string(), default_port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_split_into_host_and_port() {
        assert_eq!(split_address("203.0.113.7:7778", 1), ("203.0.113.7".into(), 7778));
        assert_eq!(split_address("my.host.net", 7778), ("my.host.net".into(), 7778));
        assert_eq!(split_address("", 5), (String::new(), 5));
    }
}
