// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The servers that are online right now, and the rules that turn an announce into a listing.
//!
//! In memory, deliberately: a listing only exists while its relay holds a WebSocket open, so nothing here outlives the process anyway. If the
//! master restarts, every relay reconnects within its heartbeat and the list rebuilds itself.

use cairn_protocol::master_api::{ServerAnnounce, ServerListing, ServerUpdate};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;

/// One listed server, as the master holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct RegisteredServer {
    pub id: String,
    pub address: String,
    pub port: i32,
    pub info_port: i32,
    pub name: String,
    pub release_version: String,
    pub protocol_version: u8,
    pub map_name: String,
    pub players: i32,
    pub max_players: i32,
    pub tags: Vec<String>,
    pub description: Option<String>,
    /// Seconds since 1970-01-01 UTC.
    pub online_since: i64,
    pub last_update: i64,
}

impl RegisteredServer {
    /// What a player actually dials, which is the only thing two registrations can genuinely collide on.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }

    pub fn to_listing(&self) -> ServerListing {
        ServerListing {
            id: self.id.clone(),
            name: self.name.clone(),
            address: self.address.clone(),
            port: self.port,
            info_port: self.info_port,
            map_name: self.map_name.clone(),
            players: self.players,
            max_players: self.max_players,
            release_version: self.release_version.clone(),
            protocol_version: self.protocol_version,
            tags: self.tags.clone(),
            description: self.description.clone(),
            online_since: iso8601(self.online_since),
        }
    }
}

/// `2026-10-23T15:57:55+00:00`, the shape the .NET server wrote for a `DateTimeOffset`.
pub fn iso8601(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}+00:00",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

pub struct Registry {
    servers: Mutex<HashMap<String, RegisteredServer>>,
    max_servers: usize,
}

impl Registry {
    pub fn new(max_servers: usize) -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
            max_servers: max_servers.max(1),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, RegisteredServer>> {
        self.servers.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn count(&self) -> usize {
        self.lock().len()
    }

    pub fn total_players(&self) -> i32 {
        self.lock().values().map(|s| s.players).sum()
    }

    /// Lists a server, replacing any earlier listing for the same address: a relay that crashes and restarts before its old socket is reaped
    /// would otherwise appear twice. The newer connection wins; the older one finds its id gone on its next message and closes itself.
    /// `false` when the master is already at its limit.
    pub fn add(&self, server: RegisteredServer) -> bool {
        let mut all = self.lock();
        let endpoint = server.endpoint();
        let stale: Vec<String> = all
            .values()
            .filter(|e| e.endpoint() == endpoint && e.id != server.id)
            .map(|e| e.id.clone())
            .collect();
        for id in stale {
            all.remove(&id);
            crate::log::info(&format!("Replaced the listing for {endpoint}; a newer connection claimed it."));
        }
        if all.len() >= self.max_servers {
            crate::log::warn(&format!(
                "Refused {endpoint}: already listing {} servers (Registry:MaxServers).",
                all.len()
            ));
            return false;
        }
        all.insert(server.id.clone(), server);
        true
    }

    /// Applies the mutable part of a listing; `false` when the id is no longer listed (a newer connection replaced this one).
    pub fn update(&self, id: &str, update: &ServerUpdate, now: i64) -> bool {
        match self.lock().get_mut(id) {
            Some(s) => {
                s.map_name = update.map_name.clone();
                s.players = update.players;
                s.max_players = update.max_players;
                s.last_update = now;
                true
            }
            None => false,
        }
    }

    pub fn touch(&self, id: &str, now: i64) -> bool {
        match self.lock().get_mut(id) {
            Some(s) => {
                s.last_update = now;
                true
            }
            None => false,
        }
    }

    /// `false` when the id was not listed: the ordinary end of a connection that had already been replaced.
    pub fn remove(&self, id: &str) -> bool {
        self.lock().remove(id).is_some()
    }

    pub fn get(&self, id: &str) -> Option<RegisteredServer> {
        self.lock().get(id).cloned()
    }

    /// A snapshot, ordered so the browser is stable between refreshes.
    pub fn all(&self) -> Vec<RegisteredServer> {
        let mut v: Vec<_> = self.lock().values().cloned().collect();
        v.sort_by(|a, b| {
            b.players
                .cmp(&a.players)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        v
    }
}

// ---- checking an announce ---------------------------------------------------------------------------------------------------------------

/// Tags a server may advertise: a fixed set, so the launcher's filter has known values and the browser cannot publish arbitrary strings.
pub const ALLOWED_TAGS: [&str; 6] = ["PvP", "PvE", "RP", "Hardcore", "Friendly", "Modded"];
pub const MAX_TAGS: usize = 3;
const MAX_NAME_LENGTH: usize = 64;
const MAX_MAP_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 500;
const MAX_VERSION_LENGTH: usize = 32;
const MAX_ADDRESS_LENGTH: usize = 253;
/// The relay addresses ghosts by a single byte and reserves none of it, so no relay can hold more than this.
const MAX_PLAYER_LIMIT: i32 = 255;

/// Exactly one of `server` and `error` is set; `warnings` are things quietly fixed up, handed back so the operator hears about them.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnounceResult {
    pub server: Option<RegisteredServer>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

fn clean(s: Option<&str>) -> String {
    s.map(|s| s.trim().to_string()).unwrap_or_default()
}

fn cut(s: String, max: usize) -> String {
    if s.chars().count() > max {
        s.chars().take(max).collect()
    } else {
        s
    }
}

fn refused(error: &str) -> AnnounceResult {
    AnnounceResult {
        server: None,
        error: Some(error.into()),
        warnings: vec![],
    }
}

/// Turns an announce into a listing, or says why it cannot. Nothing in the announce is trusted: strings are bounded, numbers clamped, and the
/// address is the one the connection came from unless the relay supplied a plain host name.
pub fn build(announce: &ServerAnnounce, observed_address: &str, id: &str, now: i64) -> AnnounceResult {
    let mut warnings = Vec::new();
    let mut name = clean(Some(&announce.name));
    if name.is_empty() {
        return refused("name is required");
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        name = cut(name, MAX_NAME_LENGTH);
        warnings.push(format!("name was truncated to {MAX_NAME_LENGTH} characters"));
    }
    if !(1..=65535).contains(&announce.port) {
        return refused("port must be between 1 and 65535");
    }
    let mut address = observed_address.to_string();
    let advertised = clean(announce.address.as_deref());
    if !advertised.is_empty() {
        if advertised.chars().count() > MAX_ADDRESS_LENGTH || advertised.chars().any(|c| c.is_whitespace() || c == '/' || c == ':') {
            warnings.push("address was ignored: it is not a plain hostname or IPv4 address".into());
        } else {
            address = advertised;
        }
    }
    if address.is_empty() {
        return refused("address could not be determined from the connection and none was supplied");
    }
    let mut info_port = announce.info_port;
    if !(0..=65535).contains(&info_port) {
        warnings.push("infoPort was ignored: it is not a valid port".into());
        info_port = 0;
    }
    let mut map_name = clean(Some(&announce.map_name));
    if map_name.chars().count() > MAX_MAP_NAME_LENGTH {
        map_name = cut(map_name, MAX_MAP_NAME_LENGTH);
        warnings.push(format!("mapName was truncated to {MAX_MAP_NAME_LENGTH} characters"));
    }
    let max_players = announce.max_players.clamp(0, MAX_PLAYER_LIMIT);
    if max_players != announce.max_players {
        warnings.push(format!(
            "maxPlayers was clamped to {max_players}; a relay cannot address more than {MAX_PLAYER_LIMIT} players"
        ));
    }
    let players = announce.players.clamp(0, max_players);
    let mut description = clean(announce.description.as_deref());
    if description.chars().count() > MAX_DESCRIPTION_LENGTH {
        description = cut(description, MAX_DESCRIPTION_LENGTH);
        warnings.push(format!("description was truncated to {MAX_DESCRIPTION_LENGTH} characters"));
    }
    let release_version = cut(clean(Some(&announce.release_version)), MAX_VERSION_LENGTH);
    let tags = select_tags(&announce.tags, &mut warnings);
    AnnounceResult {
        server: Some(RegisteredServer {
            id: id.to_string(),
            address,
            port: announce.port,
            info_port,
            name,
            release_version,
            protocol_version: announce.protocol_version,
            map_name,
            players,
            max_players,
            tags,
            description: (!description.is_empty()).then_some(description),
            online_since: now,
            last_update: now,
        }),
        error: None,
        warnings,
    }
}

/// Bounds an update the same way [`build`] bounds the announce it amends.
pub fn clean_update(update: &ServerUpdate) -> ServerUpdate {
    let max_players = update.max_players.clamp(0, MAX_PLAYER_LIMIT);
    ServerUpdate {
        map_name: cut(clean(Some(&update.map_name)), MAX_MAP_NAME_LENGTH),
        max_players,
        players: update.players.clamp(0, max_players),
    }
}

/// The address a listing is published under. A relay on the same machine arrives as `::ffff:127.0.0.1`, which nothing downstream would connect
/// to, so it is folded back to its IPv4 form.
pub fn observed_address(remote: Option<IpAddr>) -> String {
    remote.map(|ip| ip.to_canonical().to_string()).unwrap_or_default()
}

fn select_tags(tags: &[String], warnings: &mut Vec<String>) -> Vec<String> {
    let mut selected: Vec<String> = Vec::new();
    for raw in tags {
        let raw = raw.trim();
        let Some(m) = ALLOWED_TAGS.iter().find(|t| t.eq_ignore_ascii_case(raw)) else {
            warnings.push(format!("tag \"{raw}\" was dropped; allowed tags are {}", ALLOWED_TAGS.join(", ")));
            continue;
        };
        if selected.iter().any(|s| s == m) {
            continue;
        }
        if selected.len() == MAX_TAGS {
            warnings.push(format!("tag \"{m}\" was dropped; at most {MAX_TAGS} tags are listed"));
            continue;
        }
        selected.push((*m).to_string());
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn announce() -> ServerAnnounce {
        ServerAnnounce {
            name: "  Pub  ".into(),
            port: 7778,
            info_port: 7779,
            players: 9,
            max_players: 4,
            tags: vec![
                "pvp".into(),
                "PVE".into(),
                "rp".into(),
                "hardcore".into(),
                "bogus".into(),
                "pvp".into(),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn an_announce_becomes_a_bounded_listing() {
        let r = build(&announce(), "203.0.113.7", "id1", 100);
        let s = r.server.unwrap();
        assert_eq!(
            (s.name.as_str(), s.address.as_str(), s.port, s.players, s.max_players),
            ("Pub", "203.0.113.7", 7778, 4, 4)
        );
        assert_eq!(s.tags, vec!["PvP", "PvE", "RP"]);
        assert!(r.warnings.iter().any(|w| w.contains("bogus")) && r.warnings.iter().any(|w| w.contains("Hardcore")));
        assert_eq!(s.endpoint(), "203.0.113.7:7778");
    }

    #[test]
    fn what_cannot_be_listed_is_refused_and_what_can_be_fixed_is() {
        assert_eq!(
            build(
                &ServerAnnounce {
                    port: 1,
                    ..Default::default()
                },
                "1.2.3.4",
                "i",
                0
            )
            .error
            .as_deref(),
            Some("name is required")
        );
        let bad_port = ServerAnnounce {
            name: "x".into(),
            port: 70000,
            ..Default::default()
        };
        assert!(build(&bad_port, "1.2.3.4", "i", 0).error.unwrap().contains("port"));
        let none = ServerAnnounce {
            name: "x".into(),
            port: 5,
            ..Default::default()
        };
        assert!(build(&none, "", "i", 0).error.unwrap().contains("address"));
        let adv = ServerAnnounce {
            name: "x".into(),
            port: 5,
            address: Some("play.example.net".into()),
            info_port: -4,
            ..Default::default()
        };
        let s = build(&adv, "9.9.9.9", "i", 0);
        assert_eq!(s.server.as_ref().unwrap().address, "play.example.net");
        assert_eq!(s.server.unwrap().info_port, 0);
        let url = ServerAnnounce {
            name: "x".into(),
            port: 5,
            address: Some("http://evil/".into()),
            ..Default::default()
        };
        let s = build(&url, "9.9.9.9", "i", 0);
        assert_eq!(s.server.unwrap().address, "9.9.9.9");
        assert!(s.warnings.iter().any(|w| w.contains("address was ignored")));
    }

    #[test]
    fn updates_are_clamped() {
        let u = clean_update(&ServerUpdate {
            map_name: " m ".into(),
            players: 500,
            max_players: 999,
        });
        assert_eq!((u.map_name.as_str(), u.players, u.max_players), ("m", 255, 255));
    }

    #[test]
    fn a_local_relay_is_listed_by_its_ipv4_address() {
        assert_eq!(observed_address(Some("::ffff:127.0.0.1".parse().unwrap())), "127.0.0.1");
        assert_eq!(observed_address(Some("2001:db8::1".parse().unwrap())), "2001:db8::1");
        assert_eq!(observed_address(None), "");
    }

    fn server(id: &str, port: i32, players: i32, name: &str) -> RegisteredServer {
        build(
            &ServerAnnounce {
                name: name.into(),
                port,
                players,
                max_players: 10,
                ..Default::default()
            },
            "1.1.1.1",
            id,
            5,
        )
        .server
        .unwrap()
    }

    #[test]
    fn the_newer_connection_for_an_address_wins_and_the_limit_holds() {
        let r = Registry::new(2);
        assert!(r.add(server("a", 1, 0, "A")));
        assert!(r.add(server("b", 1, 3, "B"))); // same endpoint: replaces a
        assert!(r.get("a").is_none() && r.get("b").is_some());
        assert!(!r.update("a", &ServerUpdate::default(), 9)); // the old connection finds its id gone
        assert!(r.add(server("c", 2, 7, "c")));
        assert!(!r.add(server("d", 3, 0, "D"))); // full
        assert_eq!((r.count(), r.total_players()), (2, 10));
        assert_eq!(r.all().iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["c", "b"]); // most players first
        assert!(r.update(
            "c",
            &ServerUpdate {
                map_name: "m".into(),
                players: 1,
                max_players: 4
            },
            9
        ));
        assert_eq!(r.get("c").unwrap().players, 1);
        assert!(r.touch("c", 11) && !r.touch("zzz", 11));
        assert!(r.remove("c") && !r.remove("c"));
    }

    #[test]
    fn times_are_written_like_the_net_server_wrote_them() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(iso8601(1_792_757_877), "2026-10-23T12:17:57+00:00");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00+00:00");
    }
}
