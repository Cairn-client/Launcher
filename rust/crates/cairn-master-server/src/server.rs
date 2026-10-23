// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The routes: the browser's read-only side, the relay's connection, and presence.

use crate::config::Config;
use crate::hub::{Hub, Outgoing, Peer};
use crate::log;
use crate::registry::{self, Registry};
use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ws::rejection::WebSocketUpgradeRejection, ConnectInfo, Path, State as AxState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use cairn_protocol::master_api::{self as master, MasterAccepted, MasterMessage, MasterRejected, MasterStatus, ServerListResponse};
use cairn_protocol::presence_api::{self as api, PresenceMessage};
use futures_util::{SinkExt, StreamExt};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::unbounded_channel;

/// How long a relay or a launcher has to say hello after connecting.
const FIRST_MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct State {
    pub registry: Arc<Registry>,
    pub hub: Arc<Hub>,
    pub config: Arc<Config>,
}

impl State {
    pub fn new(config: Config) -> Self {
        Self {
            registry: Arc::new(Registry::new(config.max_servers)),
            hub: Arc::new(Hub::new()),
            config: Arc::new(config),
        }
    }
}

pub fn router(state: State) -> Router {
    Router::new()
        .route(master::LIST_PATH, get(list))
        .route(master::STATUS_PATH, get(status))
        // a literal segment outranks the parameter one, so /servers/connect reaches the relay and /servers/<id> the listing; the id the master
        // hands out is a hex GUID and can never collide with the literal either way
        .route(master::CONNECT_PATH, get(relay_connect))
        .route("/api/v1/servers/{id}", get(one))
        .route(api::CONNECT_PATH, get(presence_connect))
        .with_state(state)
}

pub async fn serve(listener: tokio::net::TcpListener, state: State) {
    let app = router(state).into_make_service_with_connect_info::<SocketAddr>();
    if let Err(e) = axum::serve(listener, app).await {
        log::warn(&format!("The server stopped: {e}"));
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---- the browser's side ----------------------------------------------------------------------------------------------------------------

async fn list(AxState(s): AxState<State>) -> Json<ServerListResponse> {
    Json(ServerListResponse {
        api_version: master::VERSION,
        servers: s.registry.all().iter().map(|x| x.to_listing()).collect(),
    })
}

async fn one(AxState(s): AxState<State>, Path(id): Path<String>) -> Response {
    // a 404 means "that server went offline", the common case: a listing id only lives as long as the connection that owns it
    match s.registry.get(&id) {
        Some(x) => Json(x.to_listing()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn status(AxState(s): AxState<State>) -> Json<MasterStatus> {
    Json(MasterStatus {
        api_version: master::VERSION,
        servers: s.registry.count() as i32,
        players: s.registry.total_players(),
    })
}

// ---- the address a connection came from -----------------------------------------------------------------------------------------------

fn remote_ip(config: &Config, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
    let direct = peer.ip();
    if config.trusts(direct) {
        // the proxy appends the address it saw: the last entry is the one it vouches for
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next_back())
            .and_then(|v| v.trim().parse::<IpAddr>().ok())
        {
            return ip;
        }
    }
    direct
}

fn not_a_websocket(text: String) -> Response {
    (StatusCode::BAD_REQUEST, text).into_response()
}

// ---- the relay's connection -----------------------------------------------------------------------------------------------------------

enum Received<T> {
    Message(T),
    Closed,
    TimedOut,
    Malformed(String),
    Dropped(String),
}

/// Reads one whole message as JSON, or says why not.
async fn receive<T: serde::de::DeserializeOwned>(socket: &mut WebSocket, timeout: Duration, max: usize) -> Received<T> {
    loop {
        let next = match tokio::time::timeout(timeout, socket.recv()).await {
            Err(_) => return Received::TimedOut,
            Ok(None) => return Received::Closed,
            Ok(Some(Err(e))) => return Received::Dropped(e.to_string()),
            Ok(Some(Ok(m))) => m,
        };
        let bytes: Vec<u8> = match next {
            Message::Text(t) => t.as_bytes().to_vec(),
            Message::Binary(b) => b.to_vec(),
            Message::Close(_) => return Received::Closed,
            Message::Ping(_) | Message::Pong(_) => continue,
        };
        if bytes.len() > max {
            return Received::Malformed(format!("Message exceeded {max} bytes."));
        }
        return match serde_json::from_slice(&bytes) {
            Ok(m) => Received::Message(m),
            Err(e) => Received::Malformed(e.to_string()),
        };
    }
}

async fn relay_connect(
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    AxState(s): AxState<State>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let Ok(upgrade) = upgrade else {
        // reachable by anyone who opens the URL in a browser, so say what the endpoint is instead of a bare 400
        return not_a_websocket(format!(
            "This endpoint is a WebSocket. Servers announce themselves here; the launcher reads {}.",
            master::LIST_PATH
        ));
    };
    let address = registry::observed_address(Some(remote_ip(&s.config, peer, &headers)));
    upgrade
        .max_message_size(master::MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| relay(socket, address, s))
}

async fn send_master(socket: &mut WebSocket, m: &MasterMessage) {
    let _ = socket.send(Message::text(serde_json::to_string(m).unwrap_or_default())).await;
}

/// Sent as a message before the close frame: a close reason is capped at 123 bytes and is awkward to read from a client library, and the relay
/// logs this verbatim for its operator.
async fn reject(socket: &mut WebSocket, code: &str, reason: &str) {
    send_master(
        socket,
        &MasterMessage {
            kind: master::message_type::REJECTED.into(),
            api_version: master::VERSION,
            rejected: Some(MasterRejected {
                code: code.into(),
                reason: reason.into(),
                api_version: master::VERSION,
            }),
            ..Default::default()
        },
    )
    .await;
    close(socket, close_code::POLICY, code).await;
}

async fn close(socket: &mut WebSocket, code: u16, why: &str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: why.to_string().into(),
        })))
        .await;
}

async fn relay(mut socket: WebSocket, address: String, s: State) {
    let mut listed: Option<(String, String)> = None;
    relay_session(&mut socket, &address, &s, &mut listed).await;
    if let Some((id, name)) = listed {
        if s.registry.remove(&id) {
            log::info(&format!("Delisted \"{name}\" ({address})."));
        }
    }
}

async fn relay_session(socket: &mut WebSocket, address: &str, s: &State, listed: &mut Option<(String, String)>) {
    let announce = match receive::<MasterMessage>(socket, FIRST_MESSAGE_TIMEOUT, master::MAX_MESSAGE_BYTES).await {
        Received::Message(m) => m,
        Received::Closed => return,
        other => return end(socket, address, other).await,
    };
    if announce.api_version != master::VERSION {
        // the one mismatch worth being loud about: a relay built against a different contract may otherwise appear to work
        log::warn(&format!(
            "Refused a server from {address}: it speaks master API v{}, this master speaks v{}.",
            announce.api_version,
            master::VERSION
        ));
        let why = format!(
            "This master server speaks master API v{}; the server sent v{}. Update whichever is older.",
            master::VERSION,
            announce.api_version
        );
        return reject(socket, master::reject_code::API_VERSION, &why).await;
    }
    let Some(body) = announce
        .announce
        .as_ref()
        .filter(|_| announce.kind == master::message_type::ANNOUNCE)
    else {
        let why = format!("The first message on this socket must be \"{}\".", master::message_type::ANNOUNCE);
        return reject(socket, master::reject_code::INVALID, &why).await;
    };
    let id = format!("{:032x}", rand::random::<u128>());
    let result = registry::build(body, address, &id, now_secs());
    let Some(server) = result.server else {
        let error = result.error.unwrap_or_else(|| "The announce was rejected.".into());
        log::warn(&format!("Refused a server from {address}: {error}"));
        return reject(socket, master::reject_code::INVALID, &error).await;
    };
    if !s.registry.add(server.clone()) {
        return reject(
            socket,
            master::reject_code::FULL,
            "This master server is already listing as many servers as it is configured to hold.",
        )
        .await;
    }
    *listed = Some((server.id.clone(), server.name.clone()));
    send_master(
        socket,
        &MasterMessage {
            kind: master::message_type::ACCEPTED.into(),
            api_version: master::VERSION,
            accepted: Some(MasterAccepted {
                id: server.id.clone(),
                address: server.address.clone(),
                heartbeat_seconds: master::HEARTBEAT_SECONDS,
                warnings: result.warnings.clone(),
            }),
            ..Default::default()
        },
    )
    .await;
    log::info(&format!(
        "Listed \"{}\" at {} (release {}, relay protocol v{}).",
        server.name,
        server.endpoint(),
        if server.release_version.is_empty() {
            "unknown"
        } else {
            &server.release_version
        },
        server.protocol_version
    ));
    for w in &result.warnings {
        log::info(&format!("  \"{}\": {w}", server.name));
    }
    pump(socket, &server.id, address, s).await;
}

/// Reads updates and heartbeats until the relay stops sending them.
async fn pump(socket: &mut WebSocket, id: &str, address: &str, s: &State) {
    let timeout = Duration::from_secs(master::TIMEOUT_SECONDS as u64);
    loop {
        let message = match receive::<MasterMessage>(socket, timeout, master::MAX_MESSAGE_BYTES).await {
            Received::Message(m) => m,
            Received::Closed => return,
            other => return end(socket, address, other).await,
        };
        let now = now_secs();
        let listed = match message.kind.as_str() {
            master::message_type::UPDATE if message.update.is_some() => s.registry.update(
                id,
                &registry::clean_update(message.update.as_ref().unwrap_or(&Default::default())),
                now,
            ),
            master::message_type::HEARTBEAT => s.registry.touch(id, now),
            // forward compatibility: a newer relay may send something this master has no use for
            _ => continue,
        };
        if listed {
            continue;
        }
        // the id is gone, so another connection claimed this address: close rather than keep reading from a relay whose listing no longer exists
        close(socket, close_code::NORMAL, "Superseded by a newer connection").await;
        return;
    }
}

/// How a connection that did not end with a close frame is told apart in a log that is mostly read when a server has vanished.
async fn end<T>(socket: &mut WebSocket, address: &str, why: Received<T>) {
    match why {
        Received::TimedOut => log::info(&format!(
            "Connection from {address} went quiet for {}s; dropping it.",
            master::TIMEOUT_SECONDS
        )),
        Received::Malformed(m) => {
            log::warn(&format!(
                "Dropping the connection from {address}: it sent something that is not a master API message ({m})."
            ));
            close(socket, close_code::INVALID, "Malformed message").await;
        }
        Received::Dropped(m) => log::info(&format!("Connection from {address} dropped: {m}")),
        Received::Message(_) | Received::Closed => {}
    }
}

// ---- presence ---------------------------------------------------------------------------------------------------------------------------

async fn presence_connect(upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>, AxState(s): AxState<State>) -> Response {
    let Ok(upgrade) = upgrade else {
        return not_a_websocket("This endpoint is a WebSocket. Launchers connect here to see their online friends.".into());
    };
    upgrade
        .max_message_size(api::MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| presence(socket, s))
}

fn refused(reason: &str) -> PresenceMessage {
    PresenceMessage {
        kind: api::message_type::REFUSED.into(),
        api_version: api::VERSION,
        reason: Some(reason.into()),
        ..Default::default()
    }
}

async fn presence(mut socket: WebSocket, s: State) {
    let hello = match receive::<PresenceMessage>(&mut socket, FIRST_MESSAGE_TIMEOUT, api::MAX_MESSAGE_BYTES).await {
        Received::Message(m) => m,
        Received::Closed => return,
        Received::Malformed(m) => return log::warn(&format!("Dropping a presence connection: not a presence message ({m}).")),
        _ => return,
    };
    let reply = |m: PresenceMessage| serde_json::to_string(&m).unwrap_or_default();
    let body = match hello.hello.as_ref() {
        Some(h) if hello.api_version == api::VERSION && hello.kind == api::message_type::HELLO => h,
        _ => {
            let why = format!("Speak presence API v{}, starting with a hello.", api::VERSION);
            let _ = socket.send(Message::text(reply(refused(&why)))).await;
            return;
        }
    };
    let (tx, mut rx) = unbounded_channel::<Outgoing>();
    let peer: Arc<Peer> = match s.hub.hello(body, tx) {
        Ok(p) => p,
        Err(why) => {
            let _ = socket.send(Message::text(reply(refused(&why)))).await;
            return;
        }
    };
    log::info(&format!("{} online ({} joined).", s.hub.online(), peer.name));
    let (mut sink, mut stream) = socket.split();
    // everything a peer is sent goes through one writer, so two messages never interleave on the socket
    let writer = tokio::spawn(async move {
        while let Some(out) = rx.recv().await {
            match out {
                Outgoing::Message(m) => {
                    if sink
                        .send(Message::text(serde_json::to_string(&m).unwrap_or_default()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Outgoing::Close => {
                    let _ = sink
                        .send(Message::Close(Some(CloseFrame {
                            code: close_code::NORMAL,
                            reason: "Signed in elsewhere".into(),
                        })))
                        .await;
                    return;
                }
            }
        }
    });
    let timeout = Duration::from_secs(api::TIMEOUT_SECONDS as u64);
    loop {
        let next = match tokio::time::timeout(timeout, stream.next()).await {
            Ok(Some(Ok(m))) => m,
            _ => break, // closed, dropped, or quiet for too long
        };
        let bytes: Vec<u8> = match next {
            Message::Text(t) => t.as_bytes().to_vec(),
            Message::Binary(b) => b.to_vec(),
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(m) = serde_json::from_slice::<PresenceMessage>(&bytes) else {
            log::warn("Dropping a presence connection: not a presence message.");
            break;
        };
        match m.kind.as_str() {
            api::message_type::STATUS if m.status.is_some() => s.hub.status(&peer, m.status.as_ref().unwrap_or(&Default::default())),
            api::message_type::INVITE if m.invite.is_some() => {
                if let Some(why) = s.hub.invite(&peer, m.invite.as_ref().unwrap_or(&Default::default()), now_millis()) {
                    let _ = peer.send_refused(&why);
                }
            }
            _ => {} // a ping, or something a newer launcher sends that this master has no use for
        }
    }
    s.hub.leave(&peer);
    log::info(&format!("{} online ({} left).", s.hub.online(), peer.name));
    drop(peer);
    writer.abort();
}
