// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The server's settings: `appsettings.json` beside the program (the same file the earlier .NET server read, comments allowed), then the
//! environment (`ASPNETCORE_URLS`) and the command line (`--urls`).

use serde_json::Value;
use std::net::IpAddr;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The addresses to listen on, e.g. `http://0.0.0.0:5100`.
    pub urls: Vec<String>,
    pub max_servers: usize,
    /// Believe `X-Forwarded-For` from a trusted proxy (off by default: from anyone else it would let a caller list a server under any address).
    pub forwarded_enabled: bool,
    pub known_proxies: Vec<IpAddr>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            urls: vec!["http://0.0.0.0:5100".into()],
            max_servers: 500,
            forwarded_enabled: false,
            known_proxies: vec![],
        }
    }
}

/// JSON with `//` comments (outside strings) and `/* */` comments taken out, as .NET's configuration reader allows.
pub fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut chars, mut in_string, mut escaped) = (text.chars().peekable(), false, false);
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

impl Config {
    /// The settings in `json` over the defaults; text that is not JSON leaves the defaults.
    pub fn from_json(json: &str) -> Self {
        let mut c = Self::default();
        let Ok(v) = serde_json::from_str::<Value>(strip_comments(json.trim_start_matches('\u{feff}')).as_str()) else {
            return c;
        };
        if let Some(u) = v.get("Urls").and_then(Value::as_str) {
            c.urls = split_urls(u);
        }
        if let Some(n) = v.pointer("/Registry/MaxServers").and_then(Value::as_u64) {
            c.max_servers = (n as usize).max(1);
        }
        if let Some(b) = v.pointer("/ForwardedHeaders/Enabled").and_then(Value::as_bool) {
            c.forwarded_enabled = b;
        }
        if let Some(list) = v.pointer("/ForwardedHeaders/KnownProxies").and_then(Value::as_array) {
            for p in list.iter().filter_map(Value::as_str) {
                match p.parse() {
                    Ok(ip) => c.known_proxies.push(ip),
                    Err(_) => eprintln!("warn: ignoring ForwardedHeaders:KnownProxies entry \"{p}\": not an IP address."),
                }
            }
        }
        c
    }

    pub fn load(dir: &Path, env_urls: Option<String>, args: &[String]) -> Self {
        let mut c = std::fs::read_to_string(dir.join("appsettings.json"))
            .map(|t| Self::from_json(&t))
            .unwrap_or_default();
        if let Some(u) = env_urls.filter(|u| !u.trim().is_empty()) {
            c.urls = split_urls(&u);
        }
        if let Some(i) = args.iter().position(|a| a == "--urls") {
            if let Some(u) = args.get(i + 1) {
                c.urls = split_urls(u);
            }
        }
        c
    }

    /// Whether a connection from `peer` is a proxy whose `X-Forwarded-For` is believed (loopback unless named).
    pub fn trusts(&self, peer: IpAddr) -> bool {
        self.forwarded_enabled
            && (peer.to_canonical().is_loopback() || self.known_proxies.iter().any(|p| p.to_canonical() == peer.to_canonical()))
    }
}

fn split_urls(s: &str) -> Vec<String> {
    s.split(';').map(str::trim).filter(|u| !u.is_empty()).map(String::from).collect()
}

/// `http://host:port` as an address to bind (`+` and `*` mean every interface).
pub fn bind_address(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?.trim_end_matches('/');
    let rest = rest.replace(['+', '*'], "0.0.0.0");
    Some(rest.replace("localhost", "127.0.0.1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_settings_file_is_read_with_its_comments() {
        let c = Config::from_json(include_str!("../appsettings.json"));
        assert_eq!(c, Config::default());
    }

    #[test]
    fn settings_override_the_defaults() {
        let c = Config::from_json(
            r#"{ "Urls": "http://127.0.0.1:6000;http://[::1]:6001", "Registry": { "MaxServers": 0 }, // c
            "ForwardedHeaders": { "Enabled": true, "KnownProxies": ["10.0.0.2", "nope"] } }"#,
        );
        assert_eq!(c.urls, vec!["http://127.0.0.1:6000", "http://[::1]:6001"]);
        assert_eq!(c.max_servers, 1);
        assert!(c.forwarded_enabled && c.known_proxies == vec!["10.0.0.2".parse::<IpAddr>().unwrap()]);
        assert!(c.trusts("127.0.0.1".parse().unwrap()) && c.trusts("10.0.0.2".parse().unwrap()) && !c.trusts("8.8.8.8".parse().unwrap()));
        assert!(!Config::default().trusts("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn urls_become_addresses_to_bind() {
        assert_eq!(bind_address("http://0.0.0.0:5100").as_deref(), Some("0.0.0.0:5100"));
        assert_eq!(bind_address("http://localhost:5100/").as_deref(), Some("127.0.0.1:5100"));
        assert_eq!(bind_address("http://+:80").as_deref(), Some("0.0.0.0:80"));
        assert_eq!(bind_address("nonsense"), None);
    }

    #[test]
    fn the_environment_and_the_command_line_win() {
        let dir = std::env::temp_dir().join("cairn-master-no-such-dir");
        let c = Config::load(&dir, Some("http://0.0.0.0:7000".into()), &[]);
        assert_eq!(c.urls, vec!["http://0.0.0.0:7000"]);
        let c = Config::load(
            &dir,
            Some("http://0.0.0.0:7000".into()),
            &["--urls".into(), "http://0.0.0.0:7001".into()],
        );
        assert_eq!(c.urls, vec!["http://0.0.0.0:7001"]);
    }
}
