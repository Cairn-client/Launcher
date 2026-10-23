// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The Host and Join windows of a pack-run session.

use crate::app::CairnApp;
use crate::ui::theme;
use crate::ui::widgets::{primary_button, secondary_button};
use eframe::egui::{self, RichText, Vec2};

impl CairnApp {
    pub fn session_windows(&mut self, ctx: &egui::Context) {
        if let Some((title, text, next)) = self.session_ui.message.clone() {
            let mut close = false;
            egui::Window::new(RichText::new(title).strong())
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .max_width(480.0)
                .show(ctx, |ui| {
                    ui.add(egui::Label::new(&text).wrap());
                    if !next.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&next).color(theme::PARCHMENT_DIM)).wrap());
                    }
                    if primary_button(ui, "OK", self.accent, true).clicked() {
                        close = true;
                    }
                });
            if close {
                self.session_ui.message = None;
            }
            return;
        }
        let Some(hosting) = self.session_ui.open else { return };
        let v = self.session_view.clone();
        let accent = self.accent;
        let (mut close, mut launch, mut connect, mut friends) = (false, false, false, false);
        let mut use_code = None;
        let idle = matches!(v.stage.as_str(), "idle" | "failed");
        egui::Window::new(RichText::new(if hosting { "HOST A GAME" } else { "JOIN A GAME" }).strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .min_width(460.0)
            .show(ctx, |ui| {
                if hosting {
                    if !v.host.error.is_empty() {
                        ui.colored_label(theme::DANGER_BRIGHT, &v.host.error);
                    } else if v.host.relay_up {
                        ui.colored_label(theme::ONLINE, format!("Your game is open on port {}.", self.settings.host_port));
                    } else {
                        ui.label("Starting your session...");
                    }
                    if let Some(code) = &v.host.steam_code {
                        ui.label(format!("Steam code: {code}"));
                    } else if !v.host.steam_message.is_empty() {
                        ui.label(RichText::new(&v.host.steam_message).color(theme::PARCHMENT_DIM));
                    }
                } else {
                    ui.label("Host's address");
                    ui.text_edit_singleline(&mut self.session_ui.address);
                    ui.label("or Steam code");
                    ui.text_edit_singleline(&mut self.session_ui.steam_code);
                    if secondary_button(
                        ui,
                        if self.session_ui.friends_busy {
                            "Asking Steam..."
                        } else {
                            "Find Steam friends"
                        },
                        !self.session_ui.friends_busy,
                    )
                    .clicked()
                    {
                        friends = true;
                    }
                    if let Some(f) = &self.session_ui.friends {
                        if f.friends.is_empty() {
                            ui.label(
                                RichText::new(if f.message.is_empty() { "No friends playing" } else { &f.message })
                                    .color(theme::PARCHMENT_DIM),
                            );
                        }
                        for fr in &f.friends {
                            if ui.button(&fr.name).clicked() {
                                use_code = Some(fr.code.clone());
                            }
                        }
                    }
                }
                if !v.status.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&v.status).color(theme::PARCHMENT_DIM));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if v.stage == "waiting-connect" {
                        if primary_button(ui, "CONNECT", accent, v.connect_enabled).clicked() {
                            connect = true;
                        }
                    } else if primary_button(
                        ui,
                        if hosting { "START GAME" } else { "JOIN" },
                        accent,
                        idle && (!hosting || v.host.relay_up),
                    )
                    .clicked()
                    {
                        launch = true;
                    }
                    if secondary_button(ui, "CLOSE", true).clicked() {
                        close = true;
                    }
                });
                if !v.connect_reason.is_empty() && v.stage == "waiting-connect" {
                    ui.label(RichText::new(&v.connect_reason).color(theme::WARN));
                }
            });
        if let Some(c) = use_code {
            self.session_ui.steam_code = c;
        }
        if friends {
            self.session_find_friends();
        }
        if launch {
            self.session_launch(hosting);
        }
        if connect {
            self.session_connect();
        }
        if close {
            self.session_ui.open = None;
            if idle && self.running.is_none() {
                self.release_session();
            }
        }
    }
}
