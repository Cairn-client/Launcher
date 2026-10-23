// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The ready check ("is everything in place to play?") and the setup checklist ("Before you play") windows.

use crate::app::CairnApp;
use crate::preflight::{self, CheckStatus};
use crate::setup_list::LineState;
use crate::ui::theme;
use crate::ui::widgets::{primary_button, secondary_button};
use eframe::egui::{self, RichText, Vec2};

impl CairnApp {
    pub fn ready_check_windows(&mut self, ctx: &egui::Context) {
        self.setup_window(ctx);
        self.preflight_window(ctx);
    }

    fn preflight_window(&mut self, ctx: &egui::Context) {
        if !self.preflight_open {
            return;
        }
        let Some(game) = self.selected_profile().cloned() else {
            self.preflight_open = false;
            return;
        };
        // the lines follow what changes underneath (Steam starting, the mod arriving)
        self.preflight_items = self.preflight_for(&game);
        let items = self.preflight_items.clone();
        let can_play = preflight::can_play(&items);
        let accent = self.accent;
        let (mut close, mut play) = (false, false);
        let mut fix = None;
        egui::Window::new(RichText::new("READY CHECK").strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .min_width(460.0)
            .show(ctx, |ui| {
                for it in &items {
                    ui.horizontal(|ui| {
                        let (text, color) = match it.status {
                            CheckStatus::Ok => ("OK", theme::ONLINE),
                            CheckStatus::Warn => ("WARN", theme::WARN),
                            CheckStatus::Fail => ("FIX", theme::DANGER_BRIGHT),
                        };
                        ui.label(RichText::new(text).strong().color(color));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&it.title).strong());
                            if !it.detail.is_empty() {
                                ui.add(egui::Label::new(RichText::new(&it.detail).color(theme::PARCHMENT_DIM)).wrap());
                            }
                        });
                        if let Some(f) = it.fix {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if secondary_button(ui, f.label(), true).clicked() {
                                    fix = Some(f);
                                }
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, "PLAY", accent, can_play).clicked() {
                        play = true;
                    }
                    if secondary_button(ui, "CLOSE", true).clicked() {
                        close = true;
                    }
                });
            });
        if let Some(f) = fix {
            self.apply_fix(&game.id, f);
        }
        if play {
            self.preflight_open = false;
            self.play_external(&game.id);
        }
        if close {
            self.preflight_open = false;
        }
    }

    fn setup_window(&mut self, ctx: &egui::Context) {
        if !self.setup_open {
            return;
        }
        let game_id = self.setup_game.clone();
        let lines = self.setup_lines.clone();
        let accent = self.accent;
        let busy = self.setup_busy.clone();
        let (mut close, mut act) = (false, None);
        egui::Window::new(RichText::new("BEFORE YOU PLAY").strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .min_width(480.0)
            .show(ctx, |ui| {
                if lines.is_empty() {
                    ui.label(if self.setup_checking {
                        "Checking this PC..."
                    } else {
                        "Nothing to set up."
                    });
                }
                for l in &lines {
                    ui.horizontal(|ui| {
                        let (text, color) = match l.state {
                            LineState::Done => ("DONE", theme::ONLINE),
                            LineState::Working => ("...", theme::MUTED),
                            LineState::NeedsYou => ("TODO", theme::DANGER_BRIGHT),
                            LineState::Warn => ("WARN", theme::WARN),
                        };
                        ui.label(RichText::new(text).strong().color(color));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&l.title).strong());
                            if !l.detail.is_empty() {
                                ui.add(egui::Label::new(RichText::new(&l.detail).color(theme::PARCHMENT_DIM)).wrap());
                            }
                            if let Some(p) = l.progress {
                                ui.add(egui::ProgressBar::new(p as f32).desired_width(280.0));
                            }
                        });
                        if let Some((action, label)) = &l.action {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if secondary_button(ui, label, busy.is_none() && l.state != LineState::Working).clicked() {
                                    act = Some((l.id.clone(), action.clone()));
                                }
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(6.0);
                if primary_button(ui, "CLOSE", accent, true).clicked() {
                    close = true;
                }
            });
        if let Some((step, action)) = act {
            self.do_setup_action(&game_id, &step, action);
        }
        if close {
            self.setup_open = false;
        }
    }
}
