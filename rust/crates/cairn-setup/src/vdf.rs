// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! A forgiving reader for Valve's text format (`libraryfolders.vdf`, `appmanifest_*.acf`, `loginusers.vdf`): a flat list of key/value
//! pairs with the depth each was found at. A truncated or malformed file gives what came before the damage and never an error.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub key: String,
    pub value: String,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open,
    Close,
    Text,
}

pub fn pairs(text: &str) -> Vec<Pair> {
    let tokens = tokens(text);
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < tokens.len() {
        let (kind, value) = &tokens[i];
        match kind {
            Kind::Open => depth += 1,
            Kind::Close => depth = depth.saturating_sub(1),
            // a string: a pair when the next token is a string, a section name when it is '{'
            Kind::Text => {
                if let Some((Kind::Text, v)) = tokens.get(i + 1) {
                    out.push(Pair {
                        key: value.clone(),
                        value: v.clone(),
                        depth,
                    });
                    i += 1;
                }
            }
        }
        i += 1;
    }
    out
}

/// The value of the first pair with that key (compared without case).
pub fn value<'a>(pairs: &'a [Pair], key: &str) -> Option<&'a str> {
    pairs.iter().find(|p| p.key.eq_ignore_ascii_case(key)).map(|p| p.value.as_str())
}

fn tokens(text: &str) -> Vec<(Kind, String)> {
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    let mut list = Vec::new();
    let mut i = 0;
    while i < n {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && i + 1 < n && c[i + 1] == '/' {
            while i < n && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '{' || ch == '}' {
            list.push((if ch == '{' { Kind::Open } else { Kind::Close }, String::new()));
            i += 1;
        } else if ch == '"' {
            i += 1;
            let mut s = String::new();
            let mut closed = false;
            while i < n {
                let d = c[i];
                if d == '\\' && i + 1 < n {
                    s.push(match c[i + 1] {
                        'n' => '\n',
                        't' => '\t',
                        e => e,
                    });
                    i += 2;
                    continue;
                }
                if d == '"' {
                    closed = true;
                    i += 1;
                    break;
                }
                s.push(d);
                i += 1;
            }
            if !closed {
                break; // truncated mid-string: keep what came before it
            }
            list.push((Kind::Text, s));
        } else {
            // an unquoted token (rare; Steam itself never writes them)
            let start = i;
            while i < n && !c[i].is_whitespace() && c[i] != '{' && c[i] != '}' && c[i] != '"' {
                i += 1;
            }
            list.push((Kind::Text, c[start..i].iter().collect()));
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_folders_give_their_paths_and_depths() {
        let text = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"1771300\"\t\t\"123\"\n\t\t}\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        let p = pairs(text);
        let paths: Vec<&str> = p.iter().filter(|x| x.key == "path").map(|x| x.value.as_str()).collect();
        assert_eq!(paths, vec!["C:\\Program Files (x86)\\Steam", "D:\\SteamLibrary"]);
        assert_eq!(p.iter().find(|x| x.key == "1771300").unwrap().depth, 3);
    }

    #[test]
    fn a_truncated_file_keeps_what_came_before() {
        let p = pairs("\"AppState\"\n{\n\t\"appid\"\t\"2429020\"\n\t\"installdir\"\t\"KCD2");
        assert_eq!(value(&p, "appid"), Some("2429020"));
        assert_eq!(value(&p, "installdir"), None);
    }

    #[test]
    fn garbage_does_not_panic() {
        pairs("{{{ this is not a vdf \" \" \"path\" oops");
        pairs("\"unterminated");
        assert!(pairs("").is_empty());
    }

    #[test]
    fn comments_and_quotes_inside_values() {
        let p = pairs("// header\n\"name\" \"a \\\"quoted\\\" name\" // trailing\n");
        assert_eq!(value(&p, "NAME"), Some("a \"quoted\" name"));
    }
}
