// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The server over its real sockets: the list, a relay announcing itself, and two friends seeing each other and inviting.

use cairn_master_server::config::Config;
use cairn_master_server::server::{serve, State};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use tungstenite::{connect, Message};

fn start(config: Config) -> (u16, State) {
    let state = State::new(config);
    let (tx, rx) = std::sync::mpsc::channel();
    let s = state.clone();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(listener.local_addr().unwrap().port()).unwrap();
                serve(listener, s).await;
            });
    });
    (rx.recv().unwrap(), state)
}

fn get(port: u16, path: &str) -> (u16, String) {
    let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(c, "GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
    let mut text = String::new();
    c.read_to_string(&mut text).unwrap();
    let status = text.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, text.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
}

type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

fn open(port: u16, path: &str) -> Socket {
    let (mut ws, _) = connect(format!("ws://127.0.0.1:{port}{path}")).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_mut() {
        s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    }
    ws
}

fn say(ws: &mut Socket, v: Value) {
    ws.send(Message::text(v.to_string())).unwrap();
}

fn hear(ws: &mut Socket) -> Value {
    loop {
        match ws.read().unwrap() {
            Message::Text(t) => return serde_json::from_str(t.as_str()).unwrap(),
            Message::Close(_) => return Value::Null,
            _ => {}
        }
    }
}

#[test]
fn the_list_and_the_status_answer_and_an_unknown_server_is_not_found() {
    let (port, _) = start(Config::default());
    let (code, body) = get(port, "/api/v1/servers");
    assert_eq!(code, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({ "apiVersion": 1, "servers": [] })
    );
    let (_, body) = get(port, "/api/v1/status");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({ "apiVersion": 1, "servers": 0, "players": 0 })
    );
    assert_eq!(get(port, "/api/v1/servers/0123").0, 404);
    let (code, body) = get(port, "/api/v1/servers/connect"); // a browser, not a relay
    assert_eq!(code, 400);
    assert!(body.contains("WebSocket"));
}

#[test]
fn an_announced_server_is_listed_while_its_socket_is_open_and_gone_after() {
    let (port, state) = start(Config::default());
    let mut relay = open(port, "/api/v1/servers/connect");
    say(
        &mut relay,
        json!({ "type": "announce", "apiVersion": 1, "announce": { "name": "Smoke", "releaseVersion": "0.0.0", "protocolVersion": 1,
        "port": 7778, "infoPort": 7779, "mapName": "x", "players": 1, "maxPlayers": 4, "tags": ["pvp"] } }),
    );
    let accepted = hear(&mut relay);
    assert_eq!(accepted["type"], "accepted");
    let id = accepted["accepted"]["id"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 32);
    assert_eq!(accepted["accepted"]["address"], "127.0.0.1");
    let (_, body) = get(port, "/api/v1/servers");
    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(list["servers"][0]["name"], "Smoke");
    assert_eq!(list["servers"][0]["tags"], json!(["PvP"]));
    assert!(list["servers"][0]["onlineSince"].as_str().unwrap().ends_with("+00:00"));
    assert_eq!(get(port, &format!("/api/v1/servers/{id}")).0, 200);
    // an update changes the row
    say(
        &mut relay,
        json!({ "type": "update", "apiVersion": 1, "update": { "mapName": "y", "players": 3, "maxPlayers": 4 } }),
    );
    say(&mut relay, json!({ "type": "heartbeat", "apiVersion": 1 }));
    for _ in 0..50 {
        if state.registry.total_players() == 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(state.registry.total_players(), 3);
    drop(relay); // the connection is the listing
    for _ in 0..50 {
        if state.registry.count() == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(state.registry.count(), 0);
}

#[test]
fn a_relay_of_another_api_version_or_with_a_bad_announce_is_told_why() {
    let (port, _) = start(Config::default());
    let mut a = open(port, "/api/v1/servers/connect");
    say(
        &mut a,
        json!({ "type": "announce", "apiVersion": 9, "announce": { "name": "x", "port": 1 } }),
    );
    let r = hear(&mut a);
    assert_eq!(
        (r["type"].as_str(), r["rejected"]["code"].as_str()),
        (Some("rejected"), Some("api_version"))
    );
    let mut b = open(port, "/api/v1/servers/connect");
    say(
        &mut b,
        json!({ "type": "announce", "apiVersion": 1, "announce": { "name": "", "port": 1 } }),
    );
    assert_eq!(hear(&mut b)["rejected"]["code"], "invalid");
    let mut c = open(port, "/api/v1/servers/connect");
    say(&mut c, json!({ "type": "heartbeat", "apiVersion": 1 }));
    assert_eq!(hear(&mut c)["rejected"]["code"], "invalid");
}

#[test]
fn a_full_master_refuses_another_server() {
    let (port, _) = start(Config {
        max_servers: 1,
        ..Config::default()
    });
    let announce = |name: &str, p: i32| json!({ "type": "announce", "apiVersion": 1, "announce": { "name": name, "port": p } });
    let mut a = open(port, "/api/v1/servers/connect");
    say(&mut a, announce("one", 1));
    assert_eq!(hear(&mut a)["type"], "accepted");
    let mut b = open(port, "/api/v1/servers/connect");
    say(&mut b, announce("two", 2));
    assert_eq!(hear(&mut b)["rejected"]["code"], "full");
}

#[test]
fn friends_see_each_other_and_an_invitation_arrives() {
    let (port, _) = start(Config::default());
    let (id_a, id_b) = ("76561198000000001", "76561198000000002");
    let mut a = open(port, "/api/v1/presence/connect");
    say(
        &mut a,
        json!({ "type": "hello", "apiVersion": 1, "hello": { "id": id_a, "name": "A", "friends": [id_b], "status": { "activity": "online", "game": "" } } }),
    );
    assert_eq!(hear(&mut a)["type"], "welcome");
    let mut b = open(port, "/api/v1/presence/connect");
    say(
        &mut b,
        json!({ "type": "hello", "apiVersion": 1, "hello": { "id": id_b, "name": "B", "friends": [id_a], "status": { "activity": "online", "game": "" } } }),
    );
    assert_eq!(hear(&mut b)["type"], "welcome");
    let seen = hear(&mut b);
    assert_eq!(
        (
            seen["type"].as_str(),
            seen["friend"]["id"].as_str(),
            seen["friend"]["online"].as_bool()
        ),
        (Some("friend"), Some(id_a), Some(true))
    );
    assert_eq!(hear(&mut a)["friend"]["id"], id_b);
    say(
        &mut a,
        json!({ "type": "invite", "apiVersion": 1, "invite": { "to": id_b, "game": "G", "code": "abc" } }),
    );
    let invited = hear(&mut b);
    assert_eq!(
        (
            invited["type"].as_str(),
            invited["invited"]["code"].as_str(),
            invited["invited"]["name"].as_str()
        ),
        (Some("invited"), Some("abc"), Some("A"))
    );
    // a bad invitation is refused to the inviter
    say(
        &mut a,
        json!({ "type": "invite", "apiVersion": 1, "invite": { "to": "76561198000000009", "code": "x" } }),
    );
    assert_eq!(hear(&mut a)["type"], "refused");
    // B leaving tells A
    drop(b);
    let gone = hear(&mut a);
    assert_eq!(
        (gone["friend"]["id"].as_str(), gone["friend"]["online"].as_bool()),
        (Some(id_b), Some(false))
    );
}

#[test]
fn a_hello_of_the_wrong_kind_is_refused() {
    let (port, _) = start(Config::default());
    let mut a = open(port, "/api/v1/presence/connect");
    say(&mut a, json!({ "type": "ping", "apiVersion": 1 }));
    assert_eq!(hear(&mut a)["type"], "refused");
}
