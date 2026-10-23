// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The short notes in the bottom right corner of the window.

use super::theme;
use crate::app::CairnApp;
use eframe::egui::{self, Align2, Color32, CornerRadius, Stroke};

impl CairnApp {
    pub fn toasts_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut y = -18.0;
        for (i, t) in self.toasts.iter().enumerate().rev() {
            #[allow(deprecated)]
            let id = egui::Id::new(("toast", i));
            egui::Area::new(id)
                .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-18.0, y))
                .order(egui::Order::Tooltip)
                .interactable(false)
                .show(&ctx, |ui| {
                    let tone = if t.error { theme::DANGER_BRIGHT } else { self.accent };
                    egui::Frame::new()
                        .fill(theme::OAK)
                        .stroke(Stroke::new(1.0, theme::with_alpha(tone, 0.55)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            ui.set_max_width(380.0);
                            ui.label(egui::RichText::new(&t.text).color(if t.error {
                                Color32::from_rgb(0xFF, 0xD7, 0xD2)
                            } else {
                                theme::PARCHMENT
                            }));
                        });
                });
            y -= 62.0;
        }
    }
}
