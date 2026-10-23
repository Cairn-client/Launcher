// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The "ready check" before hosting, joining or playing: everything that commonly goes wrong, in plain words, as a pure function.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

/// What a line that is not fine can do about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    InstallMod,
    PlaceMod,
    UpdateMod,
    StartSteam,
    LocateGame,
    UpdateLauncher,
}

impl Fix {
    /// The words on the fix's button.
    pub fn label(self) -> &'static str {
        match self {
            Fix::InstallMod => "DOWNLOAD THE MOD",
            Fix::PlaceMod => "PUT THE MOD IN THE GAME",
            Fix::UpdateMod => "UPDATE THE MOD",
            Fix::StartSteam => "START STEAM",
            Fix::LocateGame => "LOCATE THE GAME",
            Fix::UpdateLauncher => "UPDATE CAIRN",
        }
    }

    /// The fix for a rule of a game's own setup list, by its kind.
    pub fn for_rule(kind: &str) -> Option<Fix> {
        match kind {
            "steam-running" => Some(Fix::StartSteam),
            "game-found" => Some(Fix::LocateGame),
            "mod-installed" => Some(Fix::InstallMod),
            _ => None,
        }
    }
}

/// One line of the ready check: what was looked at, how it stands, and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightItem {
    pub title: String,
    pub status: CheckStatus,
    pub detail: String,
    pub fix: Option<Fix>,
}

fn item(title: &str, status: CheckStatus, detail: &str, fix: Option<Fix>) -> PreflightItem {
    PreflightItem {
        title: title.into(),
        status,
        detail: detail.into(),
        fix,
    }
}

/// What the launcher knows about this PC and this game when the ready check is asked for.
#[derive(Debug, Clone, Default)]
pub struct PreflightInput {
    pub supported: bool,
    pub game_found: bool,
    pub mod_installed: bool,
    pub setup_ok: bool,
    pub steam_running: bool,
    pub port_free: bool,
    pub game_running: bool,
    pub newer_launcher: Option<String>,
    pub mod_blocked: Option<String>,
}

pub fn build(i: &PreflightInput) -> Vec<PreflightItem> {
    use CheckStatus::*;
    if !i.supported {
        return vec![item(
            "Multiplayer",
            Fail,
            "Multiplayer for this game is still in development.",
            None,
        )];
    }
    let mut list = vec![if i.game_found {
        item("The game", Ok, "Found on this PC.", None)
    } else {
        item(
            "The game",
            Fail,
            "Not found. Set its path in Settings, or use Locate game on its page.",
            Some(Fix::LocateGame),
        )
    }];
    list.push(if !i.mod_installed {
        item(
            "The mod",
            Fail,
            "Not installed. Download it from the game's page.",
            Some(Fix::InstallMod),
        )
    } else if let Some(why) = &i.mod_blocked {
        item(
            "The mod",
            Warn,
            &format!("This version has a known problem: {why} Update it from the game's page when a new one is out."),
            Some(Fix::UpdateMod),
        )
    } else {
        item("The mod", Ok, "Downloaded and installed.", None)
    });
    list.push(if i.setup_ok {
        item("Setup", Ok, "Everything the checklist looks at is in place.", None)
    } else {
        item(
            "Setup",
            Warn,
            "The setup checklist has something left to do. Open it from the game's page.",
            None,
        )
    });
    list.push(if i.steam_running {
        item("Steam", Ok, "Running.", None)
    } else {
        item(
            "Steam",
            Warn,
            "Not running. Friends join through Steam, so start it first.",
            Some(Fix::StartSteam),
        )
    });
    list.push(if i.port_free {
        item("Host port", Ok, "Free on this PC.", None)
    } else {
        item(
            "Host port",
            Warn,
            "Already in use. Hosting may fail; change the port in Settings, or close what uses it.",
            None,
        )
    });
    list.push(if i.game_running {
        item(
            "The game",
            Warn,
            "It is already running. Close it before starting a new session.",
            None,
        )
    } else {
        item("Nothing running", Ok, "The game is closed, so a new session can start.", None)
    });
    list.push(match &i.newer_launcher {
        None => item("Version", Ok, "This is the newest Cairn.", None),
        Some(v) => item(
            "Version",
            Warn,
            &format!("Cairn {v} is out. Both players need the same version to play together."),
            Some(Fix::UpdateLauncher),
        ),
    });
    list
}

/// No line fails (warnings are advice, not blockers).
pub fn can_play(items: &[PreflightItem]) -> bool {
    items.iter().all(|x| x.status != CheckStatus::Fail)
}

/// The overall verdict: the worst line.
pub fn worst(items: &[PreflightItem]) -> CheckStatus {
    if items.iter().any(|x| x.status == CheckStatus::Fail) {
        CheckStatus::Fail
    } else if items.iter().any(|x| x.status == CheckStatus::Warn) {
        CheckStatus::Warn
    } else {
        CheckStatus::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> PreflightInput {
        PreflightInput {
            supported: true,
            game_found: true,
            mod_installed: true,
            setup_ok: true,
            steam_running: true,
            port_free: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_game_that_is_ready_has_nothing_to_fix() {
        let items = build(&good());
        assert!(can_play(&items) && worst(&items) == CheckStatus::Ok && items.iter().all(|i| i.fix.is_none()));
        assert_eq!(items.len(), 7);
    }

    #[test]
    fn a_game_without_multiplayer_says_so_and_stops() {
        let items = build(&PreflightInput::default());
        assert_eq!(items.len(), 1);
        assert!(!can_play(&items));
    }

    #[test]
    fn what_is_missing_fails_and_offers_its_fix() {
        let items = build(&PreflightInput {
            game_found: false,
            mod_installed: false,
            ..good()
        });
        assert!(!can_play(&items) && worst(&items) == CheckStatus::Fail);
        assert_eq!(items[0].fix, Some(Fix::LocateGame));
        assert_eq!(items[1].fix, Some(Fix::InstallMod));
        assert_eq!(Fix::LocateGame.label(), "LOCATE THE GAME");
    }

    #[test]
    fn advice_is_a_warning_and_does_not_block() {
        let items = build(&PreflightInput {
            steam_running: false,
            port_free: false,
            game_running: true,
            setup_ok: false,
            newer_launcher: Some("0.9.0".into()),
            mod_blocked: Some("Saves can be lost.".into()),
            ..good()
        });
        assert!(can_play(&items) && worst(&items) == CheckStatus::Warn);
        assert!(
            items.iter().any(|i| i.fix == Some(Fix::StartSteam))
                && items.iter().any(|i| i.fix == Some(Fix::UpdateLauncher))
                && items.iter().any(|i| i.fix == Some(Fix::UpdateMod))
        );
        assert!(items.iter().any(|i| i.detail.contains("Saves can be lost.")));
    }

    #[test]
    fn a_rule_of_the_setup_list_has_a_fix_by_its_kind() {
        assert_eq!(Fix::for_rule("steam-running"), Some(Fix::StartSteam));
        assert_eq!(Fix::for_rule("game-found"), Some(Fix::LocateGame));
        assert_eq!(Fix::for_rule("mod-installed"), Some(Fix::InstallMod));
        assert_eq!(Fix::for_rule("free-disk"), None);
    }
}
