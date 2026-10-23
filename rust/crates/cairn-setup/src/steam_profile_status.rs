// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! What a Steam friend is doing, from their public profile (`steamcommunity.com/profiles/<id>/?xml=1`), and the signed-in player's own
//! profile card. The parsing only; the launcher fetches the XML.

use roxmltree::{Document, Node};

pub const ONLINE: &str = "online";
pub const IN_GAME: &str = "in-game";
pub const OFFLINE: &str = "offline";
pub const UNKNOWN: &str = "unknown";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub state: String,
    pub game: String,
    pub message: String,
}

impl Status {
    pub fn none() -> Self {
        Self {
            state: UNKNOWN.into(),
            game: String::new(),
            message: String::new(),
        }
    }
}

/// The URL of the public profile in its XML form.
pub fn profile_url(steam_id: &str) -> String {
    format!("https://steamcommunity.com/profiles/{steam_id}/?xml=1")
}

pub fn parse(xml: &str) -> Status {
    let Ok(doc) = Document::parse(xml) else { return Status::none() };
    let p = doc.root_element();
    if p.tag_name().name() != "profile" {
        return Status::none();
    }
    let state = child_text(p, "onlineState");
    if ![ONLINE, IN_GAME, OFFLINE].contains(&state.as_str()) {
        return Status::none();
    }
    let game = p
        .children()
        .find(|c| c.has_tag_name("inGameInfo"))
        .map(|g| child_text(g, "gameName"))
        .unwrap_or_default();
    let message = plain(&child_text(p, "stateMessage"));
    let state = if state == IN_GAME && game.is_empty() {
        ONLINE.to_string()
    } else {
        state
    };
    Status { state, game, message }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayedGame {
    pub name: String,
    pub icon: String,
    pub hours_2_weeks: f64,
    pub hours_total: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileInfo {
    pub avatar: String,
    pub state: String,
    pub state_message: String,
    pub real_name: String,
    pub location: String,
    pub summary: String,
    /// (year, month, day) of "memberSince", when it reads as a date.
    pub member_since: Option<(i32, u32, u32)>,
    pub most_played: Vec<PlayedGame>,
}

/// `None` when the XML is not a profile (a private or missing one answers with an `<error>`).
pub fn parse_info(xml: &str) -> Option<ProfileInfo> {
    let doc = Document::parse(xml).ok()?;
    let p = doc.root_element();
    if p.tag_name().name() != "profile" || !p.children().any(|c| c.has_tag_name("steamID64")) {
        return None;
    }
    let s = |name: &str| child_text(p, name);
    let mut games = Vec::new();
    if let Some(list) = p.children().find(|c| c.has_tag_name("mostPlayedGames")) {
        for g in list.children().filter(|c| c.has_tag_name("mostPlayedGame")) {
            let name = child_text(g, "gameName");
            if name.is_empty() {
                continue;
            }
            let hours = |e: &str| child_text(g, e).replace(',', "").parse::<f64>().unwrap_or(0.0);
            games.push(PlayedGame {
                name,
                icon: child_text(g, "gameIcon"),
                hours_2_weeks: hours("hoursPlayed"),
                hours_total: hours("hoursOnRecord"),
            });
        }
    }
    Some(ProfileInfo {
        avatar: s("avatarFull"),
        state: s("onlineState"),
        state_message: plain(&s("stateMessage")),
        real_name: s("realname"),
        location: s("location"),
        summary: strip_tags(&s("summary")),
        member_since: parse_date(&s("memberSince")),
        most_played: games,
    })
}

fn child_text(parent: Node, name: &str) -> String {
    parent
        .children()
        .find(|c| c.has_tag_name(name))
        .map(|c| c.text().unwrap_or("").trim().to_string())
        .unwrap_or_default()
}

/// The text before the first `<` (Steam writes "In-Game<br/>Noita" in the message).
fn plain(s: &str) -> String {
    s.split('<').next().unwrap_or("").trim().to_string()
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in s.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            c if !tag => out.push(c),
            _ => {}
        }
    }
    html_decode(&out).trim().to_string()
}

fn html_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// "May 30, 2017", "30 May 2017" or "2017-05-30".
fn parse_date(s: &str) -> Option<(i32, u32, u32)> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let month = |w: &str| {
        let w = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        (w.len() >= 3)
            .then(|| MONTHS.iter().position(|m| w.starts_with(m)).map(|i| i as u32 + 1))
            .flatten()
    };
    let s = s.trim();
    if let Some((y, rest)) = s.split_once('-') {
        let (m, d) = rest.split_once('-')?;
        return Some((y.parse().ok()?, m.parse().ok()?, d.get(..2).unwrap_or(d).parse().ok()?));
    }
    let words: Vec<&str> = s.split([' ', ',']).filter(|w| !w.is_empty()).collect();
    let num = |w: &str| w.trim_matches(|c: char| !c.is_ascii_digit()).parse::<i32>().ok();
    match words.as_slice() {
        [a, b, c] if month(a).is_some() => Some((num(c)?, month(a)?, num(b)? as u32)),
        [a, b, c] if month(b).is_some() => Some((num(c)?, month(b)?, num(a)? as u32)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(xml: &str) -> (String, String, String) {
        let s = parse(xml);
        (s.state, s.game, s.message)
    }

    #[test]
    fn a_public_profile_says_what_the_friend_is_doing() {
        let t = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
        assert_eq!(
            status("<profile><onlineState>in-game</onlineState><stateMessage><![CDATA[In-Game<br/>Noita]]></stateMessage><inGameInfo><gameName>Noita</gameName></inGameInfo></profile>"),
            t("in-game", "Noita", "In-Game")
        );
        assert_eq!(
            status("<profile><onlineState>online</onlineState><stateMessage>Online</stateMessage></profile>"),
            t("online", "", "Online")
        );
        assert_eq!(
            status("<profile><onlineState>offline</onlineState><stateMessage><![CDATA[Last Online 2 hrs, 5 mins ago]]></stateMessage></profile>"),
            t("offline", "", "Last Online 2 hrs, 5 mins ago")
        );
        assert_eq!(
            status("<response><error>The specified profile could not be found.</error></response>"),
            t("unknown", "", "")
        );
        assert_eq!(status("not xml"), t("unknown", "", ""));
        // "in game" with no game name is only online
        assert_eq!(status("<profile><onlineState>in-game</onlineState></profile>").0, "online");
    }

    #[test]
    fn an_own_profile_gives_the_status_names_and_most_played_games() {
        let i = parse_info(
            "<profile><steamID64>76561198000000001</steamID64><avatarFull>a.jpg</avatarFull><onlineState>online</onlineState><stateMessage><![CDATA[Online]]></stateMessage><realname>Ann</realname><location>Lyon</location><memberSince>May 30, 2017</memberSince><summary><![CDATA[Hi <b>there</b> &amp; you]]></summary><mostPlayedGames><mostPlayedGame><gameName>Noita</gameName><gameIcon>n.jpg</gameIcon><hoursPlayed>1.5</hoursPlayed><hoursOnRecord>1,240</hoursOnRecord></mostPlayedGame></mostPlayedGames></profile>",
        )
        .unwrap();
        assert_eq!(
            (
                i.avatar.as_str(),
                i.state.as_str(),
                i.real_name.as_str(),
                i.location.as_str(),
                i.summary.as_str()
            ),
            ("a.jpg", "online", "Ann", "Lyon", "Hi there & you")
        );
        assert_eq!(i.member_since, Some((2017, 5, 30)));
        assert_eq!(
            (
                i.most_played[0].name.as_str(),
                i.most_played[0].hours_2_weeks,
                i.most_played[0].hours_total
            ),
            ("Noita", 1.5, 1240.0)
        );
        assert!(parse_info("<response><error>no</error></response>").is_none());
    }

    #[test]
    fn dates_read_in_the_ways_steam_writes_them() {
        assert_eq!(parse_date("May 30, 2017"), Some((2017, 5, 30)));
        assert_eq!(parse_date("30 May 2017"), Some((2017, 5, 30)));
        assert_eq!(parse_date("2017-05-30"), Some((2017, 5, 30)));
        assert_eq!(parse_date("soon"), None);
    }

    #[test]
    fn the_url_names_the_profile() {
        assert_eq!(
            profile_url("76561198000000001"),
            "https://steamcommunity.com/profiles/76561198000000001/?xml=1"
        );
    }
}
