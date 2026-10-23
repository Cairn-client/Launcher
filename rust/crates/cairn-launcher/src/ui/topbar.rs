// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The top bar: Home, Library and Store on the left; the search, Versions, Help, Settings, the account and the updates on the right.

use super::icons;
use super::theme;
use super::widgets::{nav_button, paint_cover};
use crate::app::{CairnApp, View};
use crate::games::GameProfile;
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Stroke, Vec2};

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
                        self.friends_button(ui);
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
                        let field = ui.add(search);
                        if field.changed() || field.gained_focus() {
                            self.search_open = !self.search.trim().is_empty();
                        }
                        self.search_results(ui.ctx(), &field);
                    });
                });
            });
    }

    /// The games that fit what was typed in the search, the closest first.
    fn search_matches(&self) -> Vec<GameProfile> {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let in_order = |hay: &str| {
            let mut chars = q.chars();
            let mut next = chars.next();
            for c in hay.chars() {
                if Some(c) == next {
                    next = chars.next();
                }
            }
            next.is_none()
        };
        let mut found: Vec<(u8, GameProfile)> = Vec::new();
        for g in self.registry.all() {
            let name = g.display_name.to_lowercase();
            let score = if name.starts_with(&q) {
                0
            } else if name.split_whitespace().any(|w| w.starts_with(&q)) {
                1
            } else if name.contains(&q) {
                2
            } else if g.developer.to_lowercase().contains(&q) || g.franchise.to_lowercase().contains(&q) {
                3
            } else if in_order(&name) {
                4
            } else {
                continue;
            };
            found.push((score, g.clone()));
        }
        found.sort_by_key(|(s, g)| (*s, g.display_name.to_lowercase()));
        found.into_iter().take(6).map(|(_, g)| g).collect()
    }

    #[allow(deprecated)]
    /// The drop-down under the search. A game in the library opens its page there; any other opens its page in the Store.
    fn search_results(&mut self, ctx: &egui::Context, field: &egui::Response) {
        if !self.search_open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.search_open = false;
            return;
        }
        let matches = self.search_matches();
        let mut chosen = None;
        if field.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            chosen = matches.first().map(|g| g.id.clone());
        }
        let width = 340.0;
        let area = egui::Area::new(egui::Id::new("search-results"))
            .order(egui::Order::Foreground)
            .fixed_pos(field.rect.right_bottom() + egui::vec2(-width, 6.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::INK)
                    .stroke(Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.1)))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(width - 12.0);
                        if matches.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("No game fits.").color(theme::MUTED));
                            ui.add_space(6.0);
                        }
                        for g in &matches {
                            let in_library = self.prefs.in_library(&g.id);
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), egui::Sense::click());
                            if resp.hovered() {
                                ui.painter()
                                    .rect_filled(rect, CornerRadius::same(8), theme::with_alpha(Color32::WHITE, 0.07));
                            }
                            let cover = egui::Rect::from_min_size(rect.min + Vec2::new(6.0, 5.0), Vec2::new(32.0, 46.0));
                            let url = if g.poster_image.is_empty() {
                                &g.logo_image
                            } else {
                                &g.poster_image
                            };
                            if let Some(tex) = self.images.get(&self.ctx, url) {
                                paint_cover(ui, &tex, cover, 4, Color32::WHITE);
                            }
                            let x = rect.left() + 50.0;
                            ui.painter().text(
                                egui::pos2(x, rect.top() + 21.0),
                                egui::Align2::LEFT_CENTER,
                                &g.display_name,
                                egui::FontId::proportional(14.0),
                                theme::PARCHMENT,
                            );
                            ui.painter().text(
                                egui::pos2(x, rect.top() + 39.0),
                                egui::Align2::LEFT_CENTER,
                                if in_library { "In your library" } else { "Open in the Store" },
                                egui::FontId::proportional(12.0),
                                if in_library { theme::ONLINE } else { theme::MUTED },
                            );
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                chosen = Some(g.id.clone());
                            }
                        }
                    });
            });
        let outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.contains(p) && !field.rect.contains(p))
        });
        if outside {
            self.search_open = false;
        }
        if let Some(id) = chosen {
            self.search_open = false;
            self.search.clear();
            if self.prefs.in_library(&id) {
                self.open_game(&id);
            } else {
                self.store_chosen = Some(id);
                self.view = View::Store;
            }
        }
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
            } else if resp.hovered() {
                theme::with_alpha(Color32::WHITE, 0.16)
            } else {
                theme::OAK_LIT
            },
        );
        ui.painter().circle_stroke(
            rect.center(),
            17.0,
            Stroke::new(
                1.0,
                if active || resp.hovered() {
                    self.accent
                } else {
                    theme::with_alpha(Color32::WHITE, 0.15)
                },
            ),
        );
        let avatar = self
            .profile_info
            .as_ref()
            .map(|i| i.avatar.clone())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| crate::steam_service::avatar_url(self.steam.avatar(), "medium"));
        match self.images.get(&self.ctx, &avatar) {
            Some(tex) => {
                let pic = egui::Rect::from_center_size(rect.center(), Vec2::splat(30.0));
                paint_cover(ui, &tex, pic, 15, Color32::WHITE);
            }
            None => {
                let name = self.steam.name();
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if name.is_empty() {
                        "?".to_string()
                    } else {
                        crate::steam_service::initial(name)
                    },
                    egui::FontId::proportional(15.0),
                    theme::PARCHMENT_DIM,
                );
            }
        }
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
        // the icon and the word, together in the middle of the button
        let galley = ui.painter().layout_no_wrap(label.to_string(), font, theme::PARCHMENT);
        let group = 16.0 + 8.0 + galley.size().x;
        let left = rect.center().x - group / 2.0;
        icons::download(ui.painter(), egui::pos2(left + 8.0, rect.center().y), tone);
        ui.painter().galley(
            egui::pos2(left + 24.0, rect.center().y - galley.size().y / 2.0),
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
