// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Your account, laid out like a Steam profile: a banner with your avatar, name and status, your numbers, and recent activity.

use super::theme;
use super::widgets::{card, paint_cover, primary_button};
use crate::app::{CairnApp, VERSION};
use crate::home_data;
use crate::steam_service::{avatar_url, initial};
use cairn_setup::time_util;
use eframe::egui::{self, Color32, Margin, RichText, Sense, Stroke, Vec2};

impl CairnApp {
    pub fn profile_page(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::GROUND))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Frame::new().inner_margin(Margin::symmetric(40, 28)).show(ui, |ui| {
                        ui.set_max_width(1000.0);
                        self.profile_banner(ui);
                        ui.add_space(14.0);
                        self.profile_numbers(ui);
                        ui.add_space(14.0);
                        self.profile_activity(ui);
                        if let Some(about) = self.profile_info.as_ref().map(|i| i.summary.clone()).filter(|s| !s.is_empty()) {
                            ui.add_space(14.0);
                            card(ui, |ui| {
                                ui.label(RichText::new("About").size(18.0).strong());
                                ui.add(egui::Label::new(about).wrap());
                            });
                        }
                    });
                });
            });
    }

    fn profile_banner(&mut self, ui: &mut egui::Ui) {
        let name = self.steam.name().to_string();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 150.0), Sense::hover());
        ui.painter().rect_filled(rect, egui::CornerRadius::same(16), theme::INK);
        let avatar_hash = self
            .profile_info
            .as_ref()
            .map(|i| i.avatar.clone())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| avatar_url(self.steam.avatar(), "full"));
        let tex = self.images.get(&self.ctx, &avatar_hash);
        if let Some(t) = &tex {
            // the picture, blurred by its own size, dimmed behind the text
            paint_cover(ui, t, rect, 16, theme::with_alpha(Color32::WHITE, 0.28));
        }
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(16),
            Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.09)),
            egui::StrokeKind::Inside,
        );
        let av = egui::Rect::from_min_size(rect.min + Vec2::new(24.0, 25.0), Vec2::splat(100.0));
        ui.painter().rect_filled(av, egui::CornerRadius::same(14), theme::OAK_LIT);
        match &tex {
            Some(t) => paint_cover(ui, t, av, 14, Color32::WHITE),
            None => {
                let letter = if name.is_empty() { "?".to_string() } else { initial(&name) };
                ui.painter().text(
                    av.center(),
                    egui::Align2::CENTER_CENTER,
                    letter,
                    egui::FontId::proportional(40.0),
                    theme::PARCHMENT_DIM,
                );
            }
        }
        let x = av.right() + 22.0;
        if name.is_empty() {
            let installed = self.steam.installed;
            ui.painter().text(
                egui::pos2(x, rect.top() + 34.0),
                egui::Align2::LEFT_CENTER,
                if installed { "STEAM IS INSTALLED" } else { "STEAM NOT FOUND" },
                egui::FontId::proportional(11.5),
                theme::MUTED,
            );
            ui.painter().text(
                egui::pos2(x, rect.top() + 64.0),
                egui::Align2::LEFT_CENTER,
                if installed {
                    "Sign in to Steam"
                } else {
                    "Get Steam to play with friends"
                },
                egui::FontId::proportional(26.0),
                theme::PARCHMENT,
            );
            let hint = if installed {
                "Start Steam and sign in: Cairn finds your friends through it, and your profile shows up here."
            } else {
                "Cairn finds your friends and your games through Steam. Install it, sign in, and your profile shows up here."
            };
            ui.painter().text(
                egui::pos2(x, rect.top() + 92.0),
                egui::Align2::LEFT_CENTER,
                hint,
                egui::FontId::proportional(13.0),
                theme::PARCHMENT_DIM,
            );
            let mut button = ui.new_child(
                egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(egui::pos2(x, rect.top() + 104.0), Vec2::new(300.0, 44.0))),
            );
            if primary_button(&mut button, if installed { "Start Steam" } else { "Get Steam" }, self.accent, true).clicked() {
                crate::open::open_url(if installed {
                    "steam://open/main"
                } else {
                    "https://store.steampowered.com/about/"
                });
            }
        } else {
            let state = self
                .profile_info
                .as_ref()
                .map(|i| i.state.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "online".into());
            let tone = match state.as_str() {
                "in-game" => theme::ONLINE,
                "online" => theme::FOCUS,
                _ => theme::MUTED,
            };
            ui.painter().text(
                egui::pos2(x, rect.top() + 36.0),
                egui::Align2::LEFT_CENTER,
                &name,
                egui::FontId::proportional(28.0),
                theme::PARCHMENT,
            );
            let msg = self
                .profile_info
                .as_ref()
                .map(|i| i.state_message.clone())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "Online".into());
            ui.painter().text(
                egui::pos2(x, rect.top() + 66.0),
                egui::Align2::LEFT_CENTER,
                msg,
                egui::FontId::proportional(14.0),
                tone,
            );
            if let Some(i) = &self.profile_info {
                let who: Vec<&str> = [i.real_name.as_str(), i.location.as_str()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect();
                if !who.is_empty() {
                    ui.painter().text(
                        egui::pos2(x, rect.top() + 90.0),
                        egui::Align2::LEFT_CENTER,
                        who.join(" · "),
                        egui::FontId::proportional(13.0),
                        theme::MUTED,
                    );
                }
            }
            let since = self
                .profile_info
                .as_ref()
                .and_then(|i| i.member_since)
                .map(|(y, m, _)| format!("Member of Steam since {} {y} · ", MONTHS[(m.clamp(1, 12) - 1) as usize]))
                .unwrap_or_default();
            ui.painter().text(
                egui::pos2(x, rect.top() + 114.0),
                egui::Align2::LEFT_CENTER,
                format!("{since}Cairn {VERSION}"),
                egui::FontId::proportional(12.5),
                theme::MUTED,
            );
            let id = self.steam.id().to_string();
            if !id.is_empty() {
                // the id is shown on hover only: it names the account
                let r = egui::Rect::from_min_size(egui::pos2(rect.right() - 260.0, rect.top() + 14.0), Vec2::new(246.0, 24.0));
                let resp = ui.interact(r, ui.make_persistent_id("steamid"), Sense::hover());
                let text = if resp.hovered() {
                    format!("Steam id {id}")
                } else {
                    "Steam id ••••••••••••".to_string()
                };
                ui.painter().text(
                    r.right_center(),
                    egui::Align2::RIGHT_CENTER,
                    text,
                    egui::FontId::monospace(11.5),
                    theme::MUTED,
                );
            }
        }
    }

    fn profile_numbers(&mut self, ui: &mut egui::Ui) {
        let sessions: i32 = self.recent.entries.iter().map(|e| e.sessions).sum();
        let minutes: i32 = self.recent.entries.iter().map(|e| e.minutes).sum();
        let games: Vec<_> = self.registry.all().into_iter().cloned().collect();
        let mods = games.iter().filter(|g| self.is_mod_installed(g)).count();
        if sessions == 0 && mods == 0 {
            return;
        }
        ui.horizontal(|ui| {
            for (value, label) in [
                (sessions.to_string(), if sessions == 1 { "session" } else { "sessions" }.to_string()),
                (
                    if minutes > 0 { home_data::played(minutes) } else { "-".into() },
                    "played on Cairn".to_string(),
                ),
                (
                    mods.to_string(),
                    if mods == 1 { "mod installed" } else { "mods installed" }.to_string(),
                ),
            ] {
                card(ui, |ui| {
                    ui.set_min_width(170.0);
                    ui.label(RichText::new(value).size(24.0).strong());
                    ui.label(RichText::new(label).color(theme::MUTED));
                });
            }
        });
    }

    fn profile_activity(&mut self, ui: &mut egui::Ui) {
        let mut rows: Vec<_> = self.recent.entries.clone();
        rows.sort_by_key(|r| std::cmp::Reverse(r.last_secs()));
        let total: i32 = rows.iter().map(|r| r.minutes).sum();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Recent activity").size(18.0).strong());
                if total > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{} played on Cairn", home_data::played(total))).color(theme::MUTED));
                    });
                }
            });
            if rows.is_empty() {
                ui.label(RichText::new("Nothing played yet. Host or join a game and it shows up here.").color(theme::MUTED));
            }
            for r in rows.iter().take(8) {
                let Some(game) = self.registry.get(&r.game_id).cloned() else {
                    continue;
                };
                ui.horizontal(|ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(120.0, 52.0), Sense::click());
                    ui.painter().rect_filled(rect, egui::CornerRadius::same(8), theme::INK);
                    if let Some(t) = self.images.get(
                        &self.ctx,
                        if game.background_image.is_empty() {
                            &game.poster_image
                        } else {
                            &game.background_image
                        },
                    ) {
                        paint_cover(ui, &t, rect, 8, theme::with_alpha(Color32::WHITE, 0.7));
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.open_game(&game.id);
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&game.display_name).strong());
                        let detail = if r.minutes > 0 {
                            format!("{} on record", home_data::played(r.minutes))
                        } else if r.sessions == 1 {
                            "1 session".into()
                        } else {
                            format!("{} sessions", r.sessions)
                        };
                        let (_, m, d) = time_util::civil_from_days(r.last_secs().div_euclid(86_400));
                        ui.label(
                            RichText::new(format!("{detail} · last played on {d} {}", MONTHS[(m.clamp(1, 12) - 1) as usize]))
                                .color(theme::MUTED),
                        );
                    });
                });
                ui.add_space(4.0);
            }
        });
    }
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
