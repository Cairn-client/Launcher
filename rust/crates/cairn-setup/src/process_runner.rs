// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Runs a pack's program the one safe way: its arguments are a list (never a command line built from text), its standard output is read
//! line by line, and it is stopped at a timeout or when the player cancels.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How a run ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub timed_out: bool,
    pub cancelled: bool,
    pub start_error: Option<String>,
}

impl Outcome {
    fn failed(msg: impl Into<String>) -> Self {
        Self {
            start_error: Some(msg.into()),
            ..Self::default()
        }
    }
}

/// Starts `program` in its own folder with `args` and calls `on_line` for each line of output that is not empty and not longer than `max_line`.
pub fn run_lines(
    program: &Path,
    args: &[String],
    timeout: Duration,
    max_line: usize,
    on_line: &mut dyn FnMut(&str),
    cancel: &AtomicBool,
) -> Outcome {
    if !program.is_file() {
        return Outcome::failed("The program of the mod is not there.");
    }
    let mut cmd = Command::new(program);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());
    if let Some(dir) = program.parent() {
        cmd.current_dir(dir);
    }
    hide_window(&mut cmd);
    let mut child = match spawn(&mut cmd) {
        Ok(c) => c,
        Err(e) => return Outcome::failed(format!("The program of the mod did not start ({e}).")),
    };
    let Some(stdout) = child.stdout.take() else {
        return Outcome::failed("The program of the mod did not start.");
    };
    let (tx, rx) = mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
            if tx.send(String::from_utf8_lossy(&line).trim_end_matches('\r').to_string()).is_err() {
                break;
            }
        }
    });

    let started = Instant::now();
    let mut out = Outcome::default();
    loop {
        if cancel.load(Ordering::Relaxed) {
            out.cancelled = true;
            break;
        }
        if started.elapsed() >= timeout {
            out.timed_out = true;
            break;
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                if !line.is_empty() && line.len() <= max_line {
                    on_line(&line);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break, // the program closed its output: it is done
        }
    }
    if out.timed_out || out.cancelled {
        kill_tree(&mut child);
    }
    let _ = child.wait();
    // a program stopped early may have left a child of its own holding the output open: do not wait for it
    if !(out.timed_out || out.cancelled) {
        let _ = reader.join();
    }
    out
}

/// A command that does not flash a console window on Windows.
pub fn hide_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Stops the program and, on Windows, everything it started.
pub fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// Starts the command; a program that was only just written can be "busy" for a moment while another thread forks, so that is retried.
pub fn spawn(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    let mut tries = 0;
    loop {
        match cmd.spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 20 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            other => return other,
        }
    }
}
