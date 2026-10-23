// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Settings: the launcher's own settings as a page (tabs, and a search across all of them), and a game's own settings in a window opened from
//! its page. The values are keys of `settings.json`; only what the player changed is written, when they press Save.

use super::theme;
use super::widgets::{card, column, primary_button, secondary_button};
use crate::app::CairnApp;
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};

const TABS: [(&str, &str); 6] = [
    ("general", "General"),
    ("appearance", "Appearance"),
    ("notifications", "Notifications"),
    ("privacy", "Privacy"),
    ("hosting", "Hosting"),
    ("support", "Support"),
];

/// every setting: its tab, and the words it can be found by
const INDEX: [(&str, &str, &str); 16] = [
    ("startpage", "general", "start page home library opens first launch"),
    ("confirmquit", "general", "escape key ask close quit exit confirm"),
    ("accent", "appearance", "accent colour color theme gold blue green pink violet red"),
    ("motion", "appearance", "reduce motion animation animations fade spinner"),
    ("slideshow", "appearance", "slideshow slides home change automatically carousel"),
    (
        "notifyfriends",
        "notifications",
        "friends online notifications playing invite tell me",
    ),
    ("traynotices", "notifications", "windows notifications tray toast hosting invite"),
    ("presence", "privacy", "presence online invite friends cairn server see me"),
    ("friendstatus", "privacy", "friends status pictures avatars steam profile community"),
    ("addr", "privacy", "show ip address host window hidden stream share screen"),
    ("port", "hosting", "host port network tcp address 7778 listen"),
    ("steamjoin", "hosting", "let friends join through steam when i host"),
    ("connectgate", "hosting", "connect button wait world loaded gate"),
    ("takeover", "hosting", "main menu start game join game takeover"),
    ("logs", "support", "save the logs zip bug report desktop game agent launcher"),
    ("uninstall", "support", "maintenance uninstall cairn remove launcher mod"),
];

fn matches(id: &str, query: &str) -> bool {
    let Some((_, tab, words)) = INDEX.iter().find(|i| i.0 == id) else {
        return false;
    };
    let text = format!("{words} {tab}").to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| text.contains(w))
}

impl CairnApp {
    /// The Settings page.
    pub fn settings_page(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::GROUND))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    column(ui, 760.0, 40.0, 28, |ui| {
                        ui.set_max_width(760.0);
                        ui.label(RichText::new("Settings").size(32.0).strong());
                        ui.add_space(10.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings_query)
                                .hint_text("Search settings")
                                .desired_width(f32::INFINITY)
                                .margin(Margin::symmetric(12, 8))
                                .background_color(theme::TRACK),
                        );
                        ui.add_space(8.0);
                        let searching = !self.settings_query.trim().is_empty();
                        if !searching {
                            ui.horizontal_wrapped(|ui| {
                                for (id, title) in TABS {
                                    let on = self.settings_tab == id;
                                    if tab_button(ui, title, on, self.accent).clicked() {
                                        self.settings_tab = id.to_string();
                                    }
                                }
                            });
                        } else if !INDEX.iter().any(|i| matches(i.0, &self.settings_query)) {
                            ui.label(RichText::new(format!("No setting matches \"{}\".", self.settings_query)).color(theme::MUTED));
                        }
                        ui.add_space(8.0);
                        let (q, tab) = (self.settings_query.clone(), self.settings_tab.clone());
                        let show = |id: &str| {
                            if searching {
                                matches(id, &q)
                            } else {
                                INDEX.iter().any(|i| i.0 == id && i.1 == tab)
                            }
                        };
                        self.general_settings(ui, &show);
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "SAVE", self.accent, true).clicked() {
                                self.save_settings("the settings page");
                            }
                            if secondary_button(ui, "CANCEL", true).clicked() {
                                let reverted = self.settings_store.revert(&mut self.settings);
                                self.refresh_accent();
                                if !reverted.is_empty() {
                                    self.toast("The changes were not saved.", false);
                                }
                            }
                        });
                    });
                });
            });
    }

    fn general_settings(&mut self, ui: &mut egui::Ui, show: &dyn Fn(&str) -> bool) {
        let accent = self.accent;
        if show("startpage") {
            field(ui, "Page shown when Cairn starts", None, |ui| {
                let s = &mut self.settings.start_page;
                egui::ComboBox::from_id_salt("startpage")
                    .selected_text(if s == "library" { "Library" } else { "Home" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(s, "home".to_string(), "Home");
                        ui.selectable_value(s, "library".to_string(), "Library");
                    });
            });
        }
        if show("confirmquit") {
            check_field(
                ui,
                &mut self.settings.confirm_quit,
                "Pressing Escape on a page asks whether to close Cairn",
                Some("Off: Escape does nothing there (it still closes a window or a menu that is open)."),
            );
        }
        if show("accent") {
            field(
                ui,
                "Accent colour",
                Some("The colour of buttons, rings and highlights. It shows when you save."),
                |ui| {
                    ui.horizontal(|ui| {
                        for (id, color, _) in theme::ACCENTS {
                            let c = theme::hex(color).unwrap_or(accent);
                            let on = self.settings.accent_color == id;
                            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
                            ui.painter().circle_filled(rect.center(), 12.0, c);
                            if on {
                                ui.painter().circle_stroke(rect.center(), 15.0, Stroke::new(2.0, theme::PARCHMENT));
                            }
                            if resp.on_hover_text(id).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.settings.accent_color = id.to_string();
                                self.refresh_accent();
                            }
                        }
                    });
                },
            );
        }
        if show("motion") {
            check_field(
                ui,
                &mut self.settings.reduce_motion,
                "Reduce motion",
                Some("Turns the animations off: the spinning rings, the slideshow's slides and the fades."),
            );
        }
        if show("slideshow") {
            check_field(
                ui,
                &mut self.settings.slideshow_auto,
                "Change the slides on Home by themselves",
                None,
            );
        }
        if show("notifyfriends") {
            check_field(
                ui,
                &mut self.settings.notify_friends,
                "Tell me when a friend comes online, starts playing or invites me",
                None,
            );
        }
        if show("traynotices") {
            check_field(
                ui,
                &mut self.settings.tray_notices,
                "Also show Windows notifications (when a friend hosts, or invites me)",
                Some("So you see them while the game is in front or Cairn is minimised."),
            );
        }
        if show("presence") {
            check_field(
                ui,
                &mut self.settings.presence_enabled,
                "Let my Steam friends who use Cairn see when I am online and invite me",
                Some("Your Steam name and friend ids go to the Cairn server while the launcher is open. A friend is shown only when you both list each other. Takes effect the next time the launcher starts."),
            );
        }
        if show("friendstatus") {
            check_field(
                ui,
                &mut self.settings.friend_status,
                "Show what my Steam friends are doing, and their pictures",
                Some("Cairn asks each friend's public Steam profile page (steamcommunity.com) and loads their picture from Steam. Off: the friends list shows names only, and nothing is asked of Steam."),
            );
        }
        if show("addr") {
            check_field(
                ui,
                &mut self.settings.show_host_address,
                "Show my IP address in the Host window (hidden by default)",
                Some("Only for friends who join by address instead of Steam. Keep it hidden when you stream or share your screen."),
            );
        }
        if show("port") {
            field(ui, "Host port", None, |ui| {
                let mut port = self.settings.host_port;
                if ui.add(egui::DragValue::new(&mut port).range(1..=65535).speed(1.0)).changed() {
                    self.settings.host_port = port;
                }
            });
        }
        if show("steamjoin") {
            check_field(
                ui,
                &mut self.settings.host_allow_steam,
                "Let friends join me through Steam (when I host)",
                None,
            );
        }
        if show("connectgate") {
            check_field(
                ui,
                &mut self.settings.connect_gate,
                "Wait until the world is loaded before the CONNECT button works",
                Some("The button says why it waits. Off: it is always enabled."),
            );
        }
        if show("takeover") {
            check_field(
                ui,
                &mut self.settings.menu_takeover,
                "Show Cairn's Start Game / Join Game on the game's main menu",
                Some("For games Cairn starts. Off: the game's own menu, as it comes."),
            );
        }
        if show("logs") {
            field(
                ui,
                "",
                Some("One zip with the game's, the agent's and the launcher's logs. Best saved after you close the game."),
                |ui| {
                    if secondary_button(ui, "SAVE THE LOGS", true).clicked() {
                        self.save_logs();
                    }
                },
            );
        }
        if show("uninstall") {
            field(ui, "", None, |ui| {
                if !self.confirm_uninstall {
                    if secondary_button(ui, "UNINSTALL CAIRN", true).clicked() {
                        self.confirm_uninstall = true;
                    }
                    ui.add(egui::Label::new(RichText::new("Removes the mod from your games and this launcher. Your game saves stay, and so do the mod's own data (your characters, chests and start save, copied to Documents\\Cairn-kept).").color(theme::MUTED).size(12.5)).wrap());
                } else {
                    ui.label(
                        RichText::new("Remove Cairn now? The launcher closes and the uninstaller removes it. Your saves are kept.")
                            .color(theme::WARN),
                    );
                    ui.horizontal(|ui| {
                        if primary_button(ui, "YES, UNINSTALL", accent, true).clicked() {
                            self.uninstall();
                        }
                        if secondary_button(ui, "KEEP IT", true).clicked() {
                            self.confirm_uninstall = false;
                        }
                    });
                }
            });
        }
    }

    /// Writes what the player changed (the Save button).
    pub fn save_settings(&mut self, why: &str) {
        let r = self.settings_store.save(&self.settings, None);
        if !r.ok() {
            self.toast(&format!("The settings were not saved ({}).", r.result.why), true);
            return;
        }
        let _ = why;
        self.refresh_accent();
        self.toast(if r.wrote() { "Settings saved." } else { "Nothing changed." }, false);
    }

    /// Takes the accent colour of the settings (it shows at once).
    pub fn refresh_accent(&mut self) {
        self.accent = theme::accent_of(&self.settings.accent_color).0;
        theme::install(&self.ctx, self.accent);
    }

    /// One zip of the logs on the Desktop.
    pub fn save_logs(&mut self) {
        match crate::logs::save_bundle() {
            Ok(path) => self.toast(&format!("Logs saved: {}", path.display()), false),
            Err(e) => self.toast(&format!("Could not save the logs ({e})."), true),
        }
    }

    /// Hands over to the uninstaller (Windows) and closes the launcher.
    pub fn uninstall(&mut self) {
        self.confirm_uninstall = false;
        match crate::logs::start_uninstaller() {
            Ok(()) => self.ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => self.toast(&format!("Could not start the uninstaller ({e})."), true),
        }
    }

    // ---- a game's own settings -----------------------------------------------------------------------------------------

    /// Opens a game's settings window.
    pub fn open_game_settings(&mut self, game_id: &str) {
        let taken = self.settings_store.refresh(&mut self.settings);
        if !taken.is_empty() {
            self.toast("Some settings changed outside the launcher: they are taken.", false);
        }
        self.game_settings_open = Some(game_id.to_string());
        self.game_settings_tab = "settings".into();
        if let Some(g) = self.registry.get(game_id).cloned() {
            self.load_game_extras(&g);
        }
    }

    pub fn game_settings_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.game_settings_open.clone() else { return };
        let Some(game) = self.registry.get(&id).cloned() else {
            self.game_settings_open = None;
            return;
        };
        let accent = self.accent;
        let mut close = false;
        let mut save = false;
        egui::Window::new(RichText::new(game.display_name.to_uppercase()).strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .min_width(520.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("Game settings").color(theme::MUTED));
                ui.add_space(8.0);
                let has_saves = self.game_has_saves(&game);
                if has_saves {
                    ui.horizontal(|ui| {
                        for (key, label) in [("settings", "Settings"), ("saves", "Saves")] {
                            if tab_button(ui, label, self.game_settings_tab == key, accent).clicked() {
                                self.game_settings_tab = key.into();
                            }
                        }
                    });
                    ui.add_space(6.0);
                }
                if has_saves && self.game_settings_tab == "saves" {
                    self.saves_tab(ui);
                    ui.add_space(12.0);
                    if secondary_button(ui, "CLOSE", true).clicked() {
                        close = true;
                        self.settings_store.revert(&mut self.settings);
                    }
                    return;
                }
                if game.exe_is_setting {
                    ui.label(RichText::new("Game executable path").size(11.5).color(theme::MUTED).strong());
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.settings.game_path).desired_width(360.0).background_color(theme::TRACK));
                        if secondary_button(ui, "BROWSE", true).clicked() {
                            if let Some(p) = rfd::FileDialog::new().set_title(format!("Select {}", game.default_exe_name)).add_filter("Executables", &["exe"]).pick_file() {
                                self.settings.game_path = p.to_string_lossy().into_owned();
                                self.finder.forget(&game.id);
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
                if !game.mod_repo.is_empty() {
                    ui.label(RichText::new("UPDATES").size(11.5).color(theme::MUTED).strong());
                    let mut beta = self.settings.channel_of(&game.id) == "beta";
                    if ui.checkbox(&mut beta, "Receive beta versions of this mod").changed() {
                        self.settings.mod_channels.insert(game.id.clone(), crate::mods::channel_name(beta).to_string());
                        self.refresh_catalog_and_packs();
                    }
                    ui.add(egui::Label::new(RichText::new("Beta versions offer the latest multiplayer features and fixes earlier. Both players in a session should use the same version.").color(theme::MUTED).size(12.5)).wrap());
                } else if !game.exe_is_setting {
                    ui.label(RichText::new(format!("{} has no settings yet. They appear here once multiplayer for this game is available.", game.display_name)).color(theme::MUTED));
                }
                if !game.options.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("OPTIONS").size(11.5).color(theme::MUTED).strong());
                    for option in &game.options {
                        let on = self.settings.game_options.get(&game.id).and_then(|o| o.get(&option.id)).copied().unwrap_or(false);
                        let mut now = on;
                        let st = self.option_status.get(&option.id).cloned();
                        let usable = st.as_ref().is_none_or(|s| s.available);
                        if ui.add_enabled(usable && !self.options_busy, egui::Checkbox::new(&mut now, &option.title)).changed() {
                            self.settings.game_options.entry(game.id.clone()).or_default().insert(option.id.clone(), now);
                        }
                        if let Some(n) = st.as_ref().map(|s| s.note.clone()).filter(|n| !n.is_empty()) {
                            ui.add(egui::Label::new(RichText::new(n).color(theme::WARN).size(12.5)).wrap());
                        }
                        if !option.hint.is_empty() {
                            ui.add(egui::Label::new(RichText::new(&option.hint).color(theme::MUTED).size(12.5)).wrap());
                        }
                    }
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, "SAVE & CLOSE", accent, true).clicked() {
                        save = true;
                    }
                    if secondary_button(ui, "CANCEL", true).clicked() {
                        close = true;
                        self.settings_store.revert(&mut self.settings);
                        self.refresh_accent();
                    }
                });
            });
        if save {
            self.save_settings("the game settings window");
            self.apply_game_options(&game);
            self.game_settings_open = None;
        } else if close {
            self.game_settings_open = None;
        }
        let _ = (Color32::TRANSPARENT, CornerRadius::ZERO);
    }
}

impl CairnApp {
    fn saves_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Saved games").size(11.5).color(theme::MUTED).strong());
        let running = self.running.is_some();
        let Some(slots) = self.saves.clone() else {
            ui.label(RichText::new("The game's saves are listed here once its mod is installed.").color(theme::MUTED));
            return;
        };
        if slots.is_empty() {
            ui.label(RichText::new("No saves yet.").color(theme::MUTED));
        }
        let mut delete = None;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for s in &slots {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&s.name).strong());
                        ui.label(RichText::new(&s.detail).color(theme::MUTED).size(12.5));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let asked = self.confirm_delete_save == s.id;
                        if secondary_button(ui, if asked { "SURE? DELETE" } else { "DELETE" }, !self.saves_loading).clicked() {
                            if asked {
                                delete = Some(s.id.clone());
                            } else {
                                self.confirm_delete_save = s.id.clone();
                            }
                        }
                        if let Some(w) = s.when {
                            ui.label(RichText::new(crate::home_data::ago(w, cairn_setup::time_util::now_utc())).color(theme::MUTED));
                        }
                    });
                });
            }
        });
        if running {
            ui.label(
                RichText::new("The game is running: a save written in the last ten minutes is kept, since the game may be using it.")
                    .color(theme::MUTED)
                    .size(12.5),
            );
        }
        ui.add_space(8.0);
        if self.confirm_delete_all {
            ui.label(RichText::new("Delete every save of the game and everything the mod kept? This cannot be undone.").color(theme::WARN));
            ui.horizontal(|ui| {
                if secondary_button(ui, "YES, DELETE ALL", true).clicked() {
                    self.delete_all_saves();
                }
                if secondary_button(ui, "CANCEL", true).clicked() {
                    self.confirm_delete_all = false;
                }
            });
        } else if secondary_button(ui, "DELETE ALL SAVES", !running && !slots.is_empty()).clicked() {
            self.confirm_delete_all = true;
        }
        if let Some(id) = delete {
            self.delete_save(&id);
        }
    }
}

fn tab_button(ui: &mut egui::Ui, text: &str, on: bool, accent: Color32) -> egui::Response {
    let color = if on { accent } else { theme::PARCHMENT_DIM };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), egui::FontId::proportional(13.5), color);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(24.0, 14.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(255),
        if on {
            theme::with_alpha(accent, 0.14)
        } else if resp.hovered() {
            theme::with_alpha(Color32::WHITE, 0.05)
        } else {
            Color32::TRANSPARENT
        },
    );
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn field(ui: &mut egui::Ui, label: &str, hint: Option<&str>, add: impl FnOnce(&mut egui::Ui)) {
    card(ui, |ui| {
        if !label.is_empty() {
            ui.label(RichText::new(label.to_uppercase()).size(11.0).color(theme::MUTED).strong());
        }
        add(ui);
        if let Some(h) = hint {
            ui.add(egui::Label::new(RichText::new(h).color(theme::MUTED).size(12.5)).wrap());
        }
    });
    ui.add_space(2.0);
}

fn check_field(ui: &mut egui::Ui, value: &mut bool, label: &str, hint: Option<&str>) {
    card(ui, |ui| {
        ui.checkbox(value, label);
        if let Some(h) = hint {
            ui.add(egui::Label::new(RichText::new(h).color(theme::MUTED).size(12.5)).wrap());
        }
    });
    ui.add_space(2.0);
}
