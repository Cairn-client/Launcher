// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! A game's setup checklist as the window draws it: lines that are done, are being done, or need the player. The lines come from the game's own
//! pack program (`setup check`) when it has one, else from the checks the catalog names (`setup` in `game.json`).

use cairn_setup::game_setup::{SetupResult, SetupState};
use cairn_setup::pack_setup::{PackSetupReport, PackStep, PackStepStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineState {
    Done,
    /// Nothing for the player to do (it is being done, or waits for an earlier line).
    Working,
    NeedsYou,
    /// Fine to leave: a warning.
    Warn,
}

/// What a line's button does: a word the pack's program understands, or one of the launcher's own fixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineAction {
    /// An action of the pack's program (`setup do <word>`).
    Pack(String),
    /// The launcher's own fix for a catalog check.
    Fix(crate::preflight::Fix),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetupLine {
    pub id: String,
    pub title: String,
    pub state: LineState,
    pub detail: String,
    pub action: Option<(LineAction, String)>,
    pub progress: Option<f64>,
    pub optional: bool,
    /// The step concerns the whole PC (Steam), not one game.
    pub system: bool,
    /// The pack asks for the action to be started without a click: `once` or `while`.
    pub auto: Option<String>,
}

/// The lines of a pack's checklist.
pub fn from_report(report: &PackSetupReport) -> Vec<SetupLine> {
    report.steps.iter().map(from_step).collect()
}

fn from_step(s: &PackStep) -> SetupLine {
    SetupLine {
        id: s.id.clone(),
        title: s.title.clone(),
        state: match s.status {
            PackStepStatus::Done => LineState::Done,
            PackStepStatus::Working | PackStepStatus::Waiting => LineState::Working,
            PackStepStatus::NeedsYou => LineState::NeedsYou,
        },
        detail: s.detail.clone(),
        action: s
            .action
            .clone()
            .map(|a| (LineAction::Pack(a), s.action_label.clone().unwrap_or_else(|| "Do it".into()))),
        progress: s.progress,
        optional: s.optional,
        system: s.system,
        auto: s.auto.clone(),
    }
}

/// The lines of the catalog's own checks.
pub fn from_results(results: &[SetupResult]) -> Vec<SetupLine> {
    results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let fix = crate::preflight::Fix::for_rule(&r.kind);
            SetupLine {
                id: format!("{}-{i}", r.kind),
                title: r.title.clone(),
                state: match r.state {
                    SetupState::Ok => LineState::Done,
                    SetupState::Warn => LineState::Warn,
                    SetupState::Fail => LineState::NeedsYou,
                },
                detail: r.detail.clone(),
                action: (r.state != SetupState::Ok)
                    .then(|| fix.map(|f| (LineAction::Fix(f), f.label().to_string())))
                    .flatten(),
                progress: None,
                optional: r.state == SetupState::Warn,
                system: r.kind == "steam-running",
                auto: None,
            }
        })
        .collect()
}

/// Every line that has to be done is done (a line that is only advice does not hold the game back).
pub fn all_done(lines: &[SetupLine]) -> bool {
    lines.iter().all(|l| l.optional || l.state == LineState::Done)
}

/// Something the player can still do about an optional line (the checklist shows itself once for it).
pub fn optional_needs_you(lines: &[SetupLine]) -> bool {
    lines.iter().any(|l| l.optional && l.state != LineState::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preflight::Fix;

    #[test]
    fn a_packs_steps_become_lines_with_their_buttons() {
        let report = PackSetupReport {
            steps: vec![
                PackStep {
                    id: "steam".into(),
                    title: "Steam".into(),
                    status: PackStepStatus::Done,
                    detail: "".into(),
                    action: None,
                    action_label: None,
                    progress: None,
                    optional: false,
                    system: true,
                    auto: None,
                },
                PackStep {
                    id: "tools".into(),
                    title: "Tools".into(),
                    status: PackStepStatus::NeedsYou,
                    detail: "Install them".into(),
                    action: Some("install".into()),
                    action_label: Some("Install".into()),
                    progress: Some(0.5),
                    optional: false,
                    system: false,
                    auto: Some("once".into()),
                },
                PackStep {
                    id: "later".into(),
                    title: "Later".into(),
                    status: PackStepStatus::Waiting,
                    detail: "".into(),
                    action: None,
                    action_label: None,
                    progress: None,
                    optional: true,
                    system: false,
                    auto: None,
                },
            ],
            ready: false,
            optional_needs_you: true,
            game_exe: None,
            game_dir: None,
            game_exe_note: None,
        };
        let lines = from_report(&report);
        assert_eq!(
            lines.iter().map(|l| l.state).collect::<Vec<_>>(),
            vec![LineState::Done, LineState::NeedsYou, LineState::Working]
        );
        assert_eq!(lines[1].action, Some((LineAction::Pack("install".into()), "Install".into())));
        assert!(lines[0].system && lines[1].auto.as_deref() == Some("once") && lines[1].progress == Some(0.5));
        assert!(!all_done(&lines));
        assert!(all_done(&lines[..1]) && all_done(&[lines[0].clone(), lines[2].clone()]));
        assert!(optional_needs_you(&lines) && !optional_needs_you(&lines[..2]));
    }

    #[test]
    fn the_catalogs_checks_become_lines_with_the_launchers_own_fixes() {
        let results = vec![
            SetupResult {
                title: "Steam".into(),
                state: SetupState::Ok,
                detail: "Steam is running.".into(),
                kind: "steam-running".into(),
            },
            SetupResult {
                title: "Game".into(),
                state: SetupState::Fail,
                detail: "Not found".into(),
                kind: "game-found".into(),
            },
            SetupResult {
                title: "Loader".into(),
                state: SetupState::Warn,
                detail: "Install it".into(),
                kind: "game-file".into(),
            },
            SetupResult {
                title: "Mod".into(),
                state: SetupState::Fail,
                detail: "Download it".into(),
                kind: "mod-installed".into(),
            },
        ];
        let lines = from_results(&results);
        assert_eq!(
            lines.iter().map(|l| l.state).collect::<Vec<_>>(),
            vec![LineState::Done, LineState::NeedsYou, LineState::Warn, LineState::NeedsYou]
        );
        assert_eq!(lines[1].action, Some((LineAction::Fix(Fix::LocateGame), "LOCATE THE GAME".into())));
        assert!(lines[0].action.is_none() && lines[2].action.is_none()); // a file check has no fix of its own
        assert_eq!(lines[3].action.as_ref().unwrap().0, LineAction::Fix(Fix::InstallMod));
        assert!(lines[0].system && !lines[1].system && lines[2].optional);
        assert!(!all_done(&lines) && all_done(&lines[..1]));
    }
}
