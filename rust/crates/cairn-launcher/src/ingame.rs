// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The shared in-game menu of a running game. The game's bridge (docs/BRIDGE.md) says what the mod's menu holds, its notices and the lines its
//! characters speak; this draws them over the game, takes the keys while the menu is open (Insert opens and closes it, Up and Down choose, Left and
//! Right change, Enter selects, Esc goes back or closes) and sends the bridge what was chosen. Until the mod has answered once nothing is taken: a
//! game without the launcher's menu keeps its own keys.

use crate::app::CairnApp;
use crate::menu_nav::{vk_of_gate, MenuKey, MenuKind, MenuNav, MenuState};
use crate::winapi::{self, ClientRect, KeyboardHook};
use crate::{runtime, ui::theme};
use cairn_setup::bridge_protocol;
use eframe::egui::{self, Align2, Color32, FontId, RichText};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const VK_ESC: u32 = 0x1B;
const VK_RETURN: u32 = 0x0D;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_INSERT: u32 = 0x2D;
const VK_ADD: u32 = 0x6B;
const VK_SUBTRACT: u32 = 0x6D;

/// What the keyboard hook (another thread) reads and writes.
struct Shared {
    open: AtomicBool,
    attached: AtomicBool,
    pid: AtomicU32,
    toggle_vk: u32,
    blocked: Mutex<Vec<u32>>,
    down: Mutex<[bool; 256]>,
    keys: Mutex<Sender<MenuKey>>,
}

impl Shared {
    /// The hook's decision for one key: `true` keeps it from the game.
    fn on_key(&self, vk: u32, down: bool) -> bool {
        let pid = self.pid.load(Ordering::SeqCst);
        let attached = self.attached.load(Ordering::SeqCst);
        // a key whose switch in the menu is off never reaches the game (F3, the game's own fly mode, while "Free movement" is off)
        if attached && pid != 0 && self.blocked.lock().map(|b| b.contains(&vk)).unwrap_or(false) && winapi::is_foreground(pid) {
            return true;
        }
        let key = match vk {
            v if v == self.toggle_vk => MenuKey::Toggle,
            VK_UP => MenuKey::Up,
            VK_DOWN => MenuKey::Down,
            VK_LEFT => MenuKey::Left,
            VK_RIGHT => MenuKey::Right,
            VK_RETURN => MenuKey::Enter,
            VK_ESC => MenuKey::Back,
            _ => return false,
        };
        if vk > 255 {
            return false;
        }
        if pid == 0 || !winapi::is_foreground(pid) {
            if let Ok(mut d) = self.down.lock() {
                d[vk as usize] = false;
            }
            return false;
        }
        if key == MenuKey::Toggle {
            if !attached {
                return false; // the mod has not answered: its own key does its own thing
            }
        } else if !self.open.load(Ordering::SeqCst) {
            return false; // the arrows, Enter and Esc are the game's until the menu is open
        }
        let Ok(mut d) = self.down.lock() else { return false };
        if !down {
            d[vk as usize] = false;
            return true; // the key's release is kept from the game too
        }
        let repeat = d[vk as usize];
        d[vk as usize] = true;
        // holding Up, Down, Left or Right repeats; Enter, Esc and the menu key are one press each
        if repeat && matches!(key, MenuKey::Toggle | MenuKey::Enter | MenuKey::Back) {
            return true;
        }
        if let Ok(tx) = self.keys.lock() {
            let _ = tx.send(key);
        }
        true
    }
}

pub struct InGame {
    pub nav: MenuNav,
    shared: Arc<Shared>,
    keys: Receiver<MenuKey>,
    _hook: Option<KeyboardHook>,
    stem: String,
    pid: Option<u32>,
    pid_at: Instant,
    rect: Option<ClientRect>,
    front: bool,
    window_at: Instant,
    opened_at: Instant,
    next_attach: Instant,
    next_refresh: Instant,
    subtitle: Option<(String, Instant)>,
    notice: Option<(String, Instant)>,
}

fn toggle_vk(name: &str) -> u32 {
    match name.trim().to_ascii_lowercase().as_str() {
        "np_add" => VK_ADD,
        "np_subtract" => VK_SUBTRACT,
        _ => VK_INSERT,
    }
}

impl InGame {
    fn start(exe_stem: &str, menu_key: &str) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            open: AtomicBool::new(false),
            attached: AtomicBool::new(false),
            pid: AtomicU32::new(0),
            toggle_vk: toggle_vk(menu_key),
            blocked: Mutex::new(vec![]),
            down: Mutex::new([false; 256]),
            keys: Mutex::new(tx),
        });
        let s = shared.clone();
        let hook = KeyboardHook::install(Box::new(move |vk, down| s.on_key(vk, down)));
        let now = Instant::now();
        Self {
            nav: MenuNav::default(),
            shared,
            keys: rx,
            _hook: hook,
            stem: exe_stem.to_string(),
            pid: None,
            pid_at: now - Duration::from_secs(10),
            rect: None,
            front: false,
            window_at: now - Duration::from_secs(10),
            opened_at: now,
            next_attach: now,
            next_refresh: now,
            subtitle: None,
            notice: None,
        }
    }

    fn attached(&self) -> bool {
        self.shared.attached.load(Ordering::SeqCst)
    }

    fn set_open(&mut self, open: bool) {
        self.nav.open = open;
        self.shared.open.store(open, Ordering::SeqCst);
    }

    /// A new menu from the mod: the keys it gates are now kept from the game, and the launcher draws the menu from here on.
    pub fn on_menu(&mut self, state: MenuState) {
        let mut blocked: Vec<u32> = state
            .items
            .iter()
            .filter(|i| !i.gate.is_empty() && i.on != Some(true))
            .filter_map(|i| vk_of_gate(&i.gate))
            .collect();
        blocked.sort_unstable();
        blocked.dedup();
        if let Ok(mut b) = self.shared.blocked.lock() {
            *b = blocked;
        }
        self.shared.attached.store(true, Ordering::SeqCst);
        self.nav.update(state);
    }

    pub fn on_notice(&mut self, show: bool, text: String) {
        self.notice = show.then(|| (text, Instant::now() + Duration::from_secs(120)));
    }

    pub fn on_subtitle(&mut self, seconds: f64, text: String) {
        self.subtitle = Some((text, Instant::now() + Duration::from_secs_f64(seconds.clamp(0.0, 30.0))));
    }

    pub fn bridge_gone(&mut self) {
        self.shared.attached.store(false, Ordering::SeqCst); // the menu is the game's own until the bridge is back
        self.set_open(false);
    }
}

fn blocked_text(why: &str) -> String {
    if matches!(why, "a load" | "no world") {
        "Loading the mod menu...".into()
    } else {
        format!("The menu is not available now ({why}).")
    }
}

impl CairnApp {
    pub fn bridge_send(&self, line: &str) {
        if let Some(b) = &self.bridge {
            b.send(line);
        }
    }

    /// Called every frame while a game runs: keys, asking the mod, where the game's window is, and the overlay.
    pub fn tick_ingame(&mut self, ctx: &egui::Context) {
        let running = self.running.as_ref().map(|r| r.game_id.clone());
        let Some(game_id) = running.filter(|_| self.bridge.is_some()) else {
            self.ingame = None;
            return;
        };
        if self.ingame.is_none() {
            let stem = self
                .registry
                .get(&game_id)
                .map(|g| {
                    std::path::Path::new(&g.default_exe_name)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            self.ingame = Some(InGame::start(&stem, &self.settings.menu_key));
        }
        let connected = self.bridge.as_ref().is_some_and(|b| b.connected());
        let mut sends: Vec<String> = Vec::new();
        let Some(g) = self.ingame.as_mut() else { return };
        let now = Instant::now();
        if now.duration_since(g.pid_at) > Duration::from_secs(2) {
            g.pid_at = now;
            g.pid = runtime::find_process(&g.stem);
            g.shared.pid.store(g.pid.unwrap_or(0), Ordering::SeqCst);
        }
        // until the mod has answered, ask (it learns the launcher draws the menu, and says what it holds)
        if !g.attached() && connected && now >= g.next_attach {
            g.next_attach = now + Duration::from_secs(3);
            sends.push(bridge_protocol::attach());
        }
        while let Ok(key) = g.keys.try_recv() {
            match key {
                MenuKey::Toggle => {
                    if g.nav.open {
                        g.set_open(false);
                    } else if g.attached() {
                        g.set_open(true);
                        g.opened_at = now;
                        g.next_refresh = now + Duration::from_millis(250);
                        sends.push(bridge_protocol::ask_state());
                    }
                }
                MenuKey::Back => {
                    if g.nav.state.submenu {
                        sends.push(bridge_protocol::back()); // a submenu of the game's own program: Back goes to it
                    } else {
                        g.set_open(false);
                    }
                }
                MenuKey::Up => g.nav.step(-1),
                MenuKey::Down => g.nav.step(1),
                k => {
                    if let Some((id, verb, dir)) = g.nav.request(k) {
                        if let Some(m) = bridge_protocol::do_item(&id, verb, dir) {
                            sends.push(m);
                        }
                    }
                }
            }
        }
        // while it is open the picture is asked for once a second (the role, the partner and the connection change by themselves)
        if g.nav.open && now >= g.next_refresh {
            g.next_refresh = now
                + if now.duration_since(g.opened_at) < Duration::from_secs(2) {
                    Duration::from_millis(250)
                } else {
                    Duration::from_secs(1)
                };
            sends.push(bridge_protocol::ask_state());
        }
        if now.duration_since(g.window_at) > Duration::from_millis(120) {
            g.window_at = now;
            g.front = g.pid.is_some_and(winapi::is_foreground);
            if let Some(pid) = g.pid {
                g.rect = winapi::game_client_rect(pid);
            }
        }
        let show_menu = g.nav.open && g.front;
        let subtitle = g
            .subtitle
            .as_ref()
            .filter(|(_, until)| *until > now && g.front)
            .map(|(t, _)| t.clone());
        let notice = g
            .notice
            .as_ref()
            .filter(|(_, until)| *until > now && g.front)
            .map(|(t, _)| t.clone());
        let rect = g.rect;
        let nav = g.nav.clone();
        let settling = !nav.state.blocked.is_empty() && now.duration_since(g.opened_at) < Duration::from_millis(1500);
        let attached = g.attached();
        for s in sends {
            self.bridge_send(&s);
        }
        let active = attached || connected;
        if active {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
        let Some(rect) = rect.filter(|r| r.w > 0 && r.h > 0) else { return };
        if show_menu || subtitle.is_some() || notice.is_some() {
            self.draw_overlay(ctx, rect, show_menu.then_some((&nav, settling)), subtitle, notice);
        }
    }

    /// A transparent, click-through window over the game's picture with the menu, the spoken line and the notice.
    fn draw_overlay(
        &self,
        ctx: &egui::Context,
        rect: ClientRect,
        menu: Option<(&MenuNav, bool)>,
        subtitle: Option<String>,
        notice: Option<String>,
    ) {
        let ppp = ctx.pixels_per_point().max(0.5);
        let builder = egui::ViewportBuilder::default()
            .with_title("Cairn overlay")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_taskbar(false)
            .with_active(false)
            .with_resizable(false)
            .with_position([rect.x as f32 / ppp, rect.y as f32 / ppp])
            .with_inner_size([rect.w as f32 / ppp, rect.h as f32 / ppp]);
        let accent = self.accent;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("cairn-overlay"), builder, |ui, _| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                .show(ui, |ui| {
                    let area = ui.max_rect();
                    if let Some(t) = &notice {
                        plate(
                            ui,
                            egui::pos2(area.center().x, area.top() + 36.0),
                            Align2::CENTER_TOP,
                            t,
                            17.0,
                            accent,
                        );
                    }
                    if let Some(t) = &subtitle {
                        plate(
                            ui,
                            egui::pos2(area.center().x, area.bottom() - 90.0),
                            Align2::CENTER_BOTTOM,
                            t,
                            20.0,
                            theme::PARCHMENT,
                        );
                    }
                    if let Some((nav, settling)) = menu {
                        menu_panel(ui, area, nav, settling, accent);
                    }
                });
        });
    }
}

/// A line of text on a dark plate.
fn plate(ui: &egui::Ui, at: egui::Pos2, anchor: Align2, text: &str, size: f32, color: Color32) {
    let galley = ui
        .painter()
        .layout(text.to_string(), FontId::proportional(size), color, ui.max_rect().width() * 0.7);
    let r = anchor.anchor_size(at, galley.size() + egui::vec2(28.0, 16.0));
    ui.painter().rect_filled(r, 8.0, Color32::from_black_alpha(190));
    ui.painter().galley(r.center() - galley.size() / 2.0, galley, color);
}

fn menu_panel(ui: &mut egui::Ui, area: egui::Rect, nav: &MenuNav, settling: bool, accent: Color32) {
    let state = if settling { &MenuState::empty() } else { &nav.state };
    let width = 440.0_f32.min(area.width() * 0.45);
    let panel = egui::Rect::from_min_size(
        area.left_top() + egui::vec2(48.0, 64.0),
        egui::vec2(width, (area.height() - 128.0).max(200.0)),
    );
    ui.painter().rect_filled(panel, 12.0, Color32::from_black_alpha(215));
    ui.painter().rect_stroke(
        panel,
        12.0,
        egui::Stroke::new(1.0, theme::with_alpha(accent, 0.6)),
        egui::StrokeKind::Inside,
    );
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(panel.shrink(18.0)));
    inner.label(RichText::new(state.title.to_uppercase()).size(17.0).strong().color(accent));
    if !state.blocked.is_empty() && !settling {
        inner.label(RichText::new(blocked_text(&state.blocked)).color(theme::WARN));
    }
    if !state.note.is_empty() {
        inner.label(RichText::new(&state.note).size(13.0).color(theme::PARCHMENT_DIM));
    }
    inner.add_space(6.0);
    let sel = nav.selected_index();
    let mut group = String::new();
    for (i, item) in state.items.iter().enumerate() {
        if item.group != group {
            group = item.group.clone();
            if !group.is_empty() {
                inner.add_space(4.0);
                inner.label(RichText::new(group.to_uppercase()).size(11.0).color(theme::MUTED));
            }
        }
        let selected = sel == Some(i);
        let value = match item.kind {
            MenuKind::OnOff => (if item.on == Some(true) { "ON" } else { "OFF" }).to_string(),
            MenuKind::Info => String::new(),
            _ => item.value.clone(),
        };
        let color = if item.kind == MenuKind::Info {
            theme::MUTED
        } else if item.disabled || item.locked {
            theme::MUTED_DIM
        } else if selected {
            Color32::WHITE
        } else {
            theme::PARCHMENT_DIM
        };
        let (row, _) = inner.allocate_exact_size(egui::vec2(inner.available_width(), 24.0), egui::Sense::hover());
        if selected {
            inner.painter().rect_filled(row, 6.0, theme::with_alpha(accent, 0.28));
        }
        inner.painter().text(
            row.left_center() + egui::vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            &item.label,
            FontId::proportional(14.5),
            color,
        );
        if !value.is_empty() {
            let vc = if item.kind == MenuKind::OnOff && item.on == Some(true) {
                theme::ONLINE
            } else {
                color
            };
            inner.painter().text(
                row.right_center() - egui::vec2(8.0, 0.0),
                Align2::RIGHT_CENTER,
                value,
                FontId::proportional(14.0),
                vc,
            );
        }
    }
    if let Some(item) = nav.selected().filter(|s| !s.help.is_empty() && !settling) {
        let help = egui::Rect::from_min_max(
            egui::pos2(panel.left() + 18.0, panel.bottom() - 60.0),
            panel.right_bottom() - egui::vec2(18.0, 12.0),
        );
        ui.painter().text(
            help.left_top(),
            Align2::LEFT_TOP,
            &item.help,
            FontId::proportional(12.5),
            theme::PARCHMENT_DIM,
        );
    }
}
