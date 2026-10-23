// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Steam as the launcher sees it: who is signed in, the friends, and what each friend is doing (from their public profile page).

use cairn_setup::http::Http;
use cairn_setup::setup_host::{RealSetupHost, SteamInstall};
use cairn_setup::steam_identity::{self, Local, Named};
use cairn_setup::steam_profile_status::{self as profile, ProfileInfo, Status};
use std::path::PathBuf;

/// What is known about Steam on this PC.
#[derive(Debug, Clone, Default)]
pub struct SteamSelf {
    /// Steam is installed.
    pub installed: bool,
    pub root: Option<PathBuf>,
    /// The signed-in account, with its friends; `None` when nobody is signed in.
    pub local: Option<Local>,
}

impl SteamSelf {
    pub fn name(&self) -> &str {
        self.local.as_ref().map(|l| l.name.as_str()).unwrap_or("")
    }
    pub fn id(&self) -> &str {
        self.local.as_ref().map(|l| l.id.as_str()).unwrap_or("")
    }
    pub fn avatar(&self) -> &str {
        self.local.as_ref().map(|l| l.avatar.as_str()).unwrap_or("")
    }
}

/// Reads Steam's own files (the registry, `loginusers.vdf`, `localconfig.vdf`).
pub fn read_self() -> SteamSelf {
    let steam = SteamInstall::locate(&RealSetupHost, None);
    let local = steam_identity::read(steam.root.as_deref());
    SteamSelf {
        installed: steam.found(),
        root: steam.root,
        local,
    }
}

/// What a friend is doing now, from the public profile page; "unknown" when it cannot be read (private profile, offline).
pub fn friend_status(http: &dyn Http, steam_id: &str) -> Status {
    let Ok(resp) = http.get(&profile::profile_url(steam_id)) else {
        return Status::none();
    };
    if !resp.is_success() {
        return Status::none();
    }
    resp.text(1 << 20).map(|t| profile::parse(&t)).unwrap_or_else(|_| Status::none())
}

/// The signed-in player's own profile card.
pub fn own_profile(http: &dyn Http, steam_id: &str) -> Option<ProfileInfo> {
    let resp = http.get(&profile::profile_url(steam_id)).ok().filter(|r| r.is_success())?;
    profile::parse_info(&resp.text(1 << 20).ok()?)
}

/// The avatar picture address for a Steam avatar hash.
pub fn avatar_url(hash: &str, size: &str) -> String {
    if hash.is_empty() {
        String::new()
    } else {
        format!("https://avatars.steamstatic.com/{hash}_{size}.jpg")
    }
}

/// The first letter to show when a person has no picture.
pub fn initial(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into())
}

/// One friend with what is known of what they do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendRow {
    pub id: String,
    pub name: String,
    /// `online`, `in-game`, `offline`, `unknown`, `loading`.
    pub state: String,
    pub game: String,
    pub message: String,
    pub avatar: String,
}

impl FriendRow {
    pub fn from_named(n: &Named, status: Option<&Status>) -> Self {
        match status {
            Some(s) => Self {
                id: n.id.clone(),
                name: n.name.clone(),
                state: s.state.clone(),
                game: s.game.clone(),
                message: s.message.clone(),
                avatar: n.avatar.clone(),
            },
            None => Self {
                id: n.id.clone(),
                name: n.name.clone(),
                state: "loading".into(),
                game: String::new(),
                message: String::new(),
                avatar: n.avatar.clone(),
            },
        }
    }

    /// The line under the name.
    pub fn line(&self) -> String {
        match self.state.as_str() {
            "in-game" => self.game.clone(),
            "online" => {
                if self.message.is_empty() {
                    "Online".into()
                } else {
                    self.message.clone()
                }
            }
            "loading" => "Checking...".into(),
            "hidden" => String::new(),
            "offline" => self
                .message
                .strip_prefix("Last ")
                .filter(|_| self.message.to_lowercase().starts_with("last online"))
                .map(str::to_string)
                .unwrap_or_else(|| "Offline".into()),
            _ => "Private profile".into(),
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self.state.as_str(), "online" | "in-game")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_setup::http::fake::FakeHttp;

    #[test]
    fn a_friends_line_says_what_they_do() {
        let n = Named {
            id: "7656119800000000".into(),
            name: "Ann".into(),
            avatar: "abc".into(),
        };
        let row = |state: &str, game: &str, message: &str| FriendRow {
            state: state.into(),
            game: game.into(),
            message: message.into(),
            ..FriendRow::from_named(&n, None)
        };
        assert_eq!(FriendRow::from_named(&n, None).line(), "Checking...");
        assert_eq!(row("in-game", "Noita", "In-Game").line(), "Noita");
        assert_eq!(row("online", "", "").line(), "Online");
        assert_eq!(row("online", "", "Busy").line(), "Busy");
        assert_eq!(row("offline", "", "Last Online 2 hrs ago").line(), "Online 2 hrs ago");
        assert_eq!(row("offline", "", "").line(), "Offline");
        assert_eq!(row("unknown", "", "").line(), "Private profile");
        assert!(row("in-game", "x", "").is_online() && !row("offline", "", "").is_online());
        assert_eq!((initial("ann"), initial("")), ("A".to_string(), "?".to_string()));
        assert_eq!(avatar_url("abc", "full"), "https://avatars.steamstatic.com/abc_full.jpg");
        assert_eq!(avatar_url("", "full"), "");
    }

    #[test]
    fn a_friends_status_is_read_from_the_public_profile() {
        let url = profile::profile_url("7656119800000000");
        let h = FakeHttp::serving(&url, b"<profile><onlineState>in-game</onlineState><stateMessage>In-Game</stateMessage><inGameInfo><gameName>Noita</gameName></inGameInfo></profile>");
        let s = friend_status(&h, "7656119800000000");
        assert_eq!((s.state.as_str(), s.game.as_str()), ("in-game", "Noita"));
        assert_eq!(friend_status(&FakeHttp::default(), "7656119800000000").state, "unknown");
        assert!(own_profile(&FakeHttp::default(), "1").is_none());
    }
}
