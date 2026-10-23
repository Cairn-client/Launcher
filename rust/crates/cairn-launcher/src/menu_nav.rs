// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The in-game menu as the mod describes it, and what the player has done with it: the selection, and what each key asks of the mod.
//! Nothing here is about one game: the mod chooses the words and the items.

use cairn_setup::bridge_protocol::BridgeMenu;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    OnOff,
    Choice,
    Key,
    Action,
    Info,
}

/// One row of the menu. `on` is set for a switch. A locked item belongs to the host. A switch with a `gate` (a key name such as "f3") decides
/// whether the launcher lets that key reach the game: only while the switch is on.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub id: String,
    pub group: String,
    pub kind: MenuKind,
    pub on: Option<bool>,
    pub locked: bool,
    pub value: String,
    pub label: String,
    pub help: String,
    pub gate: String,
    pub disabled: bool,
}

impl MenuItem {
    pub fn selectable(&self) -> bool {
        self.kind != MenuKind::Info
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuState {
    pub game: String,
    pub title: String,
    pub blocked: String,
    pub note: String,
    pub items: Vec<MenuItem>,
    pub submenu: bool,
}

impl MenuState {
    pub fn empty() -> Self {
        Self {
            game: String::new(),
            title: "Mod menu".into(),
            blocked: String::new(),
            note: String::new(),
            items: vec![],
            submenu: false,
        }
    }

    pub fn from_bridge(game: &str, m: &BridgeMenu) -> Self {
        Self {
            game: game.to_string(),
            title: if m.title.is_empty() { "Mod menu".into() } else { m.title.clone() },
            blocked: m.blocked.clone(),
            note: m.note.clone(),
            items: m
                .items
                .iter()
                .map(|i| MenuItem {
                    id: i.id.clone(),
                    group: i.group.clone(),
                    kind: match i.kind.as_str() {
                        "onoff" => MenuKind::OnOff,
                        "choice" => MenuKind::Choice,
                        "key" => MenuKind::Key,
                        "action" => MenuKind::Action,
                        _ => MenuKind::Info,
                    },
                    on: i.on,
                    locked: i.locked,
                    value: i.value.clone(),
                    label: i.label.clone(),
                    help: i.help.clone(),
                    gate: i.gate.clone(),
                    disabled: i.disabled,
                })
                .collect(),
            submenu: m.back,
        }
    }
}

/// The virtual-key code of a gate key name ("f1" to "f12"), or `None` for a name this does not know.
pub fn vk_of_gate(name: &str) -> Option<u32> {
    let n = name.trim().to_ascii_lowercase();
    let rest = n.strip_prefix('f')?;
    if !(1..=2).contains(&rest.len()) || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let k: u32 = rest.parse().ok()?;
    (1..=12).contains(&k).then(|| 0x70 + k - 1)
}

/// The keys the menu takes while it is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    Toggle,
    Up,
    Down,
    Left,
    Right,
    Enter,
    Back,
}

#[derive(Debug, Clone)]
pub struct MenuNav {
    pub state: MenuState,
    pub open: bool,
    selected_id: String,
}

impl Default for MenuNav {
    fn default() -> Self {
        Self {
            state: MenuState::empty(),
            open: false,
            selected_id: String::new(),
        }
    }
}

impl MenuNav {
    /// The selected row's index (the first one that can be chosen when nothing is).
    pub fn selected_index(&self) -> Option<usize> {
        let items = &self.state.items;
        items
            .iter()
            .position(|i| i.id == self.selected_id && i.selectable())
            .or_else(|| items.iter().position(MenuItem::selectable))
    }

    pub fn selected(&self) -> Option<&MenuItem> {
        self.selected_index().map(|i| &self.state.items[i])
    }

    /// A new state from the mod: the selection stays on the same item when it is still there.
    pub fn update(&mut self, state: MenuState) {
        let keep = self.selected().map(|s| s.id.clone()).unwrap_or_else(|| self.selected_id.clone());
        self.state = state;
        self.selected_id = keep;
        self.selected_id = self.selected().map(|s| s.id.clone()).unwrap_or_default();
    }

    /// Up or Down: the next item that can be chosen, wrapping round at either end.
    pub fn step(&mut self, dir: i32) {
        let n = self.state.items.len() as i32;
        let Some(mut i) = self.selected_index().map(|i| i as i32) else {
            return;
        };
        for _ in 0..n {
            i += dir;
            if i < 0 {
                i = n - 1;
            } else if i >= n {
                i = 0;
            }
            if self.state.items[i as usize].selectable() {
                self.selected_id = self.state.items[i as usize].id.clone();
                return;
            }
        }
    }

    /// The request a key makes of the mod: Left and Right change a value (never run an action: a stray arrow must not do "I'm stuck"); Enter
    /// does the item, whatever it is. A locked item (the host's) is left to the mod, which says why it refuses.
    pub fn request(&self, key: MenuKey) -> Option<(String, &'static str, i32)> {
        let sel = self.selected().filter(|s| !s.disabled)?;
        match key {
            MenuKey::Enter => Some((sel.id.clone(), "enter", 1)),
            MenuKey::Left | MenuKey::Right if matches!(sel.kind, MenuKind::OnOff | MenuKind::Choice | MenuKind::Key) => {
                Some((sel.id.clone(), "nudge", if key == MenuKey::Left { -1 } else { 1 }))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, kind: MenuKind) -> MenuItem {
        MenuItem {
            id: id.into(),
            group: String::new(),
            kind,
            on: None,
            locked: false,
            value: String::new(),
            label: id.into(),
            help: String::new(),
            gate: String::new(),
            disabled: false,
        }
    }

    fn state(items: Vec<MenuItem>) -> MenuState {
        MenuState {
            items,
            ..MenuState::empty()
        }
    }

    #[test]
    fn gate_names_become_function_keys() {
        assert_eq!(vk_of_gate("f1"), Some(0x70));
        assert_eq!(vk_of_gate(" F12 "), Some(0x7B));
        assert_eq!(vk_of_gate("f13"), None);
        assert_eq!(vk_of_gate("g1"), None);
        assert_eq!(vk_of_gate("f"), None);
    }

    #[test]
    fn the_selection_skips_information_and_wraps() {
        let mut nav = MenuNav::default();
        nav.update(state(vec![
            item("title", MenuKind::Info),
            item("a", MenuKind::OnOff),
            item("note", MenuKind::Info),
            item("b", MenuKind::Action),
        ]));
        assert_eq!(nav.selected().unwrap().id, "a"); // the first row that can be chosen
        nav.step(1);
        assert_eq!(nav.selected().unwrap().id, "b");
        nav.step(1);
        assert_eq!(nav.selected().unwrap().id, "a");
        nav.step(-1);
        assert_eq!(nav.selected().unwrap().id, "b");
    }

    #[test]
    fn a_new_state_keeps_the_selection_when_the_item_is_still_there() {
        let mut nav = MenuNav::default();
        nav.update(state(vec![item("a", MenuKind::OnOff), item("b", MenuKind::OnOff)]));
        nav.step(1);
        nav.update(state(vec![
            item("x", MenuKind::Info),
            item("a", MenuKind::OnOff),
            item("b", MenuKind::OnOff),
        ]));
        assert_eq!(nav.selected().unwrap().id, "b");
        nav.update(state(vec![item("a", MenuKind::OnOff)]));
        assert_eq!(nav.selected().unwrap().id, "a");
    }

    #[test]
    fn arrows_change_values_but_never_run_an_action() {
        let mut nav = MenuNav::default();
        nav.update(state(vec![item("sw", MenuKind::OnOff), item("go", MenuKind::Action)]));
        assert_eq!(nav.request(MenuKey::Right), Some(("sw".into(), "nudge", 1)));
        assert_eq!(nav.request(MenuKey::Left), Some(("sw".into(), "nudge", -1)));
        assert_eq!(nav.request(MenuKey::Enter), Some(("sw".into(), "enter", 1)));
        nav.step(1);
        assert_eq!(nav.request(MenuKey::Right), None); // a stray arrow does not do the action
        assert_eq!(nav.request(MenuKey::Enter), Some(("go".into(), "enter", 1)));
        let mut off = item("d", MenuKind::OnOff);
        off.disabled = true;
        nav.update(state(vec![off]));
        assert_eq!(nav.request(MenuKey::Enter), None);
    }
}
