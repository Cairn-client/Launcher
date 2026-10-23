// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Who is signed in to Steam on this PC, and their friends, read from Steam's own files: `config/loginusers.vdf` and
//! `userdata/<account>/config/localconfig.vdf`. Nothing is sent anywhere by this module.

use std::fs;
use std::path::Path;

/// The 64-bit id of account 0; a friend's folder name is the account number to add to it.
pub const INDIVIDUAL_BASE: u64 = 76_561_197_960_265_728;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub id: String,
    pub name: String,
    pub avatar: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    pub id: String,
    pub name: String,
    pub friends: Vec<String>,
    pub named: Vec<Named>,
    pub avatar: String,
}

/// The signed-in account and its friends; `None` when Steam's files are not there or have no account.
pub fn read(steam_root: Option<&Path>) -> Option<Local> {
    let root = steam_root.filter(|r| !r.as_os_str().is_empty())?;
    let users = fs::read(root.join("config").join("loginusers.vdf")).ok()?;
    let (id, name) = most_recent_user(&String::from_utf8_lossy(&users))?;
    let account = id.parse::<u64>().ok()?.checked_sub(INDIVIDUAL_BASE)?;
    let mut friends = Vec::new();
    let mut named = Vec::new();
    let mut avatar = String::new();
    let cfg = root
        .join("userdata")
        .join(account.to_string())
        .join("config")
        .join("localconfig.vdf");
    if let Ok(bytes) = fs::read(&cfg) {
        let text = String::from_utf8_lossy(&bytes);
        friends = friend_ids(&text);
        let all = friend_names(&text);
        // Steam lists the signed-in account beside its friends
        avatar = all.iter().find(|n| n.id == id).map(|n| n.avatar.clone()).unwrap_or_default();
        named = all.into_iter().filter(|n| n.id != id).collect();
    }
    Some(Local {
        id,
        name,
        friends,
        named,
        avatar,
    })
}

/// The account marked `MostRecent`, or else the one signed in last (the newest `Timestamp`): newer Steam clients no longer write the flag.
pub fn most_recent_user(vdf: &str) -> Option<(String, String)> {
    let root = Node::parse(vdf);
    let users = root.child("users")?;
    let mut newest: Option<(String, String)> = None;
    let mut newest_at = i64::MIN;
    for u in &users.children {
        let name = u.value("PersonaName").unwrap_or("");
        if u.key.parse::<u64>().is_err() || name.is_empty() {
            continue;
        }
        if u.value("MostRecent") == Some("1") {
            return Some((u.key.clone(), name.to_string()));
        }
        let at = u.value("Timestamp").and_then(|t| t.parse::<i64>().ok()).unwrap_or(0);
        if at > newest_at {
            newest_at = at;
            newest = Some((u.key.clone(), name.to_string()));
        }
    }
    newest
}

/// The friends' 64-bit ids: the numbered sections of `friends`.
pub fn friend_ids(vdf: &str) -> Vec<String> {
    let root = Node::parse(vdf);
    let Some(friends) = root.find("friends") else { return Vec::new() };
    friends
        .children
        .iter()
        .filter_map(|f| f.key.parse::<u32>().ok().filter(|a| *a != 0))
        .map(|a| (INDIVIDUAL_BASE + a as u64).to_string())
        .collect()
}

pub fn friend_names(vdf: &str) -> Vec<Named> {
    let root = Node::parse(vdf);
    let Some(friends) = root.find("friends") else { return Vec::new() };
    let mut list = Vec::new();
    for f in &friends.children {
        if f.text.is_some() {
            continue;
        }
        let Some(account) = f.key.parse::<u32>().ok().filter(|a| *a != 0) else {
            continue;
        };
        let id = (INDIVIDUAL_BASE + account as u64).to_string();
        let name = f.value("name").unwrap_or("");
        list.push(Named {
            name: if name.is_empty() { id.clone() } else { name.to_string() },
            id,
            avatar: f.value("avatar").unwrap_or("").to_string(),
        });
    }
    list
}

/// Valve's text format as a tree: a section has children, a pair has text.
struct Node {
    key: String,
    text: Option<String>,
    children: Vec<Node>,
}

impl Node {
    fn value(&self, key: &str) -> Option<&str> {
        self.children
            .iter()
            .find(|c| c.text.is_some() && c.key.eq_ignore_ascii_case(key))
            .and_then(|c| c.text.as_deref())
    }

    fn child(&self, key: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.text.is_none() && c.key.eq_ignore_ascii_case(key))
    }

    fn find(&self, key: &str) -> Option<&Node> {
        if self.text.is_none() && self.key.eq_ignore_ascii_case(key) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(key))
    }

    fn parse(text: &str) -> Node {
        let s: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut root = Node {
            key: String::new(),
            text: None,
            children: Vec::new(),
        };
        read_section(&s, &mut i, &mut root, 0);
        root
    }
}

const MAX_DEPTH: usize = 64;

fn read_section(s: &[char], i: &mut usize, into: &mut Node, depth: usize) {
    loop {
        let Some(key) = token(s, i) else { return };
        if key == "}" {
            return;
        }
        skip(s, i);
        if *i < s.len() && s[*i] == '{' {
            *i += 1;
            let mut child = Node {
                key,
                text: None,
                children: Vec::new(),
            };
            if depth < MAX_DEPTH {
                read_section(s, i, &mut child, depth + 1);
            }
            into.children.push(child);
        } else {
            let Some(value) = token(s, i) else { return };
            into.children.push(Node {
                key,
                text: Some(value),
                children: Vec::new(),
            });
        }
    }
}

fn skip(s: &[char], i: &mut usize) {
    while *i < s.len() {
        if s[*i].is_whitespace() {
            *i += 1;
        } else if s[*i] == '/' && *i + 1 < s.len() && s[*i + 1] == '/' {
            while *i < s.len() && s[*i] != '\n' {
                *i += 1;
            }
        } else {
            break;
        }
    }
}

fn token(s: &[char], i: &mut usize) -> Option<String> {
    skip(s, i);
    if *i >= s.len() {
        return None;
    }
    if s[*i] == '}' {
        *i += 1;
        return Some("}".into());
    }
    if s[*i] == '{' {
        return None;
    }
    if s[*i] != '"' {
        let st = *i;
        while *i < s.len() && !s[*i].is_whitespace() && s[*i] != '{' && s[*i] != '}' {
            *i += 1;
        }
        return Some(s[st..*i].iter().collect());
    }
    *i += 1;
    let mut out = String::new();
    while *i < s.len() && s[*i] != '"' {
        if s[*i] == '\\' && *i + 1 < s.len() {
            *i += 1;
            out.push(match s[*i] {
                'n' => '\n',
                't' => '\t',
                c => c,
            });
            *i += 1;
            continue;
        }
        out.push(s[*i]);
        *i += 1;
    }
    *i += 1;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const USERS: &str = r#"
"users"
{
    "76561198000000010"
    {
        "AccountName"   "old"
        "PersonaName"   "Old Name"
        "MostRecent"    "0"
    }
    "76561198000000001"
    {
        "AccountName"   "alice"
        "PersonaName"   "Alice \"the\" Brave"
        "RememberPassword"  "1"
        "MostRecent"    "1"
    }
}
"#;

    const CONFIG: &str = r#"
"UserLocalConfigStore"
{
    "Software" { "Valve" { "Steam" { "Apps" { "1771300" { "LastPlayed" "5" } } } } }
    "friends"
    {
        "PersonaName"   "Alice"
        "5"
        {
            "name"  "Bob"
            "tag"   ""
        }
        "37"
        {
            "name"  "Carol"
        }
        "0"
        {
            "name"  "nobody"
        }
    }
}
"#;

    #[test]
    fn the_most_recent_account_and_its_name_are_read() {
        let (id, name) = most_recent_user(USERS).unwrap();
        assert_eq!(id, "76561198000000001");
        assert_eq!(name, "Alice \"the\" Brave");
    }

    #[test]
    fn without_a_most_recent_flag_the_newest_sign_in_is_the_account() {
        let u = most_recent_user("\"users\" { \"76561198000000010\" { \"PersonaName\" \"old\" \"Timestamp\" \"100\" } \"76561198000000001\" { \"PersonaName\" \"new\" \"Timestamp\" \"200\" } }");
        assert_eq!(u.unwrap().0, "76561198000000001");
    }

    #[test]
    fn no_accounts_means_nobody() {
        assert!(most_recent_user("\"users\" { }").is_none());
    }

    #[test]
    fn friends_are_the_numbered_sections_as_64_bit_ids() {
        assert_eq!(
            friend_ids(CONFIG),
            vec![(INDIVIDUAL_BASE + 5).to_string(), (INDIVIDUAL_BASE + 37).to_string()]
        );
    }

    #[test]
    fn friend_names_come_with_their_ids() {
        let n = friend_names(CONFIG);
        assert_eq!(n.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), vec!["Bob", "Carol"]);
        assert_eq!(n[0].id, (INDIVIDUAL_BASE + 5).to_string());
    }

    #[test]
    fn a_config_without_friends_has_none() {
        assert!(friend_ids("\"UserLocalConfigStore\" { \"x\" \"y\" }").is_empty());
    }

    #[test]
    fn deeply_nested_garbage_does_not_overflow() {
        let deep = "\"a\" {".repeat(10_000);
        let _ = friend_ids(&deep);
    }

    #[test]
    fn steam_folders_are_read_from_disk() {
        let root = std::env::temp_dir().join(format!("cairn-steamid-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let account = 76_561_198_000_000_001u64 - INDIVIDUAL_BASE;
        fs::create_dir_all(root.join("config")).unwrap();
        fs::create_dir_all(root.join("userdata").join(account.to_string()).join("config")).unwrap();
        fs::write(root.join("config").join("loginusers.vdf"), USERS).unwrap();
        fs::write(
            root.join("userdata")
                .join(account.to_string())
                .join("config")
                .join("localconfig.vdf"),
            CONFIG,
        )
        .unwrap();
        let me = read(Some(&root)).unwrap();
        assert_eq!(me.id, "76561198000000001");
        assert_eq!(me.friends.len(), 2);
        assert!(read(Some(&root.join("missing"))).is_none());
        assert!(read(None).is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
