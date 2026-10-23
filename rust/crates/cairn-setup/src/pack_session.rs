// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The connection side of a session, run by the pack's own program (`session`, docs/SESSION.md): the launcher sends commands as JSON lines to
//! its standard input and reads events as JSON lines from its standard output. The launcher shows what it is told and does what the player asks;
//! the relay, the game's start, the plugin and the agent are the pack's.

use crate::catalog_limits as lim;
use crate::process_runner;
use crate::time_util;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PROTOCOL: i32 = 1;
pub const MAX_LINE: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHostView {
    pub relay_up: bool,
    pub error: String,
    pub steam_state: String,
    pub steam_message: String,
    pub steam_code: Option<String>,
    pub refused: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    pub stage: String,
    pub status: String,
    pub connect_enabled: bool,
    pub connect_reason: String,
    pub conn_text: String,
    pub conn_bad: bool,
    pub join_state: String,
    pub join_message: String,
    pub join_can_fresh: bool,
    pub host: SessionHostView,
    pub game_running: bool,
}

impl SessionView {
    pub fn idle() -> Self {
        Self {
            stage: "idle".into(),
            status: String::new(),
            connect_enabled: true,
            connect_reason: String::new(),
            conn_text: String::new(),
            conn_bad: false,
            join_state: "idle".into(),
            join_message: String::new(),
            join_can_fresh: false,
            host: SessionHostView {
                relay_up: false,
                error: String::new(),
                steam_state: "off".into(),
                steam_message: String::new(),
                steam_code: None,
                refused: String::new(),
            },
            game_running: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProblem {
    pub title: String,
    pub text: String,
    pub next: String,
    pub report_bug: bool,
    pub launch_anyway: bool,
    pub address_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFriend {
    pub name: String,
    pub code: String,
    pub release: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFriends {
    pub state: String,
    pub message: String,
    pub next: String,
    pub friends: Vec<SessionFriend>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGame {
    pub pid: i32,
    /// Seconds since 1970-01-01 UTC when the game started.
    pub started: i64,
    pub hosting: bool,
}

/// What the pack's program tells the launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    State(SessionView),
    Problem(SessionProblem),
    Error(String),
    Message {
        title: String,
        text: String,
        next: String,
    },
    Game(SessionGame),
    GameExit,
    Refused {
        text: String,
        key: String,
    },
    VersionMismatch(String),
    Toast(String),
    Friends(SessionFriends),
    Log(String),
    /// The program ended.
    Exited,
}

/// One line of output as an event; `None` for anything that is not a known good message. Text is cleaned and cut here, before it is shown.
pub fn parse_event(line: &str) -> Option<Vec<SessionEvent>> {
    let r: Value = serde_json::from_str(line).ok()?;
    let t = r.get("t")?.as_str()?.to_string();
    if !r.is_object() {
        return None;
    }
    let s = |e: &Value, n: &str, max: usize| lim::text(&lim::str_of(e, n), max);
    let b = lim::flag;
    let obj = |n: &str| r.get(n).filter(|v| v.is_object()).cloned().unwrap_or(Value::Null);
    match t.as_str() {
        "state" => {
            let (host, conn, join, connect) = (obj("host"), obj("conn"), obj("join"), obj("connect"));
            let stage = s(&r, "stage", 20);
            if !matches!(
                stage.as_str(),
                "idle" | "waiting-connect" | "injecting" | "verifying" | "connected" | "failed"
            ) {
                return None;
            }
            let steam_code = host.get("steamCode").and_then(Value::as_str).map(|c| lim::text(c, 200));
            let join_state = s(&join, "state", 30);
            let steam_state = s(&host, "steamState", 30);
            Some(vec![SessionEvent::State(SessionView {
                stage,
                status: s(&r, "status", 600),
                connect_enabled: !connect.is_object() || b(&connect, "enabled"),
                connect_reason: s(&connect, "reason", 600),
                conn_text: s(&conn, "text", 600),
                conn_bad: b(&conn, "bad"),
                join_state: if join_state.is_empty() { "idle".into() } else { join_state },
                join_message: s(&join, "message", 600),
                join_can_fresh: b(&join, "canFresh"),
                host: SessionHostView {
                    relay_up: b(&host, "relay"),
                    error: s(&host, "error", 600),
                    steam_state: if steam_state.is_empty() { "off".into() } else { steam_state },
                    steam_message: s(&host, "steamMessage", 600),
                    steam_code,
                    refused: s(&host, "refused", 800),
                },
                game_running: b(&r, "game"),
            })])
        }
        "problem" => Some(vec![
            SessionEvent::Problem(SessionProblem {
                title: s(&r, "title", 80),
                text: s(&r, "text", 1200),
                next: s(&r, "next", 1500),
                report_bug: b(&r, "reportBug"),
                launch_anyway: b(&r, "launchAnyway"),
                address_fallback: false,
            }),
            SessionEvent::Log(format!("session: {}", s(&r, "log", 300))),
        ]),
        "message" => {
            let (title, text, next) = (s(&r, "title", 80), s(&r, "text", 1200), s(&r, "next", 1500));
            Some(vec![if b(&r, "addressFallback") {
                SessionEvent::Problem(SessionProblem {
                    title,
                    text,
                    next,
                    report_bug: false,
                    launch_anyway: false,
                    address_fallback: true,
                })
            } else {
                SessionEvent::Message { title, text, next }
            }])
        }
        "error" => Some(
            Some(s(&r, "text", 800))
                .filter(|t| !t.is_empty())
                .map(SessionEvent::Error)
                .into_iter()
                .collect(),
        ),
        "toast" => Some(
            Some(s(&r, "text", 200))
                .filter(|t| !t.is_empty())
                .map(SessionEvent::Toast)
                .into_iter()
                .collect(),
        ),
        "log" => Some(
            Some(s(&r, "line", 600))
                .filter(|t| !t.is_empty())
                .map(|l| SessionEvent::Log(format!("session: {l}")))
                .into_iter()
                .collect(),
        ),
        "game" => {
            let pid = r
                .get("pid")
                .and_then(Value::as_i64)
                .and_then(|p| i32::try_from(p).ok())
                .filter(|p| *p > 0)?;
            let started = time_util::parse_utc(&s(&r, "startedAt", 40)).unwrap_or_else(time_util::now_utc);
            Some(vec![SessionEvent::Game(SessionGame {
                pid,
                started,
                hosting: b(&r, "hosting"),
            })])
        }
        "game_exit" => Some(vec![SessionEvent::GameExit]),
        "refused" => Some(vec![SessionEvent::Refused {
            text: s(&r, "text", 800),
            key: s(&r, "key", 60),
        }]),
        "version_mismatch" => Some(vec![SessionEvent::VersionMismatch(s(&r, "text", 600))]),
        "friends" => {
            let friends = lim::items(&r, "friends")
                .take(100)
                .map(|f| SessionFriend {
                    name: s(f, "name", 80),
                    code: s(f, "code", 200),
                    release: s(f, "release", 20),
                })
                .collect();
            Some(vec![SessionEvent::Friends(SessionFriends {
                state: s(&r, "state", 30),
                message: s(&r, "message", 600),
                next: s(&r, "next", 600),
                friends,
            })])
        }
        _ => None,
    }
}

/// A running session program.
pub struct PackSession {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    gone: Arc<AtomicBool>,
    view: Arc<Mutex<SessionView>>,
    events: Mutex<Option<Receiver<SessionEvent>>>,
}

impl PackSession {
    /// Starts `<program> session`; `None` when the program is not there or does not start.
    pub fn start(program: &Path) -> Option<Self> {
        if !program.is_file() {
            return None;
        }
        let mut cmd = Command::new(program);
        cmd.arg("session")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(dir) = program.parent() {
            cmd.current_dir(dir);
        }
        process_runner::hide_window(&mut cmd);
        let mut child = process_runner::spawn(&mut cmd).ok()?;
        let (stdin, stdout) = (child.stdin.take(), child.stdout.take()?);
        let (tx, rx) = mpsc::channel();
        let gone = Arc::new(AtomicBool::new(false));
        let view = Arc::new(Mutex::new(SessionView::idle()));
        {
            let (gone, view) = (gone.clone(), view.clone());
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
                    let line = String::from_utf8_lossy(&line).trim_end_matches('\r').to_string();
                    if line.is_empty() || line.len() > MAX_LINE {
                        continue;
                    }
                    for ev in parse_event(&line).unwrap_or_default() {
                        if let SessionEvent::State(v) = &ev {
                            *view.lock().unwrap() = v.clone();
                        }
                        if tx.send(ev).is_err() {
                            return;
                        }
                    }
                }
                gone.store(true, Ordering::SeqCst);
                let _ = tx.send(SessionEvent::Exited);
            });
        }
        Some(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            gone,
            view,
            events: Mutex::new(Some(rx)),
        })
    }

    /// The events, once: the caller reads them on its own thread.
    pub fn take_events(&self) -> Option<Receiver<SessionEvent>> {
        self.events.lock().unwrap().take()
    }

    pub fn alive(&self) -> bool {
        !self.gone.load(Ordering::SeqCst)
    }

    /// The last state the program reported.
    pub fn view(&self) -> SessionView {
        self.view.lock().unwrap().clone()
    }

    fn send(&self, message: Value) {
        if !self.alive() {
            return;
        }
        let mut guard = self.stdin.lock().unwrap();
        if let Some(stdin) = guard.as_mut() {
            if writeln!(stdin, "{message}").and_then(|_| stdin.flush()).is_err() {
                self.gone.store(true, Ordering::SeqCst);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn configure(
        &self,
        game_exe: &str,
        host_port: i32,
        info_port: i32,
        version_port: i32,
        steam_app_id: u32,
        allow_steam: bool,
        relay_reuse: bool,
        connect_gate: bool,
        inject_delay_seconds: i32,
        menu_takeover: bool,
        agent_path: &str,
        relay_path: &str,
        dll_path: &str,
        start_saves: &str,
    ) {
        self.send(
            json!({"t": "config", "gameExe": game_exe, "hostPort": host_port, "infoPort": info_port, "versionPort": version_port,
            "steamAppId": steam_app_id, "allowSteam": allow_steam, "relayReuse": relay_reuse, "connectGate": connect_gate,
            "injectDelaySeconds": inject_delay_seconds, "menuTakeover": menu_takeover, "agentPath": agent_path, "relayPath": relay_path,
            "dllPath": dll_path, "startSaves": start_saves}),
        );
    }
    pub fn relay(&self) {
        self.send(json!({"t": "relay"}));
    }
    pub fn relay_stop(&self) {
        self.send(json!({"t": "relay_stop"}));
    }
    pub fn launch(&self, ip: &str, port: i32, steam_code: Option<&str>, ping_ms: i32, skip_mod_check: bool) {
        self.send(json!({"t": "launch", "ip": ip, "port": port, "steam": steam_code.unwrap_or(""), "ping": ping_ms, "skipModCheck": skip_mod_check}));
    }
    pub fn connect(&self) {
        self.send(json!({"t": "connect"}));
    }
    pub fn choice(&self, choice: &str) {
        self.send(json!({"t": "choice", "choice": choice}));
    }
    pub fn fallback(&self, host: &str, port: i32) {
        self.send(json!({"t": "fallback", "host": host, "port": port}));
    }
    pub fn cancel(&self) {
        self.send(json!({"t": "cancel"}));
    }
    pub fn ask_friends(&self) {
        self.send(json!({"t": "friends"}));
    }
    pub fn test_connection(&self, ip: &str, port: i32, steam_code: Option<&str>) {
        self.send(json!({"t": "test", "ip": ip, "port": port, "steam": steam_code.unwrap_or("")}));
    }
    pub fn close_game(&self) {
        self.send(json!({"t": "close_game"}));
    }

    /// Asks the program to stop, closes its input and ends it if it does not go within two seconds.
    pub fn stop(&self) {
        self.send(json!({"t": "stop"}));
        *self.stdin.lock().unwrap() = None;
        let mut child = self.child.lock().unwrap();
        for _ in 0..40 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for PackSession {
    fn drop(&mut self) {
        if self.alive() {
            self.stop();
        }
    }
}

/// Runs the program once with `args` (for example `friends --steam-app N`) and gives every event to `on_event`; `false` when it did not run or
/// was stopped at the timeout.
pub fn run_once(
    program: &Path,
    args: &[String],
    on_event: &mut dyn FnMut(SessionEvent),
    timeout: Option<Duration>,
    cancel: &AtomicBool,
) -> bool {
    let outcome = process_runner::run_lines(
        program,
        args,
        timeout.unwrap_or(Duration::from_secs(90)),
        MAX_LINE,
        &mut |line| {
            for ev in parse_event(line).unwrap_or_default() {
                on_event(ev);
            }
        },
        cancel,
    );
    outcome.start_error.is_none() && !outcome.timed_out && !outcome.cancelled
}

/// The Steam friends who are hosting this game (the Home page asks every minute).
pub fn friends_once(program: &Path, steam_app_id: u32, game_exe: &str, cancel: &AtomicBool) -> Option<SessionFriends> {
    let mut args = vec!["friends".to_string(), "--steam-app".into(), steam_app_id.to_string()];
    if !game_exe.trim().is_empty() && game_exe.chars().count() <= 260 && !game_exe.chars().any(char::is_control) {
        args.push("--game-exe".into());
        args.push(game_exe.to_string());
    }
    let mut got = None;
    run_once(
        program,
        &args,
        &mut |ev| {
            if let SessionEvent::Friends(f) = ev {
                got = Some(f);
            }
        },
        Some(Duration::from_secs(40)),
        cancel,
    );
    got
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> SessionEvent {
        parse_event(line).unwrap().remove(0)
    }

    #[test]
    fn a_state_is_read_with_its_defaults() {
        let SessionEvent::State(v) = one(
            r#"{"t":"state","stage":"connected","status":"Playing","connect":{"enabled":false,"reason":"wait"},"conn":{"text":"23 ms","bad":true},
                "join":{"state":"joining","message":"m","canFresh":true},"host":{"relay":true,"steamState":"ready","steamCode":"abc","error":"e","refused":"r"},"game":true}"#,
        ) else {
            panic!()
        };
        assert_eq!(
            (v.stage.as_str(), v.status.as_str(), v.connect_enabled, v.connect_reason.as_str()),
            ("connected", "Playing", false, "wait")
        );
        assert_eq!(
            (v.conn_text.as_str(), v.conn_bad, v.join_state.as_str(), v.join_can_fresh),
            ("23 ms", true, "joining", true)
        );
        assert_eq!(
            (
                v.host.relay_up,
                v.host.steam_state.as_str(),
                v.host.steam_code.as_deref(),
                v.game_running
            ),
            (true, "ready", Some("abc"), true)
        );
        let SessionEvent::State(idle) = one(r#"{"t":"state","stage":"idle"}"#) else {
            panic!()
        };
        assert_eq!(idle, SessionView::idle());
        assert!(parse_event(r#"{"t":"state","stage":"exploding"}"#).is_none());
    }

    #[test]
    fn problems_messages_and_the_address_fallback() {
        let evs =
            parse_event(r#"{"t":"problem","title":"T","text":"X","next":"N","reportBug":true,"launchAnyway":true,"log":"why"}"#).unwrap();
        assert_eq!(evs.len(), 2);
        assert!(matches!(&evs[0], SessionEvent::Problem(p) if p.report_bug && p.launch_anyway && !p.address_fallback && p.title == "T"));
        assert_eq!(evs[1], SessionEvent::Log("session: why".into()));
        assert_eq!(
            one(r#"{"t":"message","title":"T","text":"X","next":"N"}"#),
            SessionEvent::Message {
                title: "T".into(),
                text: "X".into(),
                next: "N".into()
            }
        );
        assert!(
            matches!(one(r#"{"t":"message","title":"T","text":"X","addressFallback":true}"#), SessionEvent::Problem(p) if p.address_fallback)
        );
    }

    #[test]
    fn small_events_are_read_and_empty_ones_dropped() {
        assert_eq!(one(r#"{"t":"error","text":"Bad"}"#), SessionEvent::Error("Bad".into()));
        assert!(parse_event(r#"{"t":"error","text":""}"#).unwrap().is_empty());
        assert_eq!(one(r#"{"t":"toast","text":"Hi"}"#), SessionEvent::Toast("Hi".into()));
        assert_eq!(one(r#"{"t":"log","line":"x"}"#), SessionEvent::Log("session: x".into()));
        assert_eq!(one(r#"{"t":"game_exit"}"#), SessionEvent::GameExit);
        assert_eq!(
            one(r#"{"t":"refused","text":"No","key":"k"}"#),
            SessionEvent::Refused {
                text: "No".into(),
                key: "k".into()
            }
        );
        assert_eq!(
            one(r#"{"t":"version_mismatch","text":"Old"}"#),
            SessionEvent::VersionMismatch("Old".into())
        );
        let SessionEvent::Game(g) = one(r#"{"t":"game","pid":42,"startedAt":"2026-10-12T10:30:00Z","hosting":true}"#) else {
            panic!()
        };
        assert_eq!((g.pid, g.started, g.hosting), (42, 1_791_801_000, true));
        assert!(parse_event(r#"{"t":"game","pid":0}"#).is_none());
        assert!(parse_event(r#"{"t":"game","pid":"4"}"#).is_none());
    }

    #[test]
    fn friends_are_capped_and_cleaned() {
        let list: Vec<String> = (0..150)
            .map(|i| format!(r#"{{"name":"F{i}","code":"c{i}","release":"0.1.0"}}"#))
            .collect();
        let SessionEvent::Friends(f) = one(&format!(
            r#"{{"t":"friends","state":"ok","message":"m","next":"n","friends":[{}]}}"#,
            list.join(",")
        )) else {
            panic!()
        };
        assert_eq!((f.friends.len(), f.state.as_str()), (100, "ok"));
        assert_eq!(
            f.friends[0],
            SessionFriend {
                name: "F0".into(),
                code: "c0".into(),
                release: "0.1.0".into()
            }
        );
        let SessionEvent::Toast(t) = one(r#"{"t":"toast","text":"<b>hi</b>\u0007"}"#) else {
            panic!()
        };
        assert_eq!(t, "bhi/b");
    }

    #[test]
    fn anything_else_is_ignored() {
        for l in ["not json", "[]", r#"{"t":5}"#, r#"{"t":"unknown"}"#, "{}"] {
            assert!(parse_event(l).is_none(), "{l}");
        }
    }

    #[cfg(unix)]
    mod unix {
        use super::super::*;
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::path::PathBuf;

        fn program(name: &str, body: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("cairn-sess-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            let p = dir.join("tool.sh");
            fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            p
        }

        #[test]
        fn a_session_talks_both_ways_and_ends() {
            // prints a state, then answers every command it reads with a toast, and stops on "stop"
            let p = program(
                "talk",
                r#"echo '{"t":"state","stage":"waiting-connect","status":"Ready"}'
while read line; do
  case "$line" in
    *'"t":"stop"'*) exit 0;;
    *'"t":"relay"'*) echo '{"t":"toast","text":"relay asked"}';;
  esac
done"#,
            );
            let s = PackSession::start(&p).unwrap();
            let rx = s.take_events().unwrap();
            assert!(s.take_events().is_none());
            assert!(matches!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), SessionEvent::State(v) if v.status == "Ready"));
            assert_eq!(s.view().stage, "waiting-connect");
            s.relay();
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                SessionEvent::Toast("relay asked".into())
            );
            s.stop();
            assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), SessionEvent::Exited);
            assert!(!s.alive());
            s.relay(); // sending to a program that is gone does nothing
        }

        #[test]
        fn a_missing_program_gives_no_session() {
            assert!(PackSession::start(Path::new("/no/such/program")).is_none());
        }

        #[test]
        fn the_friends_of_a_game_are_asked_once() {
            let p = program(
                "friends",
                r#"echo "{\"t\":\"friends\",\"state\":\"ok\",\"friends\":[{\"name\":\"Ana\",\"code\":\"x\"}]}""#,
            );
            let f = friends_once(&p, 881100, "", &AtomicBool::new(false)).unwrap();
            assert_eq!(f.friends[0].name, "Ana");
            let none = program("nofriends", "true");
            assert!(friends_once(&none, 1, "", &AtomicBool::new(false)).is_none());
        }

        #[test]
        fn a_run_once_that_hangs_is_stopped() {
            let p = program("hang", "sleep 30");
            assert!(!run_once(
                &p,
                &[],
                &mut |_| {},
                Some(Duration::from_millis(200)),
                &AtomicBool::new(false)
            ));
        }
    }
}
