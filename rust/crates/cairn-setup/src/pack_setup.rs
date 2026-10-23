// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! A game's setup checklist when the pack's own program drives it (`setup check` and `setup do <action>`, docs/PACKS.md). The launcher draws
//! the steps and the buttons and decides readiness itself: a program cannot say "ready" over a step that is not done.

use crate::catalog_limits as lim;
use crate::process_runner;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub const MAX_STEPS: usize = 16;
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(40);
pub const DO_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackStepStatus {
    Done,
    /// Nothing for the player to do.
    Working,
    NeedsYou,
    /// An earlier step first.
    Waiting,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackStep {
    pub id: String,
    pub title: String,
    pub status: PackStepStatus,
    pub detail: String,
    pub action: Option<String>,
    pub action_label: Option<String>,
    pub progress: Option<f64>,
    pub optional: bool,
    /// Concerns the whole PC (shown in "System setup").
    pub system: bool,
    /// `once` per launcher session or `while` the step is the next thing to do.
    pub auto: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackSetupReport {
    pub steps: Vec<PackStep>,
    pub ready: bool,
    pub optional_needs_you: bool,
    pub game_exe: Option<String>,
    pub game_dir: Option<String>,
    pub game_exe_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackActionResult {
    pub ok: bool,
    pub message: String,
    pub pending_seconds: i32,
}

/// One `{"step":{...}}` line; `None` for anything that is not a valid step.
pub fn parse_step(line: &str) -> Option<PackStep> {
    let v: Value = serde_json::from_str(line).ok()?;
    let s = v.get("step").filter(|s| s.is_object())?;
    let (id, title, action) = (
        lim::str_of(s, "id"),
        lim::text(&lim::str_of(s, "title"), 120),
        lim::str_of(s, "action"),
    );
    let status = match lim::str_of(s, "status").as_str() {
        "done" => PackStepStatus::Done,
        "working" => PackStepStatus::Working,
        "needs-you" => PackStepStatus::NeedsYou,
        "waiting" => PackStepStatus::Waiting,
        _ => return None,
    };
    if !lim::is_id(&id) || title.is_empty() || (!action.is_empty() && !lim::is_id(&action)) {
        return None;
    }
    let auto = lim::str_of(s, "auto");
    if !auto.is_empty() && auto != "once" && auto != "while" {
        return None;
    }
    let progress = s.get("progress").and_then(Value::as_f64).map(|p| p.clamp(0.0, 1.0));
    let label = lim::text(&lim::str_of(s, "actionLabel"), 40);
    Some(PackStep {
        id,
        title,
        status,
        detail: lim::text(&lim::str_of(s, "detail"), 700),
        action_label: (!action.is_empty() && !label.is_empty()).then_some(label),
        action: (!action.is_empty()).then_some(action),
        progress,
        optional: lim::flag(s, "optional"),
        system: lim::flag(s, "system"),
        auto: (!auto.is_empty()).then_some(auto),
    })
}

/// `setup check [--game-exe EXE]`: the steps, then the line that ends the list. `None` when the program did not answer in time or gave no steps.
pub fn check(program: &Path, game_exe: Option<&str>, timeout: Option<Duration>, cancel: &AtomicBool) -> Option<PackSetupReport> {
    let mut steps: Vec<PackStep> = Vec::new();
    let (mut done, mut ready, mut optional_needs_you) = (false, false, false);
    let (mut exe, mut game_dir, mut note): (Option<String>, Option<String>, Option<String>) = (None, None, None);
    let outcome = process_runner::run_lines(
        program,
        &args("check", None, game_exe),
        timeout.unwrap_or(CHECK_TIMEOUT),
        4096,
        &mut |line| {
            if steps.len() < MAX_STEPS {
                if let Some(step) = parse_step(line) {
                    if steps.iter().all(|x| x.id != step.id) {
                        steps.push(step);
                    }
                    return;
                }
            }
            let Ok(r) = serde_json::from_str::<Value>(line) else { return };
            if !r.is_object() {
                return;
            }
            if lim::flag(&r, "done") {
                done = true;
                ready = lim::flag(&r, "ready");
                optional_needs_you = lim::flag(&r, "optionalNeedsYou");
            } else if let Some(n) = r.get("gameExeNote").and_then(Value::as_str) {
                note = Some(lim::text(n, 400));
            } else if let Some(f) = r.get("found").filter(|f| f.is_object()) {
                if let Some(e) = f.get("gameExe").or_else(|| f.get("moddingToolsExe")).and_then(Value::as_str) {
                    exe = Some(e.to_string());
                    game_dir = f.get("gameDir").and_then(Value::as_str).map(str::to_string);
                }
            }
        },
        cancel,
    );
    if outcome.timed_out || outcome.cancelled || !done || steps.is_empty() {
        return None;
    }
    // the launcher decides readiness itself from the steps
    let really = steps.iter().all(|x| x.optional || x.status == PackStepStatus::Done);
    Some(PackSetupReport {
        steps,
        ready: ready && really,
        optional_needs_you: optional_needs_you && really,
        game_exe: exe,
        game_dir,
        game_exe_note: note.filter(|n| !n.is_empty()),
    })
}

/// `setup do <action> [--game-exe EXE]`: `say` gets the progress lines, `progress` the 0..1 values; ends with `{"ok":bool,"message":"...","pending":seconds}`.
pub fn do_action(
    program: &Path,
    action: &str,
    game_exe: Option<&str>,
    say: &mut dyn FnMut(&str),
    progress: &mut dyn FnMut(f64),
    timeout: Option<Duration>,
    cancel: &AtomicBool,
) -> PackActionResult {
    if !lim::is_id(action) {
        return PackActionResult {
            ok: false,
            message: "Not a valid action.".into(),
            pending_seconds: 0,
        };
    }
    let mut last: Option<String> = None;
    let outcome = process_runner::run_lines(
        program,
        &args("do", Some(action), game_exe),
        timeout.unwrap_or(DO_TIMEOUT),
        4096,
        &mut |line| {
            let Ok(r) = serde_json::from_str::<Value>(line) else { return };
            if !r.is_object() {
                return;
            }
            if let Some(text) = r.get("say").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                say(&lim::text(text, 300));
            }
            if let Some(p) = r.get("progress").and_then(Value::as_f64) {
                progress(p.clamp(0.0, 1.0));
            }
            if r.get("ok").is_some() {
                last = Some(line.to_string());
            }
        },
        cancel,
    );
    if outcome.timed_out {
        return PackActionResult {
            ok: false,
            message: "It took too long and was stopped.".into(),
            pending_seconds: 0,
        };
    }
    if let Some(r) = last.as_deref().and_then(|l| serde_json::from_str::<Value>(l).ok()) {
        let message = r
            .get("message")
            .and_then(Value::as_str)
            .map(|m| lim::text(m, 600))
            .unwrap_or_default();
        let pending = r
            .get("pending")
            .and_then(Value::as_i64)
            .map(|n| n.clamp(0, 300) as i32)
            .unwrap_or(0);
        return PackActionResult {
            ok: lim::flag(&r, "ok"),
            message,
            pending_seconds: pending,
        };
    }
    PackActionResult {
        ok: false,
        message: outcome.start_error.unwrap_or_else(|| "The program did not answer.".into()),
        pending_seconds: 0,
    }
}

fn args(verb: &str, action: Option<&str>, game_exe: Option<&str>) -> Vec<String> {
    let mut a = vec!["setup".to_string(), verb.to_string()];
    if let Some(action) = action {
        a.push(action.to_string());
    }
    if let Some(exe) = game_exe.filter(|e| !e.trim().is_empty() && e.chars().count() <= 260 && !e.chars().any(char::is_control)) {
        a.push("--game-exe".into());
        a.push(exe.to_string());
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_line_is_read_and_limited() {
        let s = parse_step(r#"{"step":{"id":"workspace","title":"Game files linked","status":"working","detail":"x","action":"link-workspace","actionLabel":"Try again","progress":4,"optional":true,"system":true,"auto":"while"}}"#).unwrap();
        assert_eq!(
            (s.id.as_str(), s.status, s.action.as_deref(), s.action_label.as_deref()),
            ("workspace", PackStepStatus::Working, Some("link-workspace"), Some("Try again"))
        );
        assert_eq!(
            (s.progress, s.optional, s.system, s.auto.as_deref()),
            (Some(1.0), true, true, Some("while"))
        );
        let plain = parse_step(r#"{"step":{"id":"a","title":"A","status":"done"}}"#).unwrap();
        assert_eq!(
            (plain.action, plain.action_label, plain.auto, plain.progress),
            (None, None, None, None)
        );
        // a label without an action is dropped
        assert_eq!(
            parse_step(r#"{"step":{"id":"a","title":"A","status":"done","actionLabel":"Go"}}"#)
                .unwrap()
                .action_label,
            None
        );
        for bad in [
            r#"{"step":{"id":"a b","title":"A","status":"done"}}"#,
            r#"{"step":{"id":"a","title":"","status":"done"}}"#,
            r#"{"step":{"id":"a","title":"A","status":"great"}}"#,
            r#"{"step":{"id":"a","title":"A","status":"done","action":"run it"}}"#,
            r#"{"step":{"id":"a","title":"A","status":"done","auto":"always"}}"#,
            r#"{"done":true}"#,
            "not json",
        ] {
            assert!(parse_step(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_arguments_carry_the_game_exe_only_when_it_is_plain() {
        assert_eq!(
            args("check", None, Some("C:\\g\\game.exe")),
            vec!["setup", "check", "--game-exe", "C:\\g\\game.exe"]
        );
        assert_eq!(args("do", Some("link"), None), vec!["setup", "do", "link"]);
        assert_eq!(args("check", None, Some("a\nb")), vec!["setup", "check"]);
        assert_eq!(args("check", None, Some("  ")), vec!["setup", "check"]);
    }

    #[cfg(unix)]
    mod unix {
        use super::super::*;
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::path::PathBuf;

        fn program(name: &str, body: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("cairn-setupprog-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            let p = dir.join("tool.sh");
            fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            p
        }
        fn never() -> AtomicBool {
            AtomicBool::new(false)
        }

        #[test]
        fn a_check_collects_the_steps_and_decides_readiness_itself() {
            let p = program(
                "check",
                r#"echo '{"step":{"id":"steam","title":"Steam","status":"done"}}'
echo '{"step":{"id":"link","title":"Link","status":"needs-you","action":"link"}}'
echo '{"step":{"id":"steam","title":"duplicate","status":"done"}}'
echo '{"gameExeNote":"Wrong exe"}'
printf '%s\n' '{"found":{"gameExe":"C:\\g\\game.exe","gameDir":"C:\\g"}}'
echo '{"done":true,"ready":true,"optionalNeedsYou":false}'"#,
            );
            let r = check(&p, None, None, &never()).unwrap();
            assert_eq!(r.steps.len(), 2);
            assert!(!r.ready); // the program said ready over a step that is not done
            assert_eq!(
                (r.game_exe.as_deref(), r.game_dir.as_deref(), r.game_exe_note.as_deref()),
                (Some("C:\\g\\game.exe"), Some("C:\\g"), Some("Wrong exe"))
            );
        }

        #[test]
        fn a_check_with_everything_done_is_ready_and_one_that_never_ends_is_nothing() {
            let ok = program(
                "ready",
                r#"echo '{"step":{"id":"a","title":"A","status":"done"}}'; echo '{"done":true,"ready":true}'"#,
            );
            assert!(check(&ok, None, None, &never()).unwrap().ready);
            let open = program("open", r#"echo '{"step":{"id":"a","title":"A","status":"done"}}'"#);
            assert!(check(&open, None, None, &never()).is_none());
            let optional = program(
                "optional",
                r#"echo '{"step":{"id":"a","title":"A","status":"needs-you","optional":true}}'; echo '{"done":true,"ready":true,"optionalNeedsYou":true}'"#,
            );
            let r = check(&optional, None, None, &never()).unwrap();
            assert!(r.ready && r.optional_needs_you);
            let slow = program("slow", "sleep 30");
            assert!(check(&slow, None, Some(Duration::from_millis(200)), &never()).is_none());
        }

        #[test]
        fn an_action_reports_progress_and_the_verdict() {
            let p = program(
                "do",
                r#"echo '{"say":"Linking"}'; echo '{"progress":0.5}'; echo '{"ok":true,"message":"Done","pending":900}'"#,
            );
            let (mut said, mut seen) = (Vec::new(), Vec::new());
            let r = do_action(
                &p,
                "link",
                None,
                &mut |s| said.push(s.to_string()),
                &mut |x| seen.push(x),
                None,
                &never(),
            );
            assert_eq!(
                r,
                PackActionResult {
                    ok: true,
                    message: "Done".into(),
                    pending_seconds: 300
                }
            );
            assert_eq!((said, seen), (vec!["Linking".to_string()], vec![0.5]));
            assert!(!do_action(&p, "not valid", None, &mut |_| {}, &mut |_| {}, None, &never()).ok);
            let silent = program("silent", "true");
            assert_eq!(
                do_action(&silent, "link", None, &mut |_| {}, &mut |_| {}, None, &never()).message,
                "The program did not answer."
            );
            let slow = program("slowdo", "sleep 30");
            let r = do_action(
                &slow,
                "link",
                None,
                &mut |_| {},
                &mut |_| {},
                Some(Duration::from_millis(200)),
                &never(),
            );
            assert_eq!(r.message, "It took too long and was stopped.");
        }
    }
}
