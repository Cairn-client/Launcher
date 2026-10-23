// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Takes the player's identity out of text that leaves the PC (a bug report, saved logs): Steam ids, the user name, the PC name and the
//! Windows profile folder.

use regex::{Regex, RegexBuilder};
use std::sync::OnceLock;

fn steam_ids() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)steamid:\s*\d+|\[U:\d:\d+\]|\b\d{15,20}\b").unwrap())
}

// Any Windows profile, not just this account's: C:\Users\<name>\...  or  C:/Users/<name>/...
fn profile_paths() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?i)\b([A-Z]):[\\/]+Users[\\/]+(?:Public\b|Default\b)?[^\\/:*?"<>|\r\n]+"#).unwrap())
}

/// With this account's own names, from the environment.
pub fn text(text: &str) -> String {
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let machine = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    let profile = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    text_for(text, Some(&user), Some(&machine), Some(&profile))
}

pub fn text_for(text: &str, user_name: Option<&str>, machine_name: Option<&str>, profile: Option<&str>) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut s = steam_ids().replace_all(text, "<steam-id>").into_owned();
    if let Some(profile) = profile.filter(|p| p.chars().count() > 3) {
        s = replace_ignore_case(&s, profile, "%USERPROFILE%");
        s = replace_ignore_case(&s, &profile.replace('\\', "/"), "%USERPROFILE%");
    }
    s = replace_profiles(&s);
    // the machine first: it often contains the user name ("ALEX-DESKTOP")
    s = word(&s, machine_name, "<pc>");
    word(&s, user_name, "<user>")
}

/// C:\Users\<name> becomes C:\Users\<user>, except the shared profiles (Public, Default).
fn replace_profiles(s: &str) -> String {
    profile_paths()
        .replace_all(s, |c: &regex::Captures| {
            let whole = c.get(0).unwrap().as_str();
            let name = whole.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("");
            if name.eq_ignore_ascii_case("Public") || name.eq_ignore_ascii_case("Default") {
                whole.to_string()
            } else {
                format!("{}:\\Users\\<user>", &c[1])
            }
        })
        .into_owned()
}

fn word(s: &str, word: Option<&str>, replacement: &str) -> String {
    let Some(w) = word.filter(|w| w.chars().count() >= 3) else {
        return s.to_string();
    };
    // match the word alone and check its neighbours by hand: a name inside a longer word stays
    let re = RegexBuilder::new(&regex::escape(w)).case_insensitive(true).build().unwrap();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for m in re.find_iter(s) {
        let before_ok = m.start() == 0 || !bytes[m.start() - 1].is_ascii_alphanumeric();
        let after_ok = m.end() == s.len() || !bytes[m.end()].is_ascii_alphanumeric();
        if before_ok && after_ok {
            out.push_str(&s[last..m.start()]);
            out.push_str(replacement);
            last = m.end();
        }
    }
    out.push_str(&s[last..]);
    out
}

fn replace_ignore_case(s: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return s.to_string();
    }
    RegexBuilder::new(&regex::escape(needle))
        .case_insensitive(true)
        .build()
        .map(|re| re.replace_all(s, regex::NoExpand(replacement)).into_owned())
        .unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_output_loses_steam_id_user_and_profile() {
        let s = text_for(
            "SteamInternal_SetMinidumpSteamID:  Caching Steam ID:  76561190000000001 [API loaded no]\n\
             Game installed in:   C:\\Users\\Alex\\Games\\KingdomComeDeliverance2\nuser Alex on ALEX-DESKTOP, [U:1:12345], steamid:9988776655",
            Some("Alex"),
            Some("ALEX-DESKTOP"),
            Some("C:\\Users\\Alex"),
        );
        assert!(!s.contains("76561190000000001"));
        assert!(!s.to_lowercase().contains("alex"));
        assert!(!s.contains("12345"));
        assert!(!s.contains("9988776655"));
        assert!(s.contains("%USERPROFILE%\\Games\\KingdomComeDeliverance2"));
        assert!(s.contains("<steam-id>"));
        assert!(s.contains("<pc>"));
    }

    #[test]
    fn other_profiles_and_forward_slashes_too() {
        let s = text_for("c:/users/someone/AppData and D:\\Users\\other\\x", None, None, None);
        assert!(!s.contains("someone"));
        assert!(!s.contains("other"));
    }

    #[test]
    fn shared_profiles_stay() {
        assert_eq!(
            text_for("C:\\Users\\Public\\Documents", None, None, None),
            "C:\\Users\\Public\\Documents"
        );
    }

    #[test]
    fn steam_library_paths_stay_readable() {
        let p = "Modding tools installed in:   D:\\SteamLibrary\\steamapps\\common\\KCD2Mod";
        assert_eq!(text_for(p, Some("Alex"), Some("PC1"), Some("C:\\Users\\Alex")), p);
    }

    #[test]
    fn short_names_are_not_sprayed_over_text() {
        assert_eq!(text_for("Data paks", Some("Da"), None, None), "Data paks");
    }

    #[test]
    fn a_name_inside_a_longer_word_stays() {
        assert_eq!(text_for("Alexander met Alex.", Some("Alex"), None, None), "Alexander met <user>.");
    }
}
