// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! A game's option toggles (its settings window) and the way the launcher talks to a pack's own program: it is started with a list of
//! arguments (never a command line built from text), its standard output is read as JSON lines, and it is stopped at a timeout.

use crate::bridge_protocol::{self, BridgeIn, BridgeSaveSlot};
use crate::catalog_limits as lim;
use crate::process_runner;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub const MAX_OPTIONS: usize = 8;
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
pub const APPLY_TIMEOUT: Duration = Duration::from_secs(15 * 60);
pub const SAVES_TIMEOUT: Duration = Duration::from_secs(30);

/// What an option may require of the PC: nothing, or an NVIDIA RTX card.
pub const REQUIREMENTS: [&str; 2] = ["", "nvidia-rtx"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameOption {
    pub id: String,
    pub title: String,
    pub hint: String,
    pub requires: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionStatus {
    pub available: bool,
    pub on: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionRun {
    pub ok: bool,
    pub message: String,
}

/// The `options` array of a `game.json`; `None` when any entry is not valid.
pub fn parse(root: &Value) -> Option<Vec<GameOption>> {
    let mut list: Vec<GameOption> = Vec::new();
    for e in lim::items(root, "options") {
        if !e.is_object() || list.len() >= MAX_OPTIONS {
            return None;
        }
        let (id, requires, title) = (
            lim::str_of(e, "id"),
            lim::str_of(e, "requires"),
            lim::text(&lim::str_of(e, "title"), 60),
        );
        if !lim::is_id(&id) || title.is_empty() || !REQUIREMENTS.contains(&requires.as_str()) || list.iter().any(|o| o.id == id) {
            return None;
        }
        list.push(GameOption {
            id,
            title,
            hint: lim::text(&lim::str_of(e, "hint"), 300),
            requires,
        });
    }
    Some(list)
}

/// What a run of the program produced.
#[derive(Debug, Clone, Default)]
pub struct Run {
    /// The last non-empty line it printed.
    pub last: Option<String>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub start_error: Option<String>,
    /// The first 50 lines it printed.
    pub lines: Vec<String>,
}

impl Run {
    fn failed(msg: &str) -> Self {
        Self {
            start_error: Some(msg.into()),
            ..Self::default()
        }
    }
}

/// Starts the program with its arguments as a list, reads its output line by line (`say` gets each `{"say":"..."}` line) and stops it at the
/// timeout or when `cancel` is set.
pub fn run(program: &Path, args: &[String], timeout: Duration, say: &mut dyn FnMut(&str), cancel: &AtomicBool) -> Run {
    if args.get(2).is_some_and(|a| !lim::is_id(a)) {
        return if program.is_file() {
            Run::failed("Not a valid option.")
        } else {
            Run::failed("The program of the mod is not there.")
        };
    }
    let mut out = Run::default();
    let outcome = process_runner::run_lines(
        program,
        args,
        timeout,
        bridge_protocol::MAX_LINE, // a list of saves is one line (a hundred saves is about 10 KB); the cap is the protocol's
        &mut |line| {
            if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(line) {
                if let Some(s) = o.get("say").and_then(Value::as_str) {
                    say(&lim::text(s, 300));
                }
            }
            if out.lines.len() < 50 {
                out.lines.push(line.to_string());
            }
            out.last = Some(line.to_string());
        },
        cancel,
    );
    out.timed_out = outcome.timed_out;
    out.cancelled = outcome.cancelled;
    out.start_error = outcome.start_error;
    out
}

/// `options check <id> --game-dir <dir>`: whether the option is available and on; `None` when the program gave no answer.
pub fn check(program: &Path, option_id: &str, game_dir: &str, timeout: Option<Duration>, cancel: &AtomicBool) -> Option<OptionStatus> {
    let args = ["options", "check", option_id, "--game-dir", game_dir].map(String::from);
    let r = run(program, &args, timeout.unwrap_or(CHECK_TIMEOUT), &mut |_| {}, cancel);
    let v: Value = serde_json::from_str(r.last.as_deref()?).ok()?;
    let flag = |n: &str| v.get(n) == Some(&Value::Bool(true));
    let note = v.get("note").and_then(Value::as_str).map(|n| lim::text(n, 300)).unwrap_or_default();
    Some(OptionStatus {
        available: flag("available"),
        on: flag("on"),
        note,
    })
}

/// `options apply <id> on|off --game-dir <dir>`: the program may print `{"say":"..."}` progress lines and ends with `{"ok":bool,"message":"..."}`.
pub fn apply(
    program: &Path,
    option_id: &str,
    on: bool,
    game_dir: &str,
    say: &mut dyn FnMut(&str),
    timeout: Option<Duration>,
    cancel: &AtomicBool,
) -> OptionRun {
    let args = ["options", "apply", option_id, if on { "on" } else { "off" }, "--game-dir", game_dir].map(String::from);
    let r = run(program, &args, timeout.unwrap_or(APPLY_TIMEOUT), say, cancel);
    if r.timed_out {
        return OptionRun {
            ok: false,
            message: "It took too long and was stopped.".into(),
        };
    }
    if let Some(v) = r.last.as_deref().and_then(|l| serde_json::from_str::<Value>(l).ok()) {
        if let Some(Value::Bool(ok)) = v.get("ok") {
            let message = v
                .get("message")
                .and_then(Value::as_str)
                .map(|m| lim::text(m, 600))
                .unwrap_or_default();
            return OptionRun { ok: *ok, message };
        }
    }
    OptionRun {
        ok: false,
        message: r.start_error.unwrap_or_else(|| "The program did not answer.".into()),
    }
}

/// `saves list`: the game's saves, read from its program while the game is closed.
pub fn list_saves(program: &Path, timeout: Option<Duration>, cancel: &AtomicBool) -> Option<Vec<BridgeSaveSlot>> {
    let r = run(
        program,
        &["saves", "list"].map(String::from),
        timeout.unwrap_or(SAVES_TIMEOUT),
        &mut |_| {},
        cancel,
    );
    r.lines
        .iter()
        .filter_map(|l| bridge_protocol::parse(l))
        .filter_map(|m| if let BridgeIn::Saves(s) = m { Some(s) } else { None })
        .next_back()
}

/// `saves delete <id>`: `(ok, message, the new list)`.
pub fn delete_save(
    program: &Path,
    save_id: &str,
    timeout: Option<Duration>,
    cancel: &AtomicBool,
) -> (bool, String, Option<Vec<BridgeSaveSlot>>) {
    if !bridge_protocol::is_save_id(save_id) {
        return (false, "That save is not there any more.".into(), None);
    }
    let r = run(
        program,
        &["saves", "delete", save_id].map(String::from),
        timeout.unwrap_or(SAVES_TIMEOUT),
        &mut |_| {},
        cancel,
    );
    let messages: Vec<BridgeIn> = r.lines.iter().filter_map(|l| bridge_protocol::parse(l)).collect();
    let slots = messages
        .iter()
        .filter_map(|m| if let BridgeIn::Saves(s) = m { Some(s.clone()) } else { None })
        .next_back();
    match messages
        .iter()
        .filter_map(|m| {
            if let BridgeIn::Toast { text, error } = m {
                Some((text, error))
            } else {
                None
            }
        })
        .next_back()
    {
        Some((text, error)) => (!*error, text.clone(), slots),
        None => (false, no_answer(&r), slots),
    }
}

/// `saves delete-all`: `(ok, message)`.
pub fn delete_all_saves(program: &Path, timeout: Option<Duration>, cancel: &AtomicBool) -> (bool, String) {
    let r = run(
        program,
        &["saves", "delete-all"].map(String::from),
        timeout.unwrap_or(SAVES_TIMEOUT),
        &mut |_| {},
        cancel,
    );
    match r
        .lines
        .iter()
        .filter_map(|l| bridge_protocol::parse(l))
        .filter_map(|m| {
            if let BridgeIn::Toast { text, error } = m {
                Some((text, error))
            } else {
                None
            }
        })
        .next_back()
    {
        Some((text, error)) => (!error, text),
        None => (false, no_answer(&r)),
    }
}

fn no_answer(r: &Run) -> String {
    r.start_error.clone().unwrap_or_else(|| {
        if r.timed_out {
            "It took too long and was stopped.".into()
        } else {
            "The program did not answer.".into()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn options_are_read_and_checked() {
        let o = parse(&json!({"options": [
            {"id": "ray-tracing", "title": "Ray tracing", "hint": "Needs a card", "requires": "nvidia-rtx"},
            {"id": "intro", "title": "Skip the intro"}
        ]}))
        .unwrap();
        assert_eq!((o.len(), o[0].requires.as_str(), o[1].hint.as_str()), (2, "nvidia-rtx", ""));
        assert!(parse(&json!({"options": [{"id": "a b", "title": "x"}]})).is_none());
        assert!(parse(&json!({"options": [{"id": "a", "title": ""}]})).is_none());
        assert!(parse(&json!({"options": [{"id": "a", "title": "x", "requires": "quantum"}]})).is_none());
        assert!(parse(&json!({"options": [{"id": "a", "title": "x"}, {"id": "a", "title": "y"}]})).is_none());
        let nine: Vec<Value> = (0..9).map(|i| json!({"id": format!("o{i}"), "title": "x"})).collect();
        assert!(parse(&json!({"options": nine})).is_none());
        assert_eq!(parse(&json!({})).unwrap().len(), 0);
    }

    #[cfg(unix)]
    mod unix {
        use super::super::*;
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::path::PathBuf;

        fn program(name: &str, body: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("cairn-opt-{name}-{}", std::process::id()));
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
        fn a_check_reads_the_last_line() {
            let p = program("check", r#"echo '{"available":true,"on":false,"note":"<b>Needs</b> a restart"}'"#);
            let s = check(&p, "ray", "/game", None, &never()).unwrap();
            assert_eq!(
                s,
                OptionStatus {
                    available: true,
                    on: false,
                    note: "bNeeds/b a restart".into()
                }
            );
        }

        #[test]
        fn an_apply_reports_progress_and_the_verdict() {
            let p = program("apply", r#"echo '{"say":"Working"}'; echo '{"ok":true,"message":"Done"}'"#);
            let mut said = Vec::new();
            let r = apply(&p, "ray", true, "/game", &mut |s| said.push(s.to_string()), None, &never());
            assert_eq!(
                r,
                OptionRun {
                    ok: true,
                    message: "Done".into()
                }
            );
            assert_eq!(said, vec!["Working"]);
        }

        #[test]
        fn the_arguments_reach_the_program_as_a_list() {
            let p = program("args", r#"printf '{"ok":false,"message":"%s|%s|%s|%s"}\n' "$1" "$2" "$3" "$6""#);
            let r = apply(&p, "ray", false, "/my game; rm -rf", &mut |_| {}, None, &never());
            assert_eq!(r.message, "options|apply|ray|/my game; rm -rf");
        }

        #[test]
        fn a_program_that_hangs_is_stopped_at_the_timeout() {
            let p = program("hang", "sleep 30");
            let t = std::time::Instant::now();
            let r = apply(&p, "ray", true, "/g", &mut |_| {}, Some(Duration::from_millis(300)), &never());
            assert_eq!(
                r,
                OptionRun {
                    ok: false,
                    message: "It took too long and was stopped.".into()
                }
            );
            assert!(t.elapsed() < Duration::from_secs(10));
        }

        #[test]
        fn a_missing_program_or_a_bad_option_name_is_refused() {
            let r = apply(Path::new("/no/such/tool"), "ray", true, "/g", &mut |_| {}, None, &never());
            assert_eq!(r.message, "The program of the mod is not there.");
            let p = program("bad", "echo hi");
            assert_eq!(
                apply(&p, "a b", true, "/g", &mut |_| {}, None, &never()).message,
                "Not a valid option."
            );
            assert!(check(&p, "ray", "/g", None, &never()).is_none()); // "hi" is not JSON
        }

        #[test]
        fn saves_are_listed_and_deleted_through_the_program() {
            let list = r#"{"t":"saves","slots":[{"id":"run-1","name":"Run","kind":"world"}]}"#;
            let p = program(
                "saves",
                &format!(
                    "if [ \"$2\" = list ]; then echo '{list}'; else echo '{list}'; echo '{{\"t\":\"toast\",\"text\":\"Deleted.\"}}'; fi"
                ),
            );
            assert_eq!(list_saves(&p, None, &never()).unwrap()[0].id, "run-1");
            let (ok, msg, slots) = delete_save(&p, "run-1", None, &never());
            assert_eq!((ok, msg.as_str(), slots.map(|s| s.len())), (true, "Deleted.", Some(1)));
            assert!(!delete_save(&p, "../x", None, &never()).0);
            let silent = program("silent", "true");
            assert_eq!(
                delete_all_saves(&silent, None, &never()),
                (false, "The program did not answer.".into())
            );
        }

        #[test]
        fn cancelling_stops_the_program() {
            let p = program("cancel", "sleep 30");
            let cancel = AtomicBool::new(true);
            let r = run(&p, &["x".to_string()], Duration::from_secs(60), &mut |_| {}, &cancel);
            assert!(r.cancelled);
        }
    }
}
