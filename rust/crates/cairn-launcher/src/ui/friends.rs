// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Your Steam friends in a menu beside your account: the button shows how many are online, the panel lists everyone with a search box.

use super::theme;
use crate::app::CairnApp;
use crate::steam_service::{avatar_url, initial, FriendRow};
use eframe::egui::{self, Align2, Color32, CornerRadius, RichText, Sense, Stroke, Vec2};

impl CairnApp {
    /// The friends as rows: those who are up first (in a game, then online), the rest by name.
    pub fn friend_rows(&self) -> Vec<FriendRow> {
        let Some(local) = &self.steam.local else { return vec![] };
        let mut rows: Vec<FriendRow> = local
            .named
            .iter()
            .map(|n| FriendRow::from_named(n, self.friend_status.get(&n.id).map(|(_, s)| s)))
            .collect();
        let rank = |f: &FriendRow| match f.state.as_str() {
            "in-game" => 0,
            "online" => 1,
            _ => 2,
        };
        rows.sort_by(|a, b| {
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        rows
    }

    pub fn online_friends(&self) -> usize {
        self.friend_rows().iter().filter(|f| f.is_online()).count()
    }

    /// The Friends button of the top bar and its panel.
    pub fn friends_button(&mut self, ui: &mut egui::Ui) {
        let online = self.online_friends();
        let label = "Friends";
        let galley = ui
            .painter()
            .layout_no_wrap(label.to_string(), egui::FontId::proportional(14.0), theme::PARCHMENT);
        let extra = if online > 0 { 76.0 } else { 0.0 };
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 44.0 + extra, 34.0), Sense::click());
        let active = self.friends_open;
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            if active {
                theme::with_alpha(self.accent, 0.14)
            } else if resp.hovered() {
                theme::with_alpha(Color32::WHITE, 0.06)
            } else {
                Color32::TRANSPARENT
            },
        );
        super::icons::person(
            ui.painter(),
            egui::pos2(rect.left() + 20.0, rect.center().y),
            if active { self.accent } else { theme::PARCHMENT_DIM },
        );
        ui.painter().galley(
            egui::pos2(rect.left() + 34.0, rect.center().y - galley.size().y / 2.0),
            galley,
            theme::PARCHMENT,
        );
        if online > 0 {
            let x = rect.right() - extra + 8.0;
            ui.painter().circle_filled(egui::pos2(x, rect.center().y), 4.0, theme::ONLINE);
            ui.painter().text(
                egui::pos2(x + 10.0, rect.center().y),
                Align2::LEFT_CENTER,
                format!("{online} online"),
                egui::FontId::proportional(12.5),
                theme::ONLINE,
            );
        }
        if resp
            .on_hover_text("Your Steam friends")
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.friends_open = !self.friends_open;
            if self.friends_open {
                self.load_friend_statuses();
            }
        }
    }

    /// The panel under the button (drawn as a window of its own).
    pub fn friends_panel(&mut self, ctx: &egui::Context) {
        if !self.friends_open {
            return;
        }
        self.refresh_home_friends(false);
        let rows = self.friend_rows();
        let q = self.friends_query.trim().to_lowercase();
        let shown: Vec<&FriendRow> = rows.iter().filter(|f| q.is_empty() || f.name.to_lowercase().contains(&q)).collect();
        let online = rows.iter().filter(|f| f.is_online()).count();
        let mut close = false;
        #[allow(deprecated)]
        let panel_id = egui::Id::new("friends_panel");
        let area = egui::Area::new(panel_id)
            .anchor(Align2::RIGHT_TOP, egui::vec2(-90.0, 60.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::OAK)
                    .stroke(Stroke::new(1.0, theme::OAK_LIT))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(340.0);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.friends_query)
                                    .hint_text("Search friends")
                                    .desired_width(170.0)
                                    .background_color(theme::TRACK),
                            );
                            ui.label(
                                RichText::new(format!("{online} of {} online", rows.len()))
                                    .color(theme::MUTED)
                                    .size(12.5),
                            );
                        });
                        ui.add_space(6.0);
                        if self.steam.local.is_none() {
                            ui.label(
                                RichText::new("No Steam friends were found. Make sure Steam is installed and you are signed in to it.")
                                    .color(theme::MUTED),
                            );
                        } else if rows.is_empty() {
                            ui.label(RichText::new("No Steam friends were found.").color(theme::MUTED));
                        } else if shown.is_empty() {
                            ui.label(RichText::new("No friend matches.").color(theme::MUTED));
                        } else {
                            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                                for f in shown {
                                    self.friend_row(ui, f);
                                }
                            });
                        }
                    });
            });
        // a click outside the panel closes it
        if ctx.input(|i| i.pointer.any_click())
            && !area.response.contains_pointer()
            && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.y > 56.0)
        {
            close = true;
        }
        if close {
            self.friends_open = false;
        }
    }

    /// One friend: the avatar ringed by what they are doing, the name over a line of status.
    pub fn friend_row(&mut self, ui: &mut egui::Ui, f: &FriendRow) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::hover());
        let ring = match f.state.as_str() {
            "in-game" => theme::ONLINE,
            "online" => theme::FOCUS,
            _ => theme::MUTED_DIM,
        };
        let c = egui::pos2(rect.left() + 22.0, rect.center().y);
        ui.painter().circle_filled(c, 17.0, theme::OAK_LIT);
        match self.images.get(&self.ctx, &avatar_url(&f.avatar, "medium")) {
            Some(t) => {
                let r = egui::Rect::from_center_size(c, Vec2::splat(30.0));
                super::widgets::paint_cover(
                    ui,
                    &t,
                    r,
                    15,
                    if f.is_online() {
                        Color32::WHITE
                    } else {
                        theme::with_alpha(Color32::WHITE, 0.55)
                    },
                );
            }
            None => {
                ui.painter().text(
                    c,
                    Align2::CENTER_CENTER,
                    initial(&f.name),
                    egui::FontId::proportional(14.0),
                    theme::PARCHMENT_DIM,
                );
            }
        }
        ui.painter().circle_stroke(c, 17.0, Stroke::new(2.0, ring));
        let dim = if f.is_online() { theme::PARCHMENT } else { theme::MUTED };
        ui.painter().text(
            egui::pos2(rect.left() + 50.0, rect.center().y - 8.0),
            Align2::LEFT_CENTER,
            &f.name,
            egui::FontId::proportional(14.0),
            dim,
        );
        if let Some(code) = self.hosting_as(&f.name).map(|h| h.code.clone()).filter(|_| self.running.is_none()) {
            let r = egui::Rect::from_center_size(egui::pos2(rect.right() - 36.0, rect.center().y), Vec2::new(60.0, 26.0));
            let resp = ui.interact(r, ui.make_persistent_id(("join", &f.id)), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::same(6), self.accent);
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                "JOIN",
                egui::FontId::proportional(11.5),
                Color32::BLACK,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.join_hosting_friend(&code);
            }
        } else if self.can_invite() && self.friends_seen.contains_key(&f.id) {
            let r = egui::Rect::from_center_size(egui::pos2(rect.right() - 36.0, rect.center().y), Vec2::new(60.0, 26.0));
            let resp = ui.interact(r, ui.make_persistent_id(("invite", &f.id)), Sense::click());
            ui.painter()
                .rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, self.accent), egui::StrokeKind::Inside);
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                "INVITE",
                egui::FontId::proportional(11.5),
                self.accent,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.invite_friend(&f.id);
            }
        }
        let line_color = match f.state.as_str() {
            "in-game" => theme::ONLINE,
            "online" => theme::FOCUS,
            _ => theme::MUTED,
        };
        ui.painter().text(
            egui::pos2(rect.left() + 50.0, rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            f.line(),
            egui::FontId::proportional(12.0),
            line_color,
        );
    }
}
