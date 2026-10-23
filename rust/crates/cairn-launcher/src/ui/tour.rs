// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The welcome guide (shown once, and again from Help) and the "quit?" confirmation.

use crate::app::CairnApp;
use crate::runtime;
use crate::ui::theme;
use crate::ui::widgets::{primary_button, secondary_button};
use eframe::egui::{self, RichText, Vec2};
use std::time::{Duration, Instant};

const STEPS: usize = 5;

impl CairnApp {
    pub fn tour_window(&mut self, ctx: &egui::Context) {
        if self.prefs.tour_seen || std::env::var_os("CAIRN_SCREENSHOT").is_some() {
            return;
        }
        if self.tour_steam.0.elapsed() > Duration::from_secs(2) {
            self.tour_steam = (Instant::now(), runtime::steam_running());
            ctx.request_repaint_after(Duration::from_secs(2));
        }
        let steam = self.tour_steam.1;
        let step = self.tour_step.min(STEPS - 1);
        let accent = self.accent;
        let found: Vec<(String, String)> = self
            .registry
            .all()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .filter(|g| self.is_game_found(g))
            .map(|g| (g.id, g.display_name))
            .collect();
        let selected = self.registry.get(&self.selected_game).cloned();
        let mod_ready = selected.as_ref().is_some_and(|g| self.is_mod_installed(g));
        let busy = selected.as_ref().and_then(|g| self.downloads.item_of(&g.id));
        let friends = self.friend_status.len();
        let (mut skip, mut next, mut back, mut start_steam, mut download) = (false, false, false, false, false);
        let mut pick = None;
        egui::Window::new(RichText::new(format!("STEP {} OF {STEPS}", step + 1)).color(theme::PARCHMENT_DIM))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(520.0, 280.0))
            .show(ctx, |ui| {
                match step {
                    0 => {
                        ui.label(RichText::new("Welcome to Cairn").size(22.0).strong());
                        ui.add(egui::Label::new("Cairn turns single-player games into shared ones. One of you hosts, a friend joins, and you play in the same world. First, Steam: Cairn finds your games and your friends through it.").wrap());
                        if steam {
                            ui.colored_label(theme::ONLINE, "Steam is running.");
                        } else {
                            ui.colored_label(theme::WARN, "Steam is not running.");
                            if secondary_button(ui, "START STEAM", true).clicked() {
                                start_steam = true;
                            }
                        }
                    }
                    1 => {
                        ui.label(RichText::new("Pick your game").size(22.0).strong());
                        if found.is_empty() {
                            ui.add(egui::Label::new("Cairn did not find a supported game on this PC yet. Install one from your Steam library (the Library page lists the games Cairn supports), then come back here from Help.").wrap());
                        } else {
                            ui.label("These are the games Cairn found on this PC. Pick the one you want to play together.");
                            ui.horizontal_wrapped(|ui| {
                                for (id, name) in &found {
                                    if ui.selectable_label(id.eq_ignore_ascii_case(&self.selected_game), name).clicked() {
                                        pick = Some(id.clone());
                                    }
                                }
                            });
                        }
                    }
                    2 => {
                        ui.label(RichText::new("Get the mod").size(22.0).strong());
                        match &selected {
                            None => {
                                ui.label("Pick a game in the step before, then its mod can be downloaded here.");
                            }
                            Some(g) => {
                                ui.label(format!("The mod is a small download that installs itself in {}, once.", g.display_name));
                                if let Some(b) = busy {
                                    ui.label(format!("Downloading ({}%)", (b.progress * 100.0) as i32));
                                } else if mod_ready {
                                    ui.colored_label(theme::ONLINE, format!("The {} mod is installed.", g.display_name));
                                } else if secondary_button(ui, "DOWNLOAD THE MOD", true).clicked() {
                                    download = true;
                                }
                            }
                        }
                    }
                    3 => {
                        ui.label(RichText::new("Play with a friend").size(22.0).strong());
                        ui.add(egui::Label::new("Press Host a game on the game's page and your friends see you in their list, or open Join a game and pick the friend who is hosting. Both of you need the same version of Cairn.").wrap());
                        if friends > 0 {
                            ui.colored_label(theme::ONLINE, format!("{friends} Steam friends found."));
                        } else {
                            ui.colored_label(theme::WARN, "No Steam friends found yet. Add your friend on Steam first; they appear here by themselves.");
                        }
                    }
                    _ => {
                        ui.label(RichText::new("Everything else is in the top bar").size(22.0).strong());
                        ui.add(egui::Label::new("Settings holds your options, Versions shows what changed in each release, and Help has answers and a way to report a problem. This guide is in Help too.").wrap());
                    }
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if secondary_button(ui, if step == STEPS - 1 { "CLOSE" } else { "SKIP" }, true).clicked() {
                        skip = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary_button(ui, if step == STEPS - 1 { "GET STARTED" } else { "NEXT" }, accent, true).clicked() {
                            next = true;
                        }
                        if step > 0 && secondary_button(ui, "BACK", true).clicked() {
                            back = true;
                        }
                    });
                });
            });
        if start_steam {
            crate::open::open_url("steam://open/main");
        }
        if let Some(id) = pick {
            self.selected_game = id;
        }
        if download {
            let id = self.selected_game.clone();
            self.download_pack(&id);
        }
        if back {
            self.tour_step = step - 1;
        }
        if next && step < STEPS - 1 {
            self.tour_step = step + 1;
        } else if skip || next {
            self.prefs.tour_seen = true;
            self.tour_step = 0;
            self.save_prefs();
        }
    }

    #[allow(deprecated)]
    /// Asks before the window closes when the player wants that (a setting).
    pub fn exit_window(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.settings.confirm_quit && !self.quit_ok {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_exit = true;
        }
        if !self.confirm_exit {
            return;
        }
        let (mut quit, mut stay) = (false, false);
        egui::Area::new(egui::Id::new("quit-scrim"))
            .order(egui::Order::Middle)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                let screen = ctx.content_rect();
                let r = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter().rect_filled(screen, 0.0, theme::with_alpha(egui::Color32::BLACK, 0.55));
                if r.clicked() {
                    stay = true;
                }
            });
        let width = 380.0;
        egui::Area::new(egui::Id::new("quit-dialog"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::INK)
                    .stroke(egui::Stroke::new(1.0, theme::with_alpha(egui::Color32::WHITE, 0.1)))
                    .corner_radius(egui::CornerRadius::same(16))
                    .inner_margin(egui::Margin::same(28))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("Quit Cairn?").size(22.0).strong());
                            ui.add_space(6.0);
                            ui.label(RichText::new("Are you sure you want to quit the launcher?").color(theme::PARCHMENT_DIM));
                            ui.add_space(16.0);
                            // the two buttons, centred
                            let font = egui::FontId::proportional(13.5);
                            let wide = |ui: &egui::Ui, s: &str| {
                                ui.painter().layout_no_wrap(s.to_string(), font.clone(), theme::PARCHMENT).size().x + 32.0
                            };
                            let row = wide(ui, "QUIT") + wide(ui, "CANCEL") + ui.spacing().item_spacing.x;
                            ui.horizontal(|ui| {
                                ui.add_space(((width - row) / 2.0).max(0.0));
                                if primary_button(ui, "Quit", self.accent, true).clicked() {
                                    quit = true;
                                }
                                if secondary_button(ui, "Cancel", true).clicked() {
                                    stay = true;
                                }
                            });
                            ui.add_space(10.0);
                            let mut ask = self.settings.confirm_quit;
                            if ui.checkbox(&mut ask, "Ask me before quitting").changed() {
                                self.settings.confirm_quit = ask;
                                self.write_settings("quit confirmation");
                            }
                        });
                    });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            quit = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            stay = true;
        }
        if quit {
            self.quit_ok = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if stay {
            self.confirm_exit = false;
        }
    }
}
