// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The friend presence contract: the launcher keeps one WebSocket to the master, says what it is doing and learns what the Steam friends who
//! also run Cairn are doing. Field names are camelCase on the wire and a `None` is left out, as the C# side writes them.

use serde::{Deserialize, Serialize};

pub const VERSION: i32 = 1;
pub const CONNECT_PATH: &str = "/api/v1/presence/connect";
/// How often the launcher pings, and how long the master waits for anything before it drops the connection.
pub const PING_SECONDS: i32 = 20;
pub const TIMEOUT_SECONDS: i32 = 70;
pub const MAX_FRIENDS: usize = 2000;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_NAME_LENGTH: usize = 48;
pub const MAX_GAME_LENGTH: usize = 64;
pub const MAX_CODE_LENGTH: usize = 256;
pub const INVITES_PER_MINUTE: u32 = 6;

pub mod message_type {
    pub const HELLO: &str = "hello";
    pub const STATUS: &str = "status";
    pub const INVITE: &str = "invite";
    pub const PING: &str = "ping";
    pub const WELCOME: &str = "welcome";
    pub const FRIEND: &str = "friend";
    pub const INVITED: &str = "invited";
    pub const REFUSED: &str = "refused";
}

pub mod activity {
    pub const ONLINE: &str = "online";
    pub const PLAYING: &str = "playing";
    pub const HOSTING: &str = "hosting";

    pub fn valid(a: Option<&str>) -> bool {
        matches!(a, Some(ONLINE | PLAYING | HOSTING))
    }
}

/// A SteamID64 of an individual account: 17 digits starting 7656119.
pub fn valid_id(id: Option<&str>) -> bool {
    matches!(id, Some(s) if s.len() == 17 && s.starts_with("7656119") && s.bytes().all(|b| b.is_ascii_digit()))
}

/// Text from a stranger: control characters removed, cut to `max` characters, trimmed.
pub fn clean_text(s: Option<&str>, max: usize) -> String {
    let Some(s) = s else { return String::new() };
    if s.trim().is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut n = 0;
    for c in s.chars() {
        if c.is_control() {
            continue;
        }
        out.push(c);
        n += c.len_utf16(); // .NET counts UTF-16 units
        if n >= max {
            break;
        }
    }
    out.trim().to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresenceMessage {
    #[serde(rename = "type")]
    pub kind: String,
    pub api_version: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hello: Option<PresenceHello>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PresenceStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite: Option<PresenceInvite>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friend: Option<PresenceFriend>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invited: Option<PresenceInvited>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresenceHello {
    pub id: String,
    pub name: String,
    pub friends: Vec<String>,
    pub status: PresenceStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresenceStatus {
    pub activity: String,
    pub game: String,
}

impl Default for PresenceStatus {
    fn default() -> Self {
        Self {
            activity: activity::ONLINE.into(),
            game: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresenceInvite {
    pub to: String,
    pub game: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresenceFriend {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub activity: String,
    pub game: String,
}

impl Default for PresenceFriend {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            online: false,
            activity: activity::ONLINE.into(),
            game: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresenceInvited {
    pub from: String,
    pub name: String,
    pub game: String,
    pub code: String,
}

/// The one-line notices the launcher shows for presence changes.
pub mod notices {
    use super::*;

    /// The notice for a friend's change, or `None` when it is not worth one.
    pub fn for_friend(before: Option<&PresenceFriend>, now: &PresenceFriend) -> Option<String> {
        if !now.online {
            return None;
        }
        let active = (now.activity == activity::PLAYING || now.activity == activity::HOSTING) && !now.game.is_empty();
        let verb = if now.activity == activity::HOSTING { "hosting" } else { "playing" };
        match before {
            Some(b) if b.online => {
                let changed = active && (b.game != now.game || b.activity != now.activity);
                changed.then(|| format!("{} is {} {}", now.name, verb, now.game))
            }
            _ => Some(if active {
                format!("{} is online and {} {}", now.name, verb, now.game)
            } else {
                format!("{} is online", now.name)
            }),
        }
    }

    pub fn for_invite(i: &PresenceInvited) -> String {
        if i.game.is_empty() {
            format!("{} is inviting you to play", i.name)
        } else {
            format!("{} is inviting you to play {}", i.name, i.game)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn friend(online: bool, act: &str, game: &str) -> PresenceFriend {
        PresenceFriend {
            id: "7656119000000000".into(),
            name: "Ana".into(),
            online,
            activity: act.into(),
            game: game.into(),
        }
    }

    #[test]
    fn only_a_steam_id_of_an_individual_is_valid() {
        assert!(valid_id(Some("76561198000000000")));
        assert!(!valid_id(Some("7656119800000000"))); // 16 digits
        assert!(!valid_id(Some("76561198000000000 ")));
        assert!(!valid_id(Some("76561188000000000"))); // not an individual
        assert!(!valid_id(Some("7656119800000000a")));
        assert!(!valid_id(None));
    }

    #[test]
    fn text_from_a_stranger_loses_control_characters_and_is_cut() {
        assert_eq!(clean_text(Some("  Ana\u{0007}\n "), 48), "Ana");
        assert_eq!(clean_text(Some("abcdef"), 3), "abc");
        assert_eq!(clean_text(Some("   "), 3), "");
        assert_eq!(clean_text(None, 3), "");
    }

    #[test]
    fn only_the_three_activities_are_valid() {
        assert!(activity::valid(Some("hosting")) && activity::valid(Some("online")) && activity::valid(Some("playing")));
        assert!(!activity::valid(Some("away")) && !activity::valid(None));
    }

    #[test]
    fn notices_say_what_changed() {
        assert_eq!(notices::for_friend(None, &friend(false, "online", "")), None);
        assert_eq!(
            notices::for_friend(None, &friend(true, "online", "")).as_deref(),
            Some("Ana is online")
        );
        assert_eq!(
            notices::for_friend(None, &friend(true, "hosting", "Noita")).as_deref(),
            Some("Ana is online and hosting Noita")
        );
        let was = friend(true, "online", "");
        assert_eq!(
            notices::for_friend(Some(&was), &friend(true, "playing", "Noita")).as_deref(),
            Some("Ana is playing Noita")
        );
        assert_eq!(notices::for_friend(Some(&was), &friend(true, "online", "")), None);
        let playing = friend(true, "playing", "Noita");
        assert_eq!(notices::for_friend(Some(&playing), &playing), None);
        assert_eq!(
            notices::for_friend(Some(&friend(false, "online", "")), &playing).as_deref(),
            Some("Ana is online and playing Noita")
        );
        assert_eq!(
            notices::for_invite(&PresenceInvited {
                name: "Ana".into(),
                game: "Noita".into(),
                ..Default::default()
            }),
            "Ana is inviting you to play Noita"
        );
        assert_eq!(
            notices::for_invite(&PresenceInvited {
                name: "Ana".into(),
                ..Default::default()
            }),
            "Ana is inviting you to play"
        );
    }

    #[test]
    fn a_hello_round_trips() {
        let m = PresenceMessage {
            kind: message_type::HELLO.into(),
            api_version: VERSION,
            hello: Some(PresenceHello {
                id: "x".into(),
                friends: vec!["a".into()],
                ..Default::default()
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"hello\"") && !json.contains("invite"));
        assert_eq!(serde_json::from_str::<PresenceMessage>(&json).unwrap(), m);
    }
}
