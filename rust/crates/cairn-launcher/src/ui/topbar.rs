// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The top bar: Home, Library and Store on the left; the search, Versions, Help, Settings, the account and the updates on the right.

use super::icons;
use super::theme;
use super::widgets::nav_button;
use crate::app::{CairnApp, View};
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Stroke};

impl CairnApp {
    pub fn top_bar(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent;
        egui::Panel::top("topbar")
            .exact_size(56.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::with_alpha(theme::INK, 0.96))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .stroke(Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.07))),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (label, view) in [("Home", View::Home), ("Library", View::Library), ("Store", View::Store)] {
                        if nav_button(ui, label, self.view == view, accent).clicked() {
                            self.view = view;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        self.updates_button(ui);
                        ui.add_space(8.0);
                        self.account_button(ui);
                        for (label, view) in [("Settings", View::Settings), ("Help", View::Help), ("Versions", View::Versions)] {
                            if nav_button(ui, label, self.view == view, accent).clicked() {
                                self.view = view;
                            }
                        }
                        ui.add_space(8.0);
                        let search = egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search games")
                            .desired_width(230.0)
                            .margin(egui::Margin::symmetric(12, 7))
                            .background_color(theme::TRACK);
                        if ui.add(search).changed() && !self.search.trim().is_empty() && self.view != View::Library {
                            self.view = View::Library;
                        }
                    });
                });
            });
    }

    fn account_button(&mut self, ui: &mut egui::Ui) {
        let size = egui::vec2(34.0, 34.0);
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        let active = self.view == View::Profile;
        ui.painter().circle_filled(
            rect.center(),
            17.0,
            if active {
                theme::with_alpha(self.accent, 0.25)
            } else {
                theme::OAK_LIT
            },
        );
        ui.painter().circle_stroke(
            rect.center(),
            17.0,
            Stroke::new(
                1.0,
                if active {
                    self.accent
                } else {
                    theme::with_alpha(Color32::WHITE, 0.15)
                },
            ),
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "?",
            egui::FontId::proportional(15.0),
            theme::PARCHMENT_DIM,
        );
        if resp
            .on_hover_text("Your account")
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.view = View::Profile;
        }
    }

    fn updates_button(&mut self, ui: &mut egui::Ui) {
        let active = self.downloads.active();
        let pending = self.mod_updates().len() + self.downloads.waiting_count();
        let label = "Updates";
        let font = egui::FontId::proportional(14.0);
        let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), theme::PARCHMENT);
        let size = egui::vec2(galley.size().x + 52.0, 34.0);
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        let tone = if active.is_some() || pending > 0 {
            self.accent
        } else {
            theme::PARCHMENT_DIM
        };
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            if self.updates_open {
                theme::with_alpha(tone, 0.16)
            } else if resp.hovered() {
                theme::with_alpha(Color32::WHITE, 0.06)
            } else {
                Color32::TRANSPARENT
            },
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.0, theme::with_alpha(tone, 0.45)),
            egui::StrokeKind::Inside,
        );
        icons::download(ui.painter(), egui::pos2(rect.left() + 18.0, rect.center().y), tone);
        let galley = ui.painter().layout_no_wrap(label.to_string(), font, theme::PARCHMENT);
        ui.painter().galley(
            egui::pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0),
            galley,
            theme::PARCHMENT,
        );
        if let Some(a) = &active {
            let w = (rect.width() - 8.0) * a.progress.clamp(0.0, 1.0) as f32;
            let y = rect.bottom() - 3.0;
            ui.painter().line_segment(
                [egui::pos2(rect.left() + 4.0, y), egui::pos2(rect.left() + 4.0 + w, y)],
                Stroke::new(2.0, self.accent),
            );
        } else if pending > 0 {
            let badge = egui::pos2(rect.right() - 12.0, rect.center().y);
            ui.painter().circle_filled(badge, 8.0, self.accent);
            ui.painter().text(
                badge,
                egui::Align2::CENTER_CENTER,
                pending.to_string(),
                egui::FontId::proportional(11.0),
                Color32::BLACK,
            );
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.updates_open = !self.updates_open;
        }
    }
}
