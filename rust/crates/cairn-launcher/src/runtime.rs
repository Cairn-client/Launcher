// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! What is running on this PC: the game, Steam, and whatever holds the host port.

use std::path::Path;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, Signal, System, UpdateKind};

/// A process of this name (without `.exe`, compared without case) is running; its id.
pub fn find_process(exe_stem: &str) -> Option<u32> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let want = exe_stem.trim_end_matches(".exe").to_lowercase();
    sys.processes()
        .values()
        .find(|p| process_stem(p.name().to_string_lossy().as_ref()) == want)
        .map(|p| p.pid().as_u32())
}

/// A process running from this file.
pub fn find_process_of(exe: &Path) -> Option<u32> {
    exe.file_stem().and_then(|s| find_process(&s.to_string_lossy()))
}

fn process_stem(name: &str) -> String {
    name.trim_end_matches(".exe").trim_end_matches(".EXE").to_lowercase()
}

/// Steam is running (its client process, `steam` / `steam.exe`).
pub fn steam_running() -> bool {
    find_process("steam").is_some()
}

/// Closes a process and what it started. `false` when it could not.
pub fn kill(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let Some(p) = sys.process(sysinfo::Pid::from_u32(pid)) else {
        return true;
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    p.kill_with(Signal::Kill).unwrap_or(false) || p.kill()
}

/// The program that is listening on a TCP port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortOwner {
    pub pid: u32,
    pub name: String,
}

/// The process id listening on `port` in the output of `netstat -ano -p tcp`; `None` when nothing listens.
pub fn parse_listening_pid(netstat: &str, port: u16) -> Option<u32> {
    for raw in netstat.lines() {
        let p: Vec<&str> = raw.split_whitespace().collect();
        if p.len() < 5
            || !p[0].eq_ignore_ascii_case("TCP")
            || !p[1].ends_with(&format!(":{port}"))
            || !p[3].eq_ignore_ascii_case("LISTENING")
        {
            continue;
        }
        if let Ok(pid) = p[4].parse() {
            return Some(pid);
        }
    }
    None
}

/// Windows itself and this launcher are never offered for closing.
pub fn is_protected(pid: u32) -> bool {
    pid <= 4 || pid == std::process::id()
}

/// Who holds the port (the ready check's "close it" button); `None` when it is free or the question cannot be asked.
pub fn port_owner(port: u16) -> Option<PortOwner> {
    let out = std::process::Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let pid = parse_listening_pid(&String::from_utf8_lossy(&out.stdout), port)?;
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let name = sys.process(sysinfo::Pid::from_u32(pid))?.name().to_string_lossy().into_owned();
    Some(PortOwner { pid, name })
}

/// Whether nothing on this PC is listening on the port: tried by binding it.
pub fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETSTAT: &str = "\n  Proto  Local Address          Foreign Address        State           PID\n  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1032\n  TCP    0.0.0.0:7778           0.0.0.0:0              LISTENING       4242\n  TCP    10.0.0.5:51234         52.1.2.3:443           ESTABLISHED     999\n  TCP    [::]:77780             [::]:0                 LISTENING       1\n  UDP    0.0.0.0:7778           *:*                                    77\n";

    #[test]
    fn the_listener_of_a_port_is_read_from_netstat() {
        assert_eq!(parse_listening_pid(NETSTAT, 7778), Some(4242));
        assert_eq!(parse_listening_pid(NETSTAT, 135), Some(1032));
        assert_eq!(parse_listening_pid(NETSTAT, 51234), None); // connected, not listening
        assert_eq!(parse_listening_pid(NETSTAT, 778), None); // a port that only ends alike
        assert_eq!(parse_listening_pid("", 80), None);
    }

    #[test]
    fn windows_and_the_launcher_are_never_closed() {
        assert!(is_protected(0) && is_protected(4) && is_protected(std::process::id()));
        assert!(!is_protected(4242));
    }

    #[test]
    fn a_process_that_is_not_there_is_not_found() {
        assert!(find_process("definitely-not-a-running-program").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_running_program_is_found_by_its_name_and_can_be_closed() {
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        assert_eq!(find_process("sleep"), Some(pid).or_else(|| find_process("sleep")));
        assert!(find_process("sleep").is_some());
        assert!(kill(pid));
        let _ = child.wait();
        assert!(kill(pid)); // gone already: nothing to do is a success
    }

    #[test]
    fn a_taken_port_is_not_free() {
        let l = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(!port_is_free(port));
        drop(l);
        assert!(port_is_free(port));
    }
}
