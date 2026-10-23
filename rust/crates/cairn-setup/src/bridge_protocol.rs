// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The messages between the launcher and a running game's mod (its bridge): one JSON object per line. Everything that comes from the
//! mod is checked here before the launcher shows it: ids are plain, text has no control characters and is cut, lists are capped.

use crate::time_util;
use serde_json::{json, Value};

pub const VERSION: i32 = 1;
pub const PIPE_ENVIRONMENT_VARIABLE: &str = "CAIRN_BRIDGE_PIPE";
/// 100 saves of the longest names and details fit.
pub const MAX_LINE: usize = 48 * 1024;
pub const MAX_ITEMS: usize = 64;
pub const MAX_SAVES: usize = 100;

/// A message from the mod to the launcher.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeIn {
    Hello { protocol: i32, game: String },
    Menu(BridgeMenu),
    Notice { show: bool, text: String },
    Saves(Vec<BridgeSaveSlot>),
    Subtitle { seconds: f64, text: String },
    Toast { text: String, error: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeMenuItem {
    pub id: String,
    pub group: String,
    pub kind: String,
    pub on: Option<bool>,
    pub locked: bool,
    pub value: String,
    pub label: String,
    pub help: String,
    pub gate: String,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeMenu {
    pub title: String,
    pub blocked: String,
    pub note: String,
    pub items: Vec<BridgeMenuItem>,
    pub back: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BridgeSaveSlot {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub kind: String,
    /// Seconds since 1970-01-01 UTC of the save's date, when the mod gave one.
    pub when: Option<i64>,
}

/// One line from the mod; `None` for anything that is not a known good message.
pub fn parse(line: &str) -> Option<BridgeIn> {
    if line.is_empty() || line.len() > MAX_LINE {
        return None;
    }
    let r: Value = serde_json::from_str(line).ok()?;
    if !r.is_object() {
        return None;
    }
    let s = |e: &Value, name: &str, max: usize| e.get(name).and_then(Value::as_str).map(|v| clean(v, max)).unwrap_or_default();
    let is_true = |e: &Value, name: &str| e.get(name) == Some(&Value::Bool(true));
    match s(&r, "t", 20).as_str() {
        "hello" => {
            let protocol = r
                .get("protocol")
                .and_then(Value::as_i64)
                .and_then(|p| i32::try_from(p).ok())
                .filter(|p| *p >= 1)?;
            let game = s(&r, "game", 40);
            is_id(&game).then_some(BridgeIn::Hello { protocol, game })
        }
        "menu" => {
            let arr = r.get("items")?.as_array()?;
            let mut items = Vec::new();
            for e in arr {
                if items.len() >= MAX_ITEMS {
                    break;
                }
                if !e.is_object() {
                    continue;
                }
                let (id, kind) = (s(e, "id", 40), s(e, "kind", 10));
                // a damaged row is left out, not the whole menu
                if !is_id(&id) || !matches!(kind.as_str(), "onoff" | "choice" | "key" | "action" | "info") {
                    continue;
                }
                let on = match e.get("on") {
                    Some(Value::Bool(b)) => Some(*b),
                    _ => None,
                };
                items.push(BridgeMenuItem {
                    id,
                    group: s(e, "group", 40),
                    kind,
                    on,
                    locked: is_true(e, "locked"),
                    value: s(e, "value", 60),
                    label: s(e, "label", 80),
                    help: s(e, "help", 200),
                    gate: s(e, "gate", 4),
                    disabled: is_true(e, "disabled"),
                });
            }
            Some(BridgeIn::Menu(BridgeMenu {
                title: s(&r, "title", 60),
                blocked: s(&r, "blocked", 60),
                note: s(&r, "note", 200),
                items,
                back: is_true(&r, "back"),
            }))
        }
        "notice" => {
            let text = s(&r, "text", 400);
            (!text.is_empty()).then_some(BridgeIn::Notice { show: true, text })
        }
        "notice_hide" => Some(BridgeIn::Notice {
            show: false,
            text: String::new(),
        }),
        "saves" => {
            let arr = r.get("slots")?.as_array()?;
            let mut slots = Vec::new();
            for e in arr {
                if slots.len() >= MAX_SAVES {
                    break;
                }
                if !e.is_object() {
                    continue;
                }
                let (id, name, kind) = (s(e, "id", 60), s(e, "name", 80), s(e, "kind", 10));
                if !is_save_id(&id) || name.is_empty() || !matches!(kind.as_str(), "world" | "character") {
                    continue;
                }
                slots.push(BridgeSaveSlot {
                    id,
                    name,
                    detail: s(e, "detail", 120),
                    kind,
                    when: time_util::parse_utc(&s(e, "when", 40)),
                });
            }
            Some(BridgeIn::Saves(slots))
        }
        "toast" => {
            let text = s(&r, "text", 200);
            (!text.is_empty()).then(|| BridgeIn::Toast {
                text,
                error: is_true(&r, "error"),
            })
        }
        "subtitle" => {
            let text = s(&r, "text", 400);
            let secs = r.get("seconds").and_then(Value::as_f64)?;
            (!text.is_empty()).then(|| BridgeIn::Subtitle {
                seconds: secs.clamp(1.0, 15.0),
                text,
            })
        }
        _ => None,
    }
}

pub fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

pub fn is_save_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 60 && !s.contains("..") && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

/// Text from a bridge is shown on screen: no control characters, no more than the cap (the "^^" line separator stays).
fn clean(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut n = 0;
    for c in s.chars() {
        if n >= max {
            break;
        }
        if c >= ' ' && c != '\u{7f}' {
            out.push(c);
            n += c.len_utf16();
        }
    }
    out.trim().to_string()
}

pub fn welcome() -> String {
    line(json!({"t": "welcome", "protocol": VERSION}))
}
pub fn attach() -> String {
    line(json!({"t": "attach"}))
}
pub fn ask_state() -> String {
    line(json!({"t": "state"}))
}
pub fn ask_saves() -> String {
    line(json!({"t": "saves_ask"}))
}
pub fn delete_save(id: &str) -> Option<String> {
    is_save_id(id).then(|| line(json!({"t": "save_delete", "id": id})))
}
pub fn bye(reason: &str) -> String {
    line(json!({"t": "bye", "reason": clean(reason, 120)}))
}
pub fn back() -> String {
    line(json!({"t": "back"}))
}

/// Press (`enter`) or nudge a menu item; `dir` is -1 for a nudge to the left. Only a plain id and a known verb make a message.
pub fn do_item(id: &str, verb: &str, dir: i32) -> Option<String> {
    if !is_id(id) || !matches!(verb, "enter" | "nudge") {
        return None;
    }
    let mut m = json!({"t": "do", "id": id, "verb": verb});
    if verb == "nudge" && dir < 0 {
        m["dir"] = json!(-1);
    }
    Some(line(m))
}

/// One compact JSON object on one line.
fn line(v: Value) -> String {
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hello_is_read() {
        assert_eq!(
            parse(r#"{"t":"hello","protocol":1,"game":"noita"}"#),
            Some(BridgeIn::Hello {
                protocol: 1,
                game: "noita".into()
            })
        );
    }

    #[test]
    fn anything_that_is_not_a_known_good_message_is_ignored() {
        for l in [
            r#"{"t":"hello","protocol":0,"game":"noita"}"#,
            r#"{"t":"hello","protocol":1,"game":"../evil"}"#,
            r#"{"t":"hello","protocol":"1","game":"noita"}"#,
            r#"{"t":"hello","game":"noita"}"#,
            "not json",
            "[]",
            r#"{"t":"somethingnew"}"#,
            "",
        ] {
            assert_eq!(parse(l), None, "{l}");
        }
    }

    #[test]
    fn a_menu_is_read_row_by_row() {
        let Some(BridgeIn::Menu(m)) = parse(
            r#"{"t":"menu","title":"Noita","blocked":"","note":"Host: Ann","items":[
              {"id":"friendly_fire","group":"Rules","kind":"onoff","on":true,"locked":true,"label":"Friendly fire","help":"Players can hurt each other","gate":"f3"},
              {"id":"leave","group":"Session","kind":"action","label":"Leave"},
              {"id":"ping","kind":"info","value":"23 ms","label":"Ping"}]}"#,
        ) else {
            panic!("a menu")
        };
        assert_eq!((m.title.as_str(), m.note.as_str(), m.items.len()), ("Noita", "Host: Ann", 3));
        assert_eq!(m.items[0].on, Some(true));
        assert!(m.items[0].locked);
        assert_eq!(m.items[0].gate, "f3");
        assert_eq!(m.items[1].on, None);
        assert_eq!(m.items[2].value, "23 ms");
    }

    #[test]
    fn a_damaged_row_is_left_out_not_the_whole_menu() {
        let Some(BridgeIn::Menu(m)) = parse(
            r#"{"t":"menu","items":[{"id":"a b","kind":"onoff","label":"bad id"},{"id":"x","kind":"slider","label":"bad kind"},{"id":"ok","kind":"action","label":"Fine"}]}"#,
        ) else {
            panic!()
        };
        assert_eq!(m.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["ok"]);
    }

    #[test]
    fn the_menu_is_capped_and_text_is_cleaned() {
        let rows: Vec<String> = (0..200)
            .map(|i| format!(r#"{{"id":"r{i}","kind":"action","label":"L{i}"}}"#))
            .collect();
        let Some(BridgeIn::Menu(m)) = parse(&format!(r#"{{"t":"menu","items":[{}]}}"#, rows.join(","))) else {
            panic!()
        };
        assert_eq!(m.items.len(), MAX_ITEMS);
        let Some(BridgeIn::Menu(m)) = parse(&format!(
            r#"{{"t":"menu","items":[{{"id":"a","kind":"action","label":"{}"}}]}}"#,
            "x".repeat(500)
        )) else {
            panic!()
        };
        assert_eq!(m.items[0].label.len(), 80);
        let Some(BridgeIn::Menu(m)) = parse(r#"{"t":"menu","items":[{"id":"a","kind":"action","label":"a\u0007b\nc"}]}"#) else {
            panic!()
        };
        assert_eq!(m.items[0].label, "abc"); // control characters never reach the screen
    }

    #[test]
    fn a_line_bigger_than_the_limit_is_refused() {
        assert_eq!(parse(&format!(r#"{{"t":"notice","text":"{}"}}"#, "x".repeat(MAX_LINE))), None);
    }

    #[test]
    fn notices_and_subtitles_are_read_and_limited() {
        assert_eq!(
            parse(r#"{"t":"notice","text":"Ann joined^^Welcome"}"#),
            Some(BridgeIn::Notice {
                show: true,
                text: "Ann joined^^Welcome".into()
            })
        );
        assert_eq!(
            parse(r#"{"t":"notice_hide"}"#),
            Some(BridgeIn::Notice {
                show: false,
                text: String::new()
            })
        );
        assert_eq!(parse(r#"{"t":"notice","text":""}"#), None);
        assert_eq!(
            parse(r#"{"t":"subtitle","seconds":900,"text":"Hello"}"#),
            Some(BridgeIn::Subtitle {
                seconds: 15.0,
                text: "Hello".into()
            })
        );
        assert_eq!(
            parse(r#"{"t":"subtitle","seconds":0,"text":"Hello"}"#),
            Some(BridgeIn::Subtitle {
                seconds: 1.0,
                text: "Hello".into()
            })
        );
        assert_eq!(parse(r#"{"t":"subtitle","text":"no seconds"}"#), None);
    }

    #[test]
    fn the_launchers_messages_are_one_line_of_json() {
        assert_eq!(welcome(), r#"{"t":"welcome","protocol":1}"#);
        assert_eq!(attach(), r#"{"t":"attach"}"#);
        assert_eq!(ask_state(), r#"{"t":"state"}"#);
        assert_eq!(
            do_item("friendly_fire", "nudge", 1).unwrap(),
            r#"{"t":"do","id":"friendly_fire","verb":"nudge"}"#
        );
        assert_eq!(bye("too new"), r#"{"t":"bye","reason":"too new"}"#);
        for l in [welcome(), attach(), bye("a\nb")] {
            assert!(!l.contains('\n'));
            assert!(serde_json::from_str::<Value>(&l).is_ok());
        }
    }

    #[test]
    fn a_do_call_never_carries_anything_but_a_plain_id_and_a_known_verb() {
        for (id, verb) in [("a b", "enter"), ("a\"}", "enter"), ("ok", "delete"), ("", "enter")] {
            assert_eq!(do_item(id, verb, 1), None);
        }
    }

    #[test]
    fn the_saves_are_read_slot_by_slot() {
        let Some(BridgeIn::Saves(s)) = parse(
            r#"{"t":"saves","slots":[
              {"id":"run-12","name":"Ann's run","detail":"depth 4, 3 perks","kind":"world","when":"2026-10-12T10:30:00Z"},
              {"id":"ann","name":"Ann","kind":"character"},
              {"id":"../x","name":"escapes","kind":"world"},
              {"id":"ok","name":"","kind":"world"},
              {"id":"ok2","name":"Strange","kind":"sandwich"}]}"#,
        ) else {
            panic!()
        };
        assert_eq!(s.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["run-12", "ann"]); // unsafe ids, nameless and unknown-kind slots are left out
        assert_eq!(s[0].when, Some(1_791_801_000));
        assert_eq!(s[1].when, None);
        assert_eq!(s[1].kind, "character");
    }

    #[test]
    fn the_saves_list_is_capped() {
        let rows: Vec<String> = (0..300)
            .map(|i| format!(r#"{{"id":"s{i}","name":"S{i}","kind":"world"}}"#))
            .collect();
        let Some(BridgeIn::Saves(s)) = parse(&format!(r#"{{"t":"saves","slots":[{}]}}"#, rows.join(","))) else {
            panic!()
        };
        assert_eq!(s.len(), MAX_SAVES);
        assert_eq!(parse(r#"{"t":"saves"}"#), None);
    }

    #[test]
    fn the_save_requests_are_built_only_from_plain_ids() {
        assert_eq!(ask_saves(), r#"{"t":"saves_ask"}"#);
        assert!(delete_save("run-12").unwrap().contains("run-12"));
        assert_eq!(delete_save("../run"), None);
        assert_eq!(delete_save("a b"), None);
        assert_eq!(delete_save(""), None);
    }

    #[test]
    fn a_toast_is_a_short_message_for_the_player() {
        assert_eq!(
            parse(r#"{"t":"toast","text":"Deleted x."}"#),
            Some(BridgeIn::Toast {
                text: "Deleted x.".into(),
                error: false
            })
        );
        assert_eq!(
            parse(r#"{"t":"toast","text":"No.","error":true}"#),
            Some(BridgeIn::Toast {
                text: "No.".into(),
                error: true
            })
        );
        assert_eq!(parse(r#"{"t":"toast","text":""}"#), None);
        let Some(BridgeIn::Toast { text, .. }) = parse(&format!(r#"{{"t":"toast","text":"{}"}}"#, "x".repeat(900))) else {
            panic!()
        };
        assert_eq!(text.len(), 200);
    }

    #[test]
    fn a_submenu_and_a_nudges_direction_are_said() {
        assert!(matches!(
            parse(r#"{"t":"menu","back":true,"items":[]}"#),
            Some(BridgeIn::Menu(BridgeMenu { back: true, .. }))
        ));
        assert!(matches!(
            parse(r#"{"t":"menu","items":[]}"#),
            Some(BridgeIn::Menu(BridgeMenu { back: false, .. }))
        ));
        let left: Value = serde_json::from_str(&do_item("x", "nudge", -1).unwrap()).unwrap();
        assert_eq!(left["dir"], -1);
        let right: Value = serde_json::from_str(&do_item("x", "nudge", 1).unwrap()).unwrap();
        assert!(right.get("dir").is_none()); // nothing new for a nudge to the right
        let enter: Value = serde_json::from_str(&do_item("x", "enter", -1).unwrap()).unwrap();
        assert!(enter.get("dir").is_none()); // a direction means nothing to Enter
        assert_eq!(back(), r#"{"t":"back"}"#);
    }
}
