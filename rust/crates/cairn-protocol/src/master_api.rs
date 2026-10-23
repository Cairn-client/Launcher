// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The master server contract: what a relay says to get itself listed, and what the launcher reads back to fill the server browser.
//!
//! The field names on the wire are camelCase, as the C# side writes them (`JsonSerializerDefaults.Web`), and a field that is `None`
//! is left out.

use serde::{Deserialize, Serialize};

/// Version of the messages in this module. Bump on any change that an older peer would misread; adding a field a peer can ignore is not one.
pub const VERSION: i32 = 1;
/// Listing endpoint the launcher GETs.
pub const LIST_PATH: &str = "/api/v1/servers";
/// WebSocket endpoint a relay holds open for as long as it is up.
pub const CONNECT_PATH: &str = "/api/v1/servers/connect";
/// Status endpoint, for a quick "is the master alive" check.
pub const STATUS_PATH: &str = "/api/v1/status";
/// How often a relay sends something: an update if anything changed, a bare heartbeat otherwise.
pub const HEARTBEAT_SECONDS: i32 = 10;
/// How long the master waits for that before treating the connection as dead.
pub const TIMEOUT_SECONDS: i32 = 30;
/// Largest message either side will read, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// `MasterMessage.type` values.
pub mod message_type {
    pub const ANNOUNCE: &str = "announce";
    pub const UPDATE: &str = "update";
    pub const HEARTBEAT: &str = "heartbeat";
    pub const ACCEPTED: &str = "accepted";
    pub const REJECTED: &str = "rejected";
}

/// `MasterRejected.code` values.
pub mod reject_code {
    pub const API_VERSION: &str = "api_version";
    pub const INVALID: &str = "invalid";
    pub const FULL: &str = "full";
}

/// Envelope for everything sent over the relay-to-master WebSocket: one type with optional payloads.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MasterMessage {
    #[serde(rename = "type")]
    pub kind: String,
    pub api_version: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub announce: Option<ServerAnnounce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<ServerUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<MasterAccepted>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected: Option<MasterRejected>,
}

/// Everything the master needs to list a relay. Sent once, when the socket opens.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerAnnounce {
    pub name: String,
    pub release_version: String,
    pub protocol_version: u8,
    /// Address to publish when the relay knows better than the master does (a hostname, or a public address behind a proxy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub port: i32,
    pub info_port: i32,
    pub map_name: String,
    pub players: i32,
    pub max_players: i32,
    /// At most three, from the master's fixed list.
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The parts of an announce that change while a relay runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerUpdate {
    pub map_name: String,
    pub players: i32,
    pub max_players: i32,
}

/// Master's answer to an accepted announce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MasterAccepted {
    pub id: String,
    pub address: String,
    pub heartbeat_seconds: i32,
    pub warnings: Vec<String>,
}

impl Default for MasterAccepted {
    fn default() -> Self {
        Self {
            id: String::new(),
            address: String::new(),
            heartbeat_seconds: HEARTBEAT_SECONDS,
            warnings: Vec::new(),
        }
    }
}

/// Master's answer to a refused announce. The socket closes after it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MasterRejected {
    pub code: String,
    pub reason: String,
    pub api_version: i32,
}

impl Default for MasterRejected {
    fn default() -> Self {
        Self {
            code: String::new(),
            reason: String::new(),
            api_version: VERSION,
        }
    }
}

/// One server in the browser. Response element of GET [`LIST_PATH`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerListing {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: i32,
    pub info_port: i32,
    pub map_name: String,
    pub players: i32,
    pub max_players: i32,
    pub release_version: String,
    pub protocol_version: u8,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When the relay's connection to the master was opened (an ISO 8601 timestamp, as the C# side writes it).
    pub online_since: String,
}

/// Response of GET [`LIST_PATH`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerListResponse {
    pub api_version: i32,
    pub servers: Vec<ServerListing>,
}

impl Default for ServerListResponse {
    fn default() -> Self {
        Self {
            api_version: VERSION,
            servers: Vec::new(),
        }
    }
}

/// Response of GET [`STATUS_PATH`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MasterStatus {
    pub api_version: i32,
    pub servers: i32,
    pub players: i32,
}

impl Default for MasterStatus {
    fn default() -> Self {
        Self {
            api_version: VERSION,
            servers: 0,
            players: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_announce_is_written_camel_case_and_leaves_out_what_is_missing() {
        let m = MasterMessage {
            kind: message_type::ANNOUNCE.into(),
            api_version: VERSION,
            announce: Some(ServerAnnounce {
                name: "Pub".into(),
                port: 7777,
                tags: vec!["pvp".into()],
                ..Default::default()
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"type\":\"announce\"") && json.contains("\"apiVersion\":1") && json.contains("\"infoPort\":0"));
        assert!(!json.contains("update") && !json.contains("accepted") && !json.contains("address") && !json.contains("description"));
        assert_eq!(serde_json::from_str::<MasterMessage>(&json).unwrap(), m);
    }

    #[test]
    fn what_the_csharp_master_writes_is_read() {
        let json = r#"{"apiVersion":1,"servers":[{"id":"a1","name":"X","address":"1.2.3.4","port":5,"infoPort":6,"mapName":"m","players":1,"maxPlayers":4,"releaseVersion":"0.3.0","protocolVersion":2,"tags":["a"],"onlineSince":"2026-10-23T10:00:00+00:00"}]}"#;
        let r: ServerListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.servers[0].address, "1.2.3.4");
        assert_eq!(r.servers[0].protocol_version, 2);
        assert_eq!(r.servers[0].description, None);
    }

    #[test]
    fn a_missing_field_takes_its_default() {
        let a: MasterAccepted = serde_json::from_str(r#"{"id":"x"}"#).unwrap();
        assert_eq!(a.heartbeat_seconds, HEARTBEAT_SECONDS);
        let r: MasterRejected = serde_json::from_str(r#"{"code":"full"}"#).unwrap();
        assert_eq!(r.api_version, VERSION);
    }
}
