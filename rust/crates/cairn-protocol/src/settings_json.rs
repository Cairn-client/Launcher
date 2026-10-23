// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Changes single keys of a JSON settings file without disturbing the rest of it: the player's comments, key order, indentation,
//! unknown keys and the byte-order mark stay as they were. Only the bytes of a changed value are replaced, and a new key is appended
//! before the closing brace in the style of the last key. A file that is not one JSON object is refused and never rewritten.

use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::Path;

/// What [`apply`] says when there was nothing to do.
pub const NOTHING_TO_CHANGE: &str = "nothing to change";
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub key: String,
    pub value: Value,
    /// Only add the key when the file does not have it: the player's own value is never replaced.
    pub only_if_missing: bool,
}

impl Edit {
    pub fn set(key: &str, value: Value) -> Self {
        Self {
            key: key.into(),
            value,
            only_if_missing: false,
        }
    }
    pub fn fill(key: &str, value: Value) -> Self {
        Self {
            key: key.into(),
            value,
            only_if_missing: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Unchanged,
    Written,
    Created,
    Refused,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsResult {
    pub outcome: Outcome,
    pub why: String,
    pub changed: Vec<String>,
}

impl SettingsResult {
    pub fn ok(&self) -> bool {
        self.outcome != Outcome::Refused
    }
    fn refused(why: impl Into<String>) -> Self {
        Self {
            outcome: Outcome::Refused,
            why: why.into(),
            changed: Vec::new(),
        }
    }
}

struct Prop {
    name: String,
    start: usize,
    end: usize,
    name_start: usize,
}

struct Scan {
    props: Vec<Prop>,
    close_at: usize,
    empty: bool,
    root: Map<String, Value>,
}

/// Applies the edits to the bytes of a settings file. `Ok((bytes, changed keys))`, or `Err(why)`: the file is not one JSON
/// object (refused), or [`NOTHING_TO_CHANGE`].
pub fn apply(original: &[u8], edits: &[Edit]) -> Result<(Vec<u8>, Vec<String>), String> {
    let bom = if original.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let body = &original[bom..];
    let scan = scan(body)?;

    struct Repl {
        start: usize,
        end: usize,
        text: Vec<u8>,
        key: String,
    }
    let mut repl: Vec<Repl> = Vec::new();
    let mut appends: Vec<(String, Value)> = Vec::new();
    for e in edits {
        if e.key.is_empty() {
            continue;
        }
        let Some(hit) = scan.props.iter().rev().find(|p| p.name == e.key) else {
            if !appends.iter().any(|(k, _)| *k == e.key) {
                appends.push((e.key.clone(), e.value.clone()));
            }
            continue;
        };
        if e.only_if_missing {
            continue;
        }
        let now = &body[hit.start..hit.end];
        if same_value(now, &e.value) {
            repl.retain(|r| r.key != e.key); // a later edit of the same key may undo an earlier one
            continue;
        }
        repl.retain(|r| r.key != e.key);
        repl.push(Repl {
            start: hit.start,
            end: hit.end,
            text: value_text(&e.value).into_bytes(),
            key: e.key.clone(),
        });
    }
    if repl.is_empty() && appends.is_empty() {
        return Err(NOTHING_TO_CHANGE.into());
    }

    let mut changed = Vec::new();
    let mut out: Vec<u8> = Vec::with_capacity(body.len() + 64 * appends.len());
    let mut at = 0;
    repl.sort_by_key(|r| r.start);
    for r in &repl {
        out.extend_from_slice(&body[at..r.start]);
        out.extend_from_slice(&r.text);
        at = r.end;
        changed.push(r.key.clone());
    }
    // up to the root's closing brace, the appended keys, then the rest
    out.extend_from_slice(&body[at..scan.close_at]);
    if !appends.is_empty() {
        let (sep, tail) = separator(body, &scan.props);
        let mut first = scan.empty;
        // the whitespace before the closing brace stays before it
        let trim = out.iter().rev().take_while(|b| matches!(**b, b' ' | b'\t' | b'\r' | b'\n')).count();
        let keep: Vec<u8> = out[out.len() - trim..].to_vec();
        out.truncate(out.len() - trim);
        if out.last() == Some(&b',') {
            first = true; // a trailing comma is already there
        }
        let mut text = String::new();
        for (key, value) in &appends {
            if !first {
                text.push(',');
            }
            text.push_str(&sep);
            text.push_str(&serde_json::to_string(key).unwrap_or_default());
            text.push_str(&tail);
            text.push_str(&value_text(value));
            first = false;
            changed.push(key.clone());
        }
        out.extend_from_slice(text.as_bytes());
        if !keep.is_empty() {
            out.extend_from_slice(&keep);
        } else if sep.contains('\n') {
            out.push(b'\n');
        }
    }
    out.extend_from_slice(&body[scan.close_at..]);

    let mut result = Vec::with_capacity(bom + out.len());
    result.extend_from_slice(&original[..bom]);
    result.extend_from_slice(&out);
    Ok((result, changed))
}

/// A new settings file holding the edits (the first of a repeated key).
pub fn create(edits: &[Edit]) -> Vec<u8> {
    let mut o = Map::new();
    for e in edits {
        if !e.key.is_empty() && !o.contains_key(&e.key) {
            o.insert(e.key.clone(), e.value.clone());
        }
    }
    serde_json::to_vec(&Value::Object(o)).unwrap_or_default()
}

/// Applies the edits to the file at `path`: written through a temporary file moved over it, created when missing and `create_if_missing`.
pub fn update(path: &Path, edits: &[Edit], create_if_missing: bool) -> SettingsResult {
    if !path.exists() {
        if !create_if_missing {
            return SettingsResult::refused("no settings file");
        }
        if let Err(e) = write_atomic(path, &create(edits)) {
            return SettingsResult::refused(format!("{:?}: {e}", e.kind()));
        }
        let mut keys: Vec<String> = Vec::new();
        for e in edits {
            if !keys.contains(&e.key) {
                keys.push(e.key.clone());
            }
        }
        return SettingsResult {
            outcome: Outcome::Created,
            why: "created".into(),
            changed: keys,
        };
    }
    let cur = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return SettingsResult::refused(format!("{:?}: {e}", e.kind())),
    };
    match apply(&cur, edits) {
        Err(why) if why == NOTHING_TO_CHANGE => SettingsResult {
            outcome: Outcome::Unchanged,
            why,
            changed: Vec::new(),
        },
        Err(why) => SettingsResult::refused(why),
        Ok((next, changed)) => match write_atomic(path, &next) {
            Ok(()) => SettingsResult {
                outcome: Outcome::Written,
                why: "ok".into(),
                changed,
            },
            Err(e) => SettingsResult::refused(format!("{:?}: {e}", e.kind())),
        },
    }
}

/// The file's root object as key to value (a repeated key: the last one); `None` when missing or not one JSON object.
pub fn read(path: &Path) -> Option<Map<String, Value>> {
    let b = fs::read(path).ok()?;
    let body = b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&b);
    scan(body).ok().map(|s| s.root)
}

/// Writes through a temporary file in the same folder moved over the real one, so a crash never leaves half a file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let dir = full.parent().unwrap_or_else(|| Path::new("."));
    let name = full.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!("{name}.tmp-{}", std::process::id()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &full).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

// ---------------------------------------------------------------- the scan

fn scan(body: &[u8]) -> Result<Scan, String> {
    let mut p = Parser { b: body, i: 0 };
    p.skip_ws().map_err(|e| format!("unreadable JSON: {e}"))?;
    if p.peek() != Some(b'{') {
        return Err("the file is not one JSON object".into());
    }
    p.i += 1;
    let mut props = Vec::new();
    let mut root = Map::new();
    let mut empty = true;
    let close_at;
    loop {
        p.skip_ws().map_err(|e| format!("unreadable JSON: {e}"))?;
        match p.peek() {
            None => return Err("the root object never closes".into()),
            Some(b'}') => {
                close_at = p.i;
                p.i += 1;
                break;
            }
            Some(b'"') => {}
            Some(_) => return Err("unexpected token at the root".into()),
        }
        let name_start = p.i;
        let name = p.string().map_err(|e| format!("unreadable JSON: {e}"))?;
        p.skip_ws().map_err(|e| format!("unreadable JSON: {e}"))?;
        if p.peek() != Some(b':') {
            return Err("unreadable JSON: expected ':'".into());
        }
        p.i += 1;
        p.skip_ws().map_err(|e| format!("unreadable JSON: {e}"))?;
        let start = p.i;
        let value = p.value(1).map_err(|e| format!("unreadable JSON: {e}"))?;
        let end = p.i;
        root.insert(name.clone(), value);
        props.push(Prop {
            name,
            start,
            end,
            name_start,
        });
        empty = false;
        p.skip_ws().map_err(|e| format!("unreadable JSON: {e}"))?;
        match p.peek() {
            Some(b',') => p.i += 1,
            Some(b'}') => {}
            None => return Err("the root object never closes".into()),
            Some(_) => return Err("unreadable JSON: expected ',' or '}'".into()),
        }
    }
    // nothing but whitespace may follow the root object
    if body[p.i..].iter().any(|c| !matches!(c, b' ' | b'\t' | b'\r' | b'\n')) {
        return Err("text after the root object".into());
    }
    Ok(Scan {
        props,
        close_at,
        empty,
        root,
    })
}

/// A JSON reader that, like the one the settings were always read with, skips comments and allows trailing commas.
struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn skip_ws(&mut self) -> Result<(), String> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.i += 1,
                Some(b'/') => match self.b.get(self.i + 1) {
                    Some(b'/') => {
                        while !matches!(self.peek(), None | Some(b'\n')) {
                            self.i += 1;
                        }
                    }
                    Some(b'*') => {
                        self.i += 2;
                        loop {
                            match self.peek() {
                                None => return Err("a comment never ends".into()),
                                Some(b'*') if self.b.get(self.i + 1) == Some(&b'/') => {
                                    self.i += 2;
                                    break;
                                }
                                _ => self.i += 1,
                            }
                        }
                    }
                    _ => return Err("a stray '/'".into()),
                },
                _ => return Ok(()),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.i;
        self.i += 1;
        loop {
            match self.peek() {
                None => return Err("a string never ends".into()),
                Some(b'\\') => self.i += 2,
                Some(b'"') => {
                    self.i += 1;
                    break;
                }
                Some(c) if c < 0x20 => return Err("a control character in a string".into()),
                _ => self.i += 1,
            }
        }
        serde_json::from_slice::<String>(&self.b[start..self.i.min(self.b.len())]).map_err(|e| e.to_string())
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("nested too deeply".into());
        }
        match self.peek() {
            Some(b'{') => {
                self.i += 1;
                let mut m = Map::new();
                loop {
                    self.skip_ws()?;
                    match self.peek() {
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Value::Object(m));
                        }
                        Some(b'"') => {}
                        _ => return Err("expected a key".into()),
                    }
                    let k = self.string()?;
                    self.skip_ws()?;
                    if self.peek() != Some(b':') {
                        return Err("expected ':'".into());
                    }
                    self.i += 1;
                    self.skip_ws()?;
                    let v = self.value(depth + 1)?;
                    m.insert(k, v);
                    self.skip_ws()?;
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {}
                        _ => return Err("expected ',' or '}'".into()),
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    self.skip_ws()?;
                    if self.peek() == Some(b']') {
                        self.i += 1;
                        return Ok(Value::Array(a));
                    }
                    a.push(self.value(depth + 1)?);
                    self.skip_ws()?;
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b']') => {}
                        _ => return Err("expected ',' or ']'".into()),
                    }
                }
            }
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.i;
                while matches!(self.peek(), Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.i += 1;
                }
                serde_json::from_slice::<Value>(&self.b[start..self.i]).map_err(|e| e.to_string())
            }
            _ => Err("expected a value".into()),
        }
    }

    fn literal(&mut self, word: &str, v: Value) -> Result<Value, String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err("expected a value".into())
        }
    }
}

/// The whitespace before the last key's name and the text between that name and its value (": " or ":"), to write new keys the same way.
fn separator(body: &[u8], props: &[Prop]) -> (String, String) {
    let Some(p) = props.last() else {
        return (String::new(), ":".into());
    };
    // the whitespace before the last key's name
    let mut i = p.name_start;
    while i > 0 && matches!(body[i - 1], b' ' | b'\t' | b'\r' | b'\n') {
        i -= 1;
    }
    let lead = String::from_utf8_lossy(&body[i..p.name_start]).into_owned();
    // the text between the name and its value
    let mut name_end = p.name_start + 1;
    let mut esc = false;
    while name_end < body.len() {
        let c = body[name_end];
        if esc {
            esc = false;
        } else if c == b'\\' {
            esc = true;
        } else if c == b'"' {
            break;
        }
        name_end += 1;
    }
    let mut after = String::from_utf8_lossy(&body[(name_end + 1).min(p.start)..p.start]).into_owned();
    if !after.contains(':') {
        after = ":".into();
    }
    (lead, after)
}

fn value_text(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

fn same_value(current_text: &[u8], want: &Value) -> bool {
    let Ok(cur) = serde_json::from_slice::<Value>(current_text) else {
        return false;
    };
    values_equal(&cur, want)
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y || x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| values_equal(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| values_equal(v, w))),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(src: &str, edits: &[Edit]) -> String {
        let (out, _) = apply(src.as_bytes(), edits).expect("applies");
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_changed_value_is_replaced_in_place_and_the_rest_is_untouched() {
        let src = "{\n  // my note\n  \"A\": 1,\n  \"B\": \"x\", /* keep */\n  \"C\": [1, 2]\n}\n";
        assert_eq!(
            run(src, &[Edit::set("B", json!("y"))]),
            "{\n  // my note\n  \"A\": 1,\n  \"B\": \"y\", /* keep */\n  \"C\": [1, 2]\n}\n"
        );
    }

    #[test]
    fn a_new_key_follows_the_style_of_the_last_one() {
        assert_eq!(
            run("{\n  \"A\": 1\n}\n", &[Edit::set("B", json!(true))]),
            "{\n  \"A\": 1,\n  \"B\": true\n}\n"
        );
        assert_eq!(run("{\"A\":1}", &[Edit::set("B", json!(2))]), "{\"A\":1,\"B\":2}");
        assert_eq!(run("{\"A\": 1}", &[Edit::set("B", json!(2))]), "{\"A\": 1,\"B\": 2}");
        assert_eq!(run("{}", &[Edit::set("B", json!(2))]), "{\"B\":2}");
        assert_eq!(run("{\n}\n", &[Edit::set("B", json!(2))]), "{\"B\":2\n}\n");
    }

    #[test]
    fn a_trailing_comma_is_not_doubled() {
        assert_eq!(run("{\"A\":1,}", &[Edit::set("B", json!(2))]), "{\"A\":1,\"B\":2}");
    }

    #[test]
    fn fill_never_replaces_the_players_value() {
        assert!(apply(b"{\"A\":1}", &[Edit::fill("A", json!(9))]).is_err());
        assert_eq!(run("{\"A\":1}", &[Edit::fill("B", json!(9))]), "{\"A\":1,\"B\":9}");
    }

    #[test]
    fn the_same_value_is_nothing_to_change() {
        assert_eq!(apply(b"{\"A\":1}", &[Edit::set("A", json!(1))]).unwrap_err(), NOTHING_TO_CHANGE);
        assert_eq!(apply(b"{\"A\":1.0}", &[Edit::set("A", json!(1))]).unwrap_err(), NOTHING_TO_CHANGE);
        assert_eq!(
            apply(b"{\"A\":{\"x\":[1,2]}}", &[Edit::set("A", json!({"x":[1,2]}))]).unwrap_err(),
            NOTHING_TO_CHANGE
        );
    }

    #[test]
    fn the_last_of_a_repeated_key_is_the_one_changed() {
        assert_eq!(run("{\"A\":1,\"A\":2}", &[Edit::set("A", json!(3))]), "{\"A\":1,\"A\":3}");
    }

    #[test]
    fn a_byte_order_mark_stays() {
        let mut src = vec![0xEF, 0xBB, 0xBF];
        src.extend_from_slice(b"{\"A\":1}");
        let (out, _) = apply(&src, &[Edit::set("A", json!(2))]).unwrap();
        assert_eq!(&out[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(&out[3..], b"{\"A\":2}");
    }

    #[test]
    fn a_file_that_is_not_one_object_is_refused() {
        for bad in [
            "[1]",
            "",
            "{\"A\":1",
            "{\"A\":1} x",
            "{\"A\":}",
            "{\"A\" 1}",
            "{,}",
            "nope",
            "{\"A\":1} // c",
        ] {
            let e = apply(bad.as_bytes(), &[Edit::set("B", json!(1))]).unwrap_err();
            assert_ne!(e, NOTHING_TO_CHANGE, "{bad}");
        }
    }

    #[test]
    fn nested_values_and_strings_with_braces_are_skipped_whole() {
        let src = "{\"A\":{\"x\":\"}\",\"y\":[{\"z\":\"\\\"\"}]},\"B\":1}";
        assert_eq!(
            run(src, &[Edit::set("B", json!(2))]),
            "{\"A\":{\"x\":\"}\",\"y\":[{\"z\":\"\\\"\"}]},\"B\":2}"
        );
    }

    #[test]
    fn reading_gives_the_root_object() {
        let dir = std::env::temp_dir().join(format!("cairn-settings-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.json");
        fs::write(&f, "\u{feff}{ // c\n \"A\": 1, \"A\": 2, \"B\": [true,],}").unwrap();
        let m = read(&f).unwrap();
        assert_eq!(m["A"], json!(2));
        assert_eq!(m["B"], json!([true]));
        fs::write(&f, "[1]").unwrap();
        assert!(read(&f).is_none());
        assert!(read(&dir.join("missing.json")).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_creates_writes_and_leaves_a_refused_file_alone() {
        let dir = std::env::temp_dir().join(format!("cairn-settings-up-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.json");
        let r = update(&f, &[Edit::set("A", json!(1))], true);
        assert_eq!(r.outcome, Outcome::Created);
        assert_eq!(fs::read_to_string(&f).unwrap(), "{\"A\":1}");
        let r = update(&f, &[Edit::set("A", json!(2)), Edit::set("B", json!("x"))], true);
        assert_eq!(
            (r.outcome, r.changed.clone()),
            (Outcome::Written, vec!["A".to_string(), "B".to_string()])
        );
        assert_eq!(fs::read_to_string(&f).unwrap(), "{\"A\":2,\"B\":\"x\"}");
        assert_eq!(update(&f, &[Edit::set("A", json!(2))], true).outcome, Outcome::Unchanged);
        fs::write(&f, "garbage").unwrap();
        let r = update(&f, &[Edit::set("A", json!(1))], true);
        assert_eq!(r.outcome, Outcome::Refused);
        assert_eq!(fs::read_to_string(&f).unwrap(), "garbage");
        assert_eq!(
            update(&dir.join("none.json"), &[Edit::set("A", json!(1))], false).outcome,
            Outcome::Refused
        );
        assert!(fs::read_dir(&dir)
            .unwrap()
            .all(|e| !e.unwrap().file_name().to_string_lossy().contains(".tmp-")));
        let _ = fs::remove_dir_all(&dir);
    }
}
