// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The Store: every game Cairn has a mod for (or will have), to add to the library. Nothing is downloaded here: adding a game only puts it in
//! the Library, where its mod is downloaded when the player wants it.

use super::theme;
use super::widgets::{card, chip, paint_cover, poster, primary_button, secondary_button, PosterInfo};
use crate::app::{CairnApp, View};
use crate::games::GameProfile;
use cairn_setup::catalog::FeatureState;
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};

impl CairnApp {
    pub fn store(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::GROUND))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(id) = self.store_chosen.clone() {
                        if let Some(game) = self.registry.get(&id).cloned() {
                            self.store_detail(ui, &game);
                            return;
                        }
                        self.store_chosen = None;
                    }
                    egui::Frame::new().inner_margin(Margin::symmetric(40, 28)).show(ui, |ui| {
                        ui.set_max_width(1280.0);
                        ui.label(RichText::new("Store").size(32.0).strong());
                        ui.label(
                            RichText::new("Add the games you play to your library. Nothing is downloaded until you ask for the mod.")
                                .color(theme::PARCHMENT_DIM),
                        );
                        ui.add_space(12.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.store_query)
                                .hint_text("Search games")
                                .desired_width(320.0)
                                .margin(Margin::symmetric(12, 8))
                                .background_color(theme::TRACK),
                        );
                        ui.add_space(14.0);
                        let q = self.store_query.trim().to_lowercase();
                        let mut games: Vec<GameProfile> = self
                            .registry
                            .all()
                            .into_iter()
                            .filter(|g| {
                                q.is_empty()
                                    || g.display_name.to_lowercase().contains(&q)
                                    || g.developer.to_lowercase().contains(&q)
                                    || g.franchise.to_lowercase().contains(&q)
                            })
                            .cloned()
                            .collect();
                        games.sort_by_key(|g| (!g.is_supported, g.display_name.to_lowercase()));
                        if games.is_empty() {
                            ui.label(RichText::new("No game fits. Try another word or filter.").color(theme::MUTED));
                            return;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(16.0, 16.0);
                            for g in &games {
                                self.store_card(ui, g);
                            }
                        });
                    });
                });
            });
    }

    fn store_card(&mut self, ui: &mut egui::Ui, g: &GameProfile) {
        let in_lib = self.prefs.in_library(&g.id);
        let accent = self.accent_for(Some(g));
        ui.allocate_ui(Vec2::new(172.0, 340.0), |ui| {
            ui.vertical(|ui| {
                let url = if g.poster_image.is_empty() {
                    &g.background_image
                } else {
                    &g.poster_image
                };
                let art = self.images.get(&self.ctx, url);
                let on_pc = self.is_game_found(g);
                let sub = if g.is_supported {
                    if g.developer.is_empty() {
                        "Playable now".to_string()
                    } else {
                        g.developer.clone()
                    }
                } else {
                    "Coming soon".into()
                };
                let info = PosterInfo {
                    name: &g.display_name,
                    sub: &sub,
                    flag: on_pc.then_some("On this PC"),
                    favorite: in_lib,
                    accent,
                };
                let (open, heart) = poster(ui, art.as_ref(), &info);
                if open {
                    self.store_chosen = Some(g.id.clone());
                }
                if heart {
                    self.toggle_library(g);
                }
                ui.add_space(4.0);
                let label = if in_lib { "In your library" } else { "Add to library" };
                let size = Vec2::new(172.0, 34.0);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                let tone = if in_lib { theme::ONLINE } else { accent };
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(8),
                    theme::with_alpha(tone, if resp.hovered() { 0.22 } else { 0.12 }),
                );
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(8),
                    Stroke::new(1.0, theme::with_alpha(tone, 0.5)),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(13.0),
                    tone,
                );
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.toggle_library(g);
                }
            });
        });
    }

    fn toggle_library(&mut self, g: &GameProfile) {
        if self.prefs.in_library(&g.id) {
            let all: Vec<String> = self.registry.all().iter().map(|x| x.id.clone()).collect();
            self.prefs.remove_from_library(&g.id, &all);
            self.toast(&format!("{} removed from your library.", g.display_name), false);
        } else {
            self.prefs.add_to_library(&g.id);
            self.toast(&format!("{} added to your library.", g.display_name), false);
        }
        self.save_prefs();
    }

    fn store_detail(&mut self, ui: &mut egui::Ui, g: &GameProfile) {
        let accent = self.accent_for(Some(g));
        let owned = self.prefs.in_library(&g.id);
        let width = ui.available_width();
        // the picture at the top, with the logo and the actions over it
        let hero = egui::Rect::from_min_size(ui.cursor().min, Vec2::new(width, 340.0));
        ui.allocate_rect(hero, Sense::hover());
        ui.painter().rect_filled(hero, 0.0, theme::INK);
        let art = if g.background_image.is_empty() {
            &g.hero_image
        } else {
            &g.background_image
        };
        if let Some(tex) = self.images.get(&self.ctx, art) {
            paint_cover(ui, &tex, hero, 0, Color32::WHITE);
        }
        let ppp = ui.ctx().pixels_per_point();
        let n = 30;
        for i in 0..n {
            let t = i as f32 / n as f32;
            let y0 = ((hero.top() + hero.height() * t) * ppp).round() / ppp;
            let y1 = ((hero.top() + hero.height() * (t + 1.0 / n as f32)) * ppp).round() / ppp;
            ui.painter().rect_filled(
                egui::Rect::from_min_max(egui::pos2(hero.left(), y0), egui::pos2(hero.right(), y1)),
                0.0,
                theme::with_alpha(theme::GROUND, (t * t).min(1.0) * 0.97),
            );
        }
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
            hero.min + Vec2::new(40.0, 24.0),
            Vec2::new(width - 80.0, hero.height() - 40.0),
        )));
        if super::widgets::secondary_button(&mut body, "‹  Back to the store", true).clicked() {
            self.store_chosen = None;
        }
        body.add_space(30.0);
        match self.images.get(&self.ctx, &g.logo_image) {
            Some(tex) => {
                let s = tex.size_vec2();
                let scale = (300.0 / s.x).min(120.0 / s.y);
                body.add(egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), s * scale)));
            }
            None => {
                body.label(RichText::new(&g.display_name).size(36.0).strong());
            }
        }
        body.add_space(14.0);
        body.horizontal(|ui| {
            if owned {
                if primary_button(ui, "OPEN IN LIBRARY", accent, true).clicked() {
                    self.open_game(&g.id);
                }
                if secondary_button(ui, "REMOVE", true).clicked() {
                    self.toggle_library(g);
                }
            } else if primary_button(ui, "ADD TO LIBRARY", accent, true).clicked() {
                self.toggle_library(g);
            }
        });

        egui::Frame::new().inner_margin(Margin::symmetric(40, 10)).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let side_w = 260.0;
                let main_w = (ui.available_width() - side_w - 24.0).max(300.0);
                ui.allocate_ui(Vec2::new(main_w, 0.0), |ui| {
                    ui.set_width(main_w);
                    ui.add(
                        egui::Label::new(
                            RichText::new(if g.description.is_empty() {
                                "Multiplayer for this game is on its way."
                            } else {
                                &g.description
                            })
                            .size(15.0)
                            .color(theme::PARCHMENT_DIM),
                        )
                        .wrap(),
                    );
                    if !g.is_supported {
                        ui.label(RichText::new("You can add it now: it stays in your library until its mod is out.").color(theme::MUTED));
                    }
                    if !g.features.is_empty() {
                        ui.add_space(14.0);
                        self.store_features(ui, g, accent);
                    }
                });
                ui.add_space(24.0);
                ui.allocate_ui(Vec2::new(side_w, 0.0), |ui| {
                    ui.set_width(side_w);
                    card(ui, |ui| {
                        let on_pc = self.is_game_found(g);
                        fact(ui, "Developer", if g.developer.is_empty() { "-" } else { &g.developer }, None);
                        fact(ui, "Franchise", if g.franchise.is_empty() { "-" } else { &g.franchise }, None);
                        fact(
                            ui,
                            "Multiplayer",
                            if g.is_supported { "Playable now" } else { "Coming soon" },
                            g.is_supported.then_some(theme::ONLINE),
                        );
                        fact(
                            ui,
                            "On this PC",
                            if on_pc { "Found" } else { "Not found" },
                            on_pc.then_some(theme::ONLINE),
                        );
                        fact(
                            ui,
                            "In your library",
                            if owned { "Yes" } else { "No" },
                            owned.then_some(theme::ONLINE),
                        );
                    });
                });
            });
        });
        let _ = View::Store;
    }

    fn store_features(&mut self, ui: &mut egui::Ui, g: &GameProfile, accent: Color32) {
        let tab = if g.features.iter().any(|f| f.title == self.store_feature_tab) {
            self.store_feature_tab.clone()
        } else {
            g.features[0].title.clone()
        };
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("What the mod does").size(17.0).strong());
                for (state, label, tone) in [
                    (FeatureState::Ready, "ready", theme::ONLINE),
                    (FeatureState::InTest, "in test", theme::WARN),
                    (FeatureState::Planned, "planned", theme::MUTED),
                ] {
                    let n: usize = g.features.iter().flat_map(|f| &f.items).filter(|i| i.state == state).count();
                    if n > 0 {
                        chip(ui, &format!("{n} {label}"), tone);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                for group in &g.features {
                    let on = group.title == tab;
                    let text = format!("{}  {}", group.title, group.items.len());
                    if ui
                        .selectable_label(on, RichText::new(text).color(if on { accent } else { theme::PARCHMENT_DIM }))
                        .clicked()
                    {
                        self.store_feature_tab = group.title.clone();
                    }
                }
            });
            ui.separator();
            for group in g.features.iter().filter(|f| f.title == tab) {
                for f in &group.items {
                    ui.horizontal_top(|ui| {
                        let (label, tone) = match f.state {
                            FeatureState::Ready => ("Ready", theme::ONLINE),
                            FeatureState::InTest => ("In test", theme::WARN),
                            FeatureState::Planned => ("Planned", theme::MUTED),
                        };
                        chip(ui, label, tone);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&f.name).strong());
                            if !f.detail.is_empty() {
                                ui.add(egui::Label::new(RichText::new(&f.detail).color(theme::PARCHMENT_DIM)).wrap());
                            }
                        });
                    });
                }
            }
        });
    }
}

fn fact(ui: &mut egui::Ui, label: &str, value: &str, tone: Option<Color32>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).strong().color(tone.unwrap_or(theme::PARCHMENT)));
        });
    });
}
