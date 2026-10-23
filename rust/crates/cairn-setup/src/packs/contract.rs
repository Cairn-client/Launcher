// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! What the launcher needs to know to drive a mod's programs (the pack's `pack.json`). The mod repository decides these names; the launcher
//! never hard-codes a game's agent, relay or plugin. Everything here comes from the network: only plain names and paths that stay inside
//! the pack are accepted.

use crate::catalog_limits as lim;
use serde_json::Value;
use std::collections::BTreeMap;

pub const FILE_NAME: &str = "pack.json";
/// The version of the mod-menu messages this launcher speaks; a pack that needs a newer one is refused.
pub const SUPPORTED_MENU_PROTOCOL: i64 = 1;
/// Where a content pack's files go: the only place the launcher knows to put them, never a path the pack names.
pub const START_SAVE_TARGET: &str = "start-save";
pub const CURRENT_SCHEMA: i64 = 2;
/// `installDir` value for the game's own folder.
pub const GAME_ROOT: &str = ".";
pub const INSTALL_ROOT_LOCAL_APP_DATA: &str = "local-appdata";

pub struct PackKinds;

impl PackKinds {
    pub const MOD: &'static str = "mod";
    pub const CONTENT: &'static str = "content";
    pub const EXTERNAL: &'static str = "external";

    pub fn is_known(kind: &str) -> bool {
        matches!(kind, Self::MOD | Self::CONTENT | Self::EXTERNAL)
    }

    /// A game's mod (its own session programs, or an external mod), as opposed to content.
    pub fn is_game_mod(kind: &str) -> bool {
        matches!(kind, Self::MOD | Self::EXTERNAL)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackDependency {
    pub id: String,
    pub min: String,
}

const LUA_KEYS: [&str; 11] = [
    "eventPrefix",
    "startGameEvent",
    "startGameModel",
    "startGameTick",
    "startGameRecapStart",
    "startGameDisarm",
    "joinFlowLoad",
    "menuRequest",
    "helperFlag",
    "helperTag",
    "nativeLog",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackContract {
    pub id: String,
    pub version: String,
    /// The version of the project the mod is based on.
    pub upstream: String,
    pub mod_folder: String,
    pub lua_prefix: String,
    pub agent: String,
    pub relay: String,
    pub native: String,
    pub brand_pak: String,
    pub agent_config: String,
    pub launcher_min: String,
    pub replaces_folders: Vec<String>,
    pub lua: BTreeMap<String, String>,
    pub kind: String,
    pub depends_on: Vec<PackDependency>,
    pub menu_protocol: i64,
    pub install_to: String,
    pub install_dir: String,
    pub companion: String,
    pub companion_starts_game: bool,
    pub options_program: String,
    pub setup: bool,
    pub session: bool,
    pub install_root: String,
    pub launch: String,
    pub launch_args: Vec<String>,
}

impl PackContract {
    /// `None` for anything that is not a contract this launcher understands.
    pub fn parse(json: &str) -> Option<Self> {
        let r: Value = serde_json::from_str(json).ok()?;
        if r.get("schema")?.as_i64()? != CURRENT_SCHEMA {
            return None;
        }
        let s = |name: &str| lim::str_of(&r, name);
        let mut replaces = Vec::new();
        if let Some(rf) = r.get("replacesFolders").and_then(Value::as_array) {
            for e in rf {
                replaces.push(e.as_str()?.to_string());
            }
        }
        let mut lua = BTreeMap::new();
        if let Some(lj) = r.get("lua").and_then(Value::as_object) {
            for (name, value) in lj {
                let v = value.as_str().unwrap_or("");
                let valid = LUA_KEYS.contains(&name.as_str())
                    && match name.as_str() {
                        "eventPrefix" => is_event_prefix(v),
                        "startGameEvent" | "helperTag" => is_word(v),
                        "helperFlag" => is_flag(v),
                        "nativeLog" => is_file_name(v),
                        _ => is_lua_name(v),
                    };
                if !valid {
                    return None;
                }
                lua.insert(name.clone(), v.to_string());
            }
        }
        let kind = if r.get("kind").and_then(Value::as_str).is_some() {
            s("kind")
        } else {
            PackKinds::MOD.to_string()
        };
        let depends_on = parse_dependencies(&r)?;
        let menu_protocol = r
            .get("menuProtocol")
            .and_then(Value::as_i64)
            .filter(|n| i32::try_from(*n).is_ok())
            .unwrap_or(1);
        let launch_args = parse_launch_args(&r)?;
        let c = Self {
            id: s("id"),
            version: s("version"),
            upstream: s("upstream"),
            mod_folder: s("modFolder"),
            lua_prefix: s("luaPrefix"),
            agent: s("agent"),
            relay: s("relay"),
            native: s("native"),
            brand_pak: s("brandPak"),
            agent_config: s("agentConfig"),
            launcher_min: s("launcherMin"),
            replaces_folders: replaces,
            lua,
            kind,
            depends_on,
            menu_protocol,
            install_to: s("installTo"),
            install_dir: s("installDir"),
            companion: s("companion"),
            companion_starts_game: lim::flag(&r, "companionStartsGame"),
            options_program: s("optionsProgram"),
            setup: lim::flag(&r, "setup"),
            session: lim::flag(&r, "session"),
            install_root: s("installRoot"),
            launch: s("launch"),
            launch_args,
        };
        c.valid().then_some(c)
    }

    fn valid(&self) -> bool {
        let c = self;
        let common = is_plain_name(&c.id)
            && !c.version.is_empty()
            && PackKinds::is_known(&c.kind)
            && c.menu_protocol >= 1
            && (c.install_to.is_empty() || c.install_to == START_SAVE_TARGET)
            && c.replaces_folders.iter().all(|f| is_plain_name(f));
        let no_programs = c.agent.is_empty() && c.relay.is_empty() && c.native.is_empty();
        let external = c.kind == PackKinds::EXTERNAL;
        // a game's mod names its programs and its mod folder (and may carry a companion: its bridge to the launcher); content is only files; an
        // external mod names a folder inside the game's own folder (and may carry a companion to start with the game); only an external mod has
        // an install folder, and content has no companion
        let general = common
            && (c.install_dir.is_empty() || external)
            && (c.launch.is_empty() || (external && is_game_relative_program(&c.launch)))
            && (c.launch_args.is_empty() || !c.launch.is_empty())
            && (c.companion.is_empty() || c.kind != PackKinds::CONTENT)
            && (!c.companion_starts_game || external)
            && (c.options_program.is_empty() || (c.kind != PackKinds::CONTENT && is_pack_path(&c.options_program)))
            && (!c.setup || !c.options_program.is_empty())
            && (!c.session || (!c.options_program.is_empty() && c.kind == PackKinds::MOD));
        general
            && match c.kind.as_str() {
                PackKinds::CONTENT => no_programs,
                PackKinds::EXTERNAL => {
                    no_programs
                        && c.install_to.is_empty()
                        && install_dir_ok(c)
                        && (c.companion.is_empty() || is_pack_path(&c.companion))
                        && (!c.companion_starts_game || !c.companion.is_empty())
                }
                _ => {
                    is_plain_name(&c.mod_folder)
                        && (1..=40).contains(&c.lua_prefix.chars().count())
                        && is_pack_path(&c.agent)
                        && is_pack_path(&c.relay)
                        && is_pack_path(&c.native)
                        && (c.brand_pak.is_empty() || is_pack_path(&c.brand_pak))
                        && (c.agent_config.is_empty() || is_file_name(&c.agent_config))
                        && (c.companion.is_empty() || is_pack_path(&c.companion))
                }
            }
    }
}

/// `dependsOn`: at most 8 packs, each with a plain id and a strict `min` version; `None` when malformed.
pub fn parse_dependencies(parent: &Value) -> Option<Vec<PackDependency>> {
    let Some(d) = parent.get("dependsOn") else {
        return Some(Vec::new());
    };
    let arr = d.as_array().filter(|a| a.len() <= 8)?;
    let mut deps = Vec::new();
    for e in arr {
        let (id, min) = (lim::str_of(e, "id"), lim::str_of(e, "min"));
        if !is_plain_name(&id) || !lim::is_version(&min) {
            return None;
        }
        deps.push(PackDependency { id, min });
    }
    Some(deps)
}

/// `launchArgs`: at most 8 plain words, passed as a list and never joined into a command line; `None` when malformed.
pub fn parse_launch_args(parent: &Value) -> Option<Vec<String>> {
    let Some(a) = parent.get("launchArgs") else {
        return Some(Vec::new());
    };
    let arr = a.as_array().filter(|a| a.len() <= 8)?;
    let mut list = Vec::new();
    for e in arr {
        let v = e.as_str().filter(|v| !v.is_empty() && v.chars().count() <= 80)?;
        if !v
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'=' | b':' | b'+' | b'.' | b',' | b'/'))
        {
            return None;
        }
        list.push(v.to_string());
    }
    Some(list)
}

fn is_lua_name(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=60).contains(&b.len()) && (b[0].is_ascii_alphabetic() || b[0] == b'_') && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

fn is_flag(s: &str) -> bool {
    s.len() > 2 && s.len() <= 24 && s.starts_with("--") && s[2..].bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn is_word(s: &str) -> bool {
    !s.is_empty() && s.len() <= 20 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn is_event_prefix(s: &str) -> bool {
    (3..=40).contains(&s.chars().count()) && s.chars().all(|c| (' '..='~').contains(&c) && c != '"' && c != '\\')
}

/// A plain id: letters, digits, `-` and `_`.
pub fn is_plain_name(s: &str) -> bool {
    lim::is_id(s)
}

fn is_file_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.')) && !s.contains("..")
}

/// A path inside the pack: plain segments, never absolute, never leaving it.
pub fn is_pack_path(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 120
        && !s.contains("..")
        && !s.starts_with('/')
        && !s.contains(':')
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'/'))
}

fn install_dir_ok(c: &PackContract) -> bool {
    match c.install_root.as_str() {
        "" => is_game_relative_dir(&c.install_dir),
        INSTALL_ROOT_LOCAL_APP_DATA => c.install_dir != GAME_ROOT && is_relative_dir(&c.install_dir, 6),
        _ => false,
    }
}

/// A program inside the game's folder an external mod starts the game with (`launch`): a relative path with plain segments, ending in `.exe`.
pub fn is_game_relative_program(s: &str) -> bool {
    s.to_ascii_lowercase().ends_with(".exe") && is_relative_dir(s, 8)
}

/// The game's own folder (a loader such as BepInEx puts files there), or a folder inside it.
pub fn is_game_relative_dir(s: &str) -> bool {
    s == GAME_ROOT || is_relative_dir(s, 4)
}

pub fn is_relative_dir(s: &str, max_parts: usize) -> bool {
    if s.is_empty() || s.len() > 120 || s.contains("..") || s.contains(':') || s.contains('\\') || s.starts_with('/') || s.ends_with('/') {
        return false;
    }
    let parts: Vec<&str> = s.split('/').collect();
    parts.len() <= max_parts
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b' '))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn external(extra: &str) -> String {
        format!(r#"{{ "schema": 2, "id": "noita", "version": "1.0.0", "kind": "external", "installDir": "mods/quant.ew"{extra} }}"#)
    }

    const MOD: &str = r#"{ "schema": 2, "id": "kcd2", "version": "0.2.0", "upstream": "1.0", "modFolder": "kdcmp", "luaPrefix": "kdcmp", "agent": "bin/agent.exe", "relay": "bin/relay.exe", "native": "bin/n.dll", "launcherMin": "0.3.0", "replacesFolders": ["old_mod"], "lua": { "eventPrefix": "kdcmp", "startGameEvent": "go" } }"#;

    #[test]
    fn a_session_mod_contract_is_read() {
        let c = PackContract::parse(MOD).unwrap();
        assert_eq!(
            (c.kind.as_str(), c.mod_folder.as_str(), c.agent.as_str()),
            ("mod", "kdcmp", "bin/agent.exe")
        );
        assert_eq!(c.replaces_folders, vec!["old_mod"]);
        assert_eq!(c.lua["startGameEvent"], "go");
        assert_eq!(c.menu_protocol, 1);
        for bad in [
            MOD.replace("bin/agent.exe", "../agent.exe"),
            MOD.replace("bin/relay.exe", "/relay.exe"),
            MOD.replace("bin/n.dll", "C:/n.dll"),
            MOD.replace("\"kdcmp\", \"luaPrefix\"", "\"a b\", \"luaPrefix\""),
            MOD.replace("\"schema\": 2", "\"schema\": 1"),
            MOD.replace("\"startGameEvent\": \"go\"", "\"startGameEvent\": \"g o\""),
            MOD.replace("\"startGameEvent\": \"go\"", "\"surprise\": \"go\""),
            MOD.replace("old_mod", "../old"),
        ] {
            assert!(PackContract::parse(&bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn an_external_contract_names_a_folder_inside_the_game() {
        let c = PackContract::parse(&external(r#", "companion": "bin/proxy.exe""#)).unwrap();
        assert_eq!((c.install_dir.as_str(), c.companion.as_str()), ("mods/quant.ew", "bin/proxy.exe"));
        assert!(PackContract::parse(&external("")).unwrap().companion.is_empty());
        for bad in [
            external("").replace("mods/quant.ew", "../evil"),
            external("").replace("mods/quant.ew", "C:/evil"),
            external("").replace("mods/quant.ew", "a/b/c/d/e"),
            external("").replace("mods/quant.ew", ""),
            external(r#", "agent": "bin/a.exe""#),
            external(r#", "companion": "../p.exe""#),
            external(r#", "companionStartsGame": true"#), // no companion to do it
            external(r#", "installRoot": "system32""#),
        ] {
            assert!(PackContract::parse(&bad).is_none(), "{bad}");
        }
        assert!(PackContract::parse(&external("").replace("mods/quant.ew", ".")).is_some());
        // the game's own folder
    }

    #[test]
    fn an_external_mod_can_use_the_local_application_data_folder() {
        let c = PackContract::parse(
            &external(r#", "installRoot": "local-appdata""#).replace("mods/quant.ew", "Colossal Order/Cities_Skylines/Addons/Mods/CSM"),
        )
        .unwrap();
        assert_eq!(c.install_root, "local-appdata");
        assert!(PackContract::parse(&external(r#", "installRoot": "local-appdata""#).replace("mods/quant.ew", ".")).is_none());
    }

    #[test]
    fn a_contract_can_name_the_program_of_the_games_folder_that_starts_the_game() {
        let json = |launch: &str, kind: &str| {
            format!(r#"{{ "schema": 2, "id": "dc", "version": "1.0.0", "kind": "{kind}", "installDir": ".", "launch": "{launch}" }}"#)
        };
        let exe = "coremod/core/host/startup/DeadCellsModding.exe";
        assert_eq!(PackContract::parse(&json(exe, "external")).unwrap().launch, exe);
        for bad in [
            "../evil.exe",
            "C:/evil.exe",
            "/evil.exe",
            "tools/run.bat",
            ".hidden/run.exe",
            "a/b/c/d/e/f/g/h/run.exe",
        ] {
            assert!(PackContract::parse(&json(bad, "external")).is_none(), "{bad}");
        }
        assert!(
            PackContract::parse(r#"{ "schema": 2, "id": "dc", "version": "1.0.0", "kind": "content", "launch": "run.exe" }"#).is_none()
        );
    }

    #[test]
    fn the_launch_program_can_take_a_few_plain_arguments() {
        let json = |args: &str| {
            format!(
                r#"{{ "schema": 2, "id": "fc", "version": "1.0.0", "kind": "external", "installDir": ".", "launch": "Bin32/FarCry.exe", "launchArgs": {args} }}"#
            )
        };
        assert_eq!(
            PackContract::parse(&json(r#"["-DEVMODE", "-MOD:FarCryCoop"]"#))
                .unwrap()
                .launch_args,
            vec!["-DEVMODE", "-MOD:FarCryCoop"]
        );
        assert!(PackContract::parse(&json("[]")).unwrap().launch_args.is_empty());
        for bad in [
            r#"["a b"]"#,
            r#"["\"x"]"#,
            r#"["&calc"]"#,
            r#"[""]"#,
            "[1]",
            r#""-x""#,
            r#"["1","2","3","4","5","6","7","8","9"]"#,
        ] {
            assert!(PackContract::parse(&json(bad)).is_none(), "{bad}");
        }
        // arguments of nothing
        assert!(PackContract::parse(
            r#"{ "schema": 2, "id": "fc", "version": "1.0.0", "kind": "external", "installDir": ".", "launchArgs": ["-x"] }"#
        )
        .is_none());
    }

    #[test]
    fn a_content_contract_has_no_programs_and_a_known_target() {
        let content = |extra: &str| {
            format!(r#"{{ "schema": 2, "id": "saves", "version": "1.0.0", "kind": "content", "installTo": "start-save"{extra} }}"#)
        };
        assert!(PackContract::parse(&content("")).is_some());
        for bad in [
            content(r#", "agent": "bin/a.exe""#),
            content(r#", "companion": "bin/c.exe""#),
            content("").replace("start-save", "system32"),
            r#"{ "schema": 2, "id": "s", "version": "1", "kind": "content", "installDir": "x" }"#.to_string(),
        ] {
            assert!(PackContract::parse(&bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn dependencies_and_the_menu_protocol_are_read() {
        let c = PackContract::parse(&external(
            r#", "dependsOn": [ { "id": "core", "min": "1.2.0" } ], "menuProtocol": 1"#,
        ))
        .unwrap();
        assert_eq!(
            c.depends_on,
            vec![PackDependency {
                id: "core".into(),
                min: "1.2.0".into()
            }]
        );
        for bad in [
            r#", "dependsOn": [ { "id": "core" } ]"#,
            r#", "dependsOn": [ { "id": "../x", "min": "1.0.0" } ]"#,
            r#", "dependsOn": "core""#,
            r#", "menuProtocol": 0"#,
        ] {
            assert!(PackContract::parse(&external(bad)).is_none(), "{bad}");
        }
        let nine: Vec<String> = (0..9).map(|i| format!(r#"{{ "id": "p{i}", "min": "1.0.0" }}"#)).collect();
        assert!(PackContract::parse(&external(&format!(r#", "dependsOn": [{}]"#, nine.join(",")))).is_none());
    }

    #[test]
    fn the_options_program_the_setup_and_the_session_depend_on_each_other() {
        assert!(PackContract::parse(&external(r#", "optionsProgram": "bin/tool.exe", "setup": true"#)).is_some());
        assert!(PackContract::parse(&external(r#", "setup": true"#)).is_none()); // a setup needs a program
        assert!(PackContract::parse(&external(r#", "optionsProgram": "../tool.exe""#)).is_none());
        assert!(PackContract::parse(&external(r#", "optionsProgram": "bin/tool.exe", "session": true"#)).is_none());
        // only a session mod runs one
    }
}
