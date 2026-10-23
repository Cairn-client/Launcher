// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The launcher's end of presence: one WebSocket to the master server for as long as the launcher is open. It tells the master who this player is
//! and what they do, and reports Steam friends who also run Cairn coming online, changing what they play, or inviting this player.

use cairn_protocol::presence_api::{
    self as api, PresenceFriend, PresenceHello, PresenceInvite, PresenceInvited, PresenceMessage, PresenceStatus,
};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tungstenite::{client_tls_with_config, Connector, Message};

/// Who we are and which friends we have, read again at every connect.
pub type Identity = Box<dyn Fn() -> Option<(String, String, Vec<String>)> + Send>;

#[derive(Debug, Clone, PartialEq)]
pub enum PresenceEvent {
    Connected(bool),
    Friend(PresenceFriend),
    Invited(PresenceInvited),
}

enum Cmd {
    Status(PresenceStatus),
    Invite(PresenceInvite),
    Stop,
}

pub struct PresenceClient {
    cmd: Sender<Cmd>,
}

/// The master's address for presence, or `None` when it is this machine (a default install has no shared master: nobody else is there to see).
pub fn socket_url(master_url: &str) -> Option<String> {
    let rest = master_url.trim();
    let (scheme, after) = rest.split_once("://")?;
    let authority = after.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let bare = host.trim_start_matches('[').split([']', ':']).next().unwrap_or("");
    if bare.is_empty() || bare.eq_ignore_ascii_case("localhost") || bare.starts_with("127.") || bare == "::1" || bare == "0.0.0.0" {
        return None;
    }
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "https" | "wss" => "wss",
        "http" | "ws" => "ws",
        _ => return None,
    };
    Some(format!("{scheme}://{host}{}", api::CONNECT_PATH))
}

fn message(kind: &str) -> PresenceMessage {
    PresenceMessage {
        kind: kind.into(),
        api_version: api::VERSION,
        ..Default::default()
    }
}

impl PresenceClient {
    /// Starts the connection thread; `None` when the master is this machine.
    pub fn start(master_url: &str, identity: Identity, events: impl Fn(PresenceEvent) + Send + 'static) -> Option<Self> {
        let url = socket_url(master_url)?;
        let (cmd, rx) = mpsc::channel();
        std::thread::spawn(move || run(&url, identity, rx, &events));
        Some(Self { cmd })
    }

    pub fn set_status(&self, activity: &str, game: &str) {
        let activity = if api::activity::valid(Some(activity)) {
            activity
        } else {
            api::activity::ONLINE
        };
        let _ = self.cmd.send(Cmd::Status(PresenceStatus {
            activity: activity.into(),
            game: game.into(),
        }));
    }

    pub fn invite(&self, friend_id: &str, game: &str, code: &str) {
        let _ = self.cmd.send(Cmd::Invite(PresenceInvite {
            to: friend_id.into(),
            game: game.into(),
            code: code.into(),
        }));
    }
}

impl Drop for PresenceClient {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Stop);
    }
}

fn tls() -> Connector {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("the ring provider supports the default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Connector::Rustls(Arc::new(config))
}

fn run(url: &str, identity: Identity, rx: Receiver<Cmd>, events: &dyn Fn(PresenceEvent)) {
    let mut status = PresenceStatus::default();
    let mut backoff = 5u64;
    'outer: loop {
        let Some((id, name, friends)) = identity() else {
            if wait(&rx, Duration::from_secs(60), &mut status) {
                return;
            }
            continue;
        };
        let session = connect(url).and_then(|mut ws| {
            let mut hello = message(api::message_type::HELLO);
            hello.hello = Some(PresenceHello {
                id,
                name,
                friends: friends.into_iter().take(api::MAX_FRIENDS).collect(),
                status: status.clone(),
            });
            send(&mut ws, &hello)?;
            Ok(ws)
        });
        if let Ok(mut ws) = session {
            backoff = 5;
            let mut last_ping = Instant::now();
            loop {
                while let Ok(c) = rx.try_recv() {
                    match c {
                        Cmd::Stop => {
                            let _ = ws.close(None);
                            return;
                        }
                        Cmd::Status(s) => {
                            status = s.clone();
                            let mut m = message(api::message_type::STATUS);
                            m.status = Some(s);
                            if send(&mut ws, &m).is_err() {
                                break;
                            }
                        }
                        Cmd::Invite(i) => {
                            let mut m = message(api::message_type::INVITE);
                            m.invite = Some(i);
                            if send(&mut ws, &m).is_err() {
                                break;
                            }
                        }
                    }
                }
                if last_ping.elapsed() >= Duration::from_secs(api::PING_SECONDS as u64) {
                    last_ping = Instant::now();
                    if send(&mut ws, &message(api::message_type::PING)).is_err() {
                        break;
                    }
                }
                match ws.read() {
                    Ok(Message::Text(t)) => {
                        if t.len() <= api::MAX_MESSAGE_BYTES {
                            if let Ok(m) = serde_json::from_str::<PresenceMessage>(t.as_str()) {
                                handle(m, events);
                            }
                        } else {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                    }
                    Err(_) => break,
                }
            }
            events(PresenceEvent::Connected(false));
        }
        if wait(&rx, Duration::from_secs(backoff), &mut status) {
            return;
        }
        backoff = (backoff * 2).min(120);
        continue 'outer;
    }
}

/// Waits out a delay, keeping the status that is asked meanwhile; `true` when the client was dropped.
fn wait(rx: &Receiver<Cmd>, how_long: Duration, status: &mut PresenceStatus) -> bool {
    let until = Instant::now() + how_long;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(Cmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return true,
            Ok(Cmd::Status(s)) => *status = s,
            Ok(Cmd::Invite(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => break,
        }
    }
    false
}

type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

fn connect(url: &str) -> Result<Socket, tungstenite::Error> {
    let uri: tungstenite::http::Uri = url
        .parse()
        .map_err(|_| tungstenite::Error::Url(tungstenite::error::UrlError::NoPathOrQuery))?;
    let host = uri.host().unwrap_or_default().to_string();
    let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("wss") { 443 } else { 80 });
    let tcp = TcpStream::connect((host.as_str(), port))?;
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(Duration::from_secs(1)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(10)))?;
    let (ws, _) = client_tls_with_config(url, tcp, None, Some(tls())).map_err(|e| match e {
        tungstenite::HandshakeError::Failure(e) => e,
        tungstenite::HandshakeError::Interrupted(_) => tungstenite::Error::Io(std::io::ErrorKind::TimedOut.into()),
    })?;
    Ok(ws)
}

fn send(ws: &mut Socket, m: &PresenceMessage) -> Result<(), tungstenite::Error> {
    ws.send(Message::text(serde_json::to_string(m).unwrap_or_default()))
}

/// One message from the master (public so the rules can be tested without a socket).
pub fn handle(m: PresenceMessage, events: &dyn Fn(PresenceEvent)) {
    match m.kind.as_str() {
        api::message_type::WELCOME => events(PresenceEvent::Connected(true)),
        api::message_type::FRIEND => {
            if let Some(f) = m.friend.filter(|f| api::valid_id(Some(&f.id))) {
                events(PresenceEvent::Friend(f));
            }
        }
        api::message_type::INVITED => {
            if let Some(i) = m.invited.filter(|i| api::valid_id(Some(&i.from)) && !i.code.is_empty()) {
                events(PresenceEvent::Invited(i));
            }
        }
        _ => {}
    }
}

impl crate::app::CairnApp {
    /// Opens the connection once the settings say so and the master is a shared one.
    pub fn start_presence(&mut self) {
        if self.presence_tried || !self.settings.presence_enabled {
            return;
        }
        self.presence_tried = true;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let identity: Identity = Box::new(|| {
            let me = crate::steam_service::read_self().local?;
            Some((me.id, me.name, me.friends))
        });
        self.presence = PresenceClient::start(&self.settings.master_server_url, identity, move |e| {
            let _ = tx.send(crate::app::Msg::Presence(e));
            ctx.request_repaint();
        });
    }

    pub fn on_presence(&mut self, e: PresenceEvent) {
        match e {
            PresenceEvent::Connected(on) => {
                self.presence_connected = on;
                if !on {
                    self.friends_seen.clear();
                }
            }
            PresenceEvent::Friend(f) => {
                let before = self.friends_seen.get(&f.id).cloned();
                if f.online {
                    self.friends_seen.insert(f.id.clone(), f.clone());
                } else {
                    self.friends_seen.remove(&f.id);
                }
                if self.settings.notify_friends {
                    if let Some(text) = api::notices::for_friend(before.as_ref(), &f) {
                        self.toast(&text, false);
                    }
                }
            }
            PresenceEvent::Invited(i) => {
                if self.settings.notify_friends {
                    self.toast(&api::notices::for_invite(&i), false);
                    self.pending_invite = Some(i);
                }
            }
        }
    }

    /// What this player is doing, from the game this launcher started (online when nothing runs).
    pub fn publish_presence(&self) {
        let Some(p) = &self.presence else { return };
        match &self.running {
            Some(r) => {
                let name = self.registry.get(&r.game_id).map(|g| g.display_name.clone()).unwrap_or_default();
                let hosting = self.session_view.host.relay_up;
                p.set_status(if hosting { api::activity::HOSTING } else { api::activity::PLAYING }, &name);
            }
            None => p.set_status(api::activity::ONLINE, ""),
        }
    }

    /// Whether the Invite button can be offered: this player hosts and the Steam code is ready.
    pub fn can_invite(&self) -> bool {
        self.presence.is_some() && self.running.is_some() && self.session_view.host.relay_up && self.session_view.host.steam_code.is_some()
    }

    pub fn invite_friend(&mut self, friend_id: &str) {
        let (Some(p), Some(code)) = (&self.presence, self.session_view.host.steam_code.clone()) else {
            return;
        };
        let Some(f) = self.friends_seen.get(friend_id).cloned() else {
            self.toast("That friend is not on Cairn right now.", true);
            return;
        };
        let game = self
            .running
            .as_ref()
            .and_then(|r| self.registry.get(&r.game_id))
            .map(|g| g.display_name.clone())
            .unwrap_or_default();
        p.invite(friend_id, &game, &code);
        self.toast(&format!("Invitation sent to {}", f.name), false);
    }

    /// The invitation in the corner: Join opens the join window with the host's code.
    pub fn invite_window(&mut self, ctx: &eframe::egui::Context) {
        use eframe::egui;
        let Some(i) = self.pending_invite.clone() else { return };
        let (mut join, mut dismiss) = (false, false);
        egui::Window::new(egui::RichText::new("INVITATION").strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                ui.label(api::notices::for_invite(&i));
                ui.horizontal(|ui| {
                    if crate::ui::widgets::primary_button(ui, "JOIN", self.accent, self.running.is_none()).clicked() {
                        join = true;
                    }
                    if crate::ui::widgets::secondary_button(ui, "DISMISS", true).clicked() {
                        dismiss = true;
                    }
                });
                if self.running.is_some() {
                    ui.label("The game is already running. Close it first.");
                }
            });
        if join {
            let id = self
                .registry
                .all()
                .into_iter()
                .find(|g| g.display_name.eq_ignore_ascii_case(&i.game) && self.is_session_game(&g.id))
                .map(|g| g.id.clone())
                .unwrap_or_else(|| self.selected_game.clone());
            self.open_join(&id);
            self.session_ui.steam_code = i.code.clone();
            self.pending_invite = None;
        }
        if dismiss {
            self.pending_invite = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn a_local_master_has_no_presence_and_a_remote_one_gets_a_socket_address() {
        assert_eq!(socket_url("http://localhost:5000"), None);
        assert_eq!(socket_url("http://127.0.0.1:5000/"), None);
        assert_eq!(socket_url(""), None);
        assert_eq!(socket_url("ftp://x.example"), None);
        assert_eq!(
            socket_url("https://master.example.net").as_deref(),
            Some("wss://master.example.net/api/v1/presence/connect")
        );
        assert_eq!(
            socket_url("http://10.0.0.5:8080/x").as_deref(),
            Some("ws://10.0.0.5:8080/api/v1/presence/connect")
        );
    }

    #[test]
    fn the_masters_messages_become_events() {
        let got = RefCell::new(Vec::new());
        let push = |e| got.borrow_mut().push(e);
        handle(message(api::message_type::WELCOME), &push);
        let mut f = message(api::message_type::FRIEND);
        f.friend = Some(PresenceFriend {
            id: "76561198000000001".into(),
            name: "Ann".into(),
            online: true,
            ..Default::default()
        });
        handle(f, &push);
        let mut bad = message(api::message_type::INVITED);
        bad.invited = Some(PresenceInvited {
            from: "76561198000000001".into(),
            name: "Ann".into(),
            game: "g".into(),
            code: String::new(), // no code: dropped
        });
        handle(bad, &push);
        assert_eq!(got.borrow().len(), 2);
        assert_eq!(got.borrow()[0], PresenceEvent::Connected(true));
    }
}
