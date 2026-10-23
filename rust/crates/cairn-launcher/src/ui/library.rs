// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The Library: the game list on the left and the selected game's page on the right.

use super::icons;
use super::theme;
use super::widgets::{card, chip, paint_contain, paint_cover, primary_button, secondary_button};
use crate::app::CairnApp;
use crate::game_list::{self, ListContext};
use crate::games::GameProfile;
use crate::prefs::{filter, Grouping};
use cairn_setup::catalog::FeatureState;
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};

impl CairnApp {
    pub fn library(&mut self, ui: &mut egui::Ui) {
        self.library_sidebar(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::GROUND))
            .show(ui, |ui| {
                let Some(game) = self.selected_profile().cloned().filter(|g| self.prefs.in_library(&g.id)) else {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("Pick a game on the left.").color(theme::MUTED).size(16.0));
                    });
                    return;
                };
                self.game_page(ui, &game);
            });
    }

    // ---- the list -------------------------------------------------------------------------------------------------------

    fn library_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("library_list")
            .exact_size(330.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::with_alpha(theme::INK, 0.9))
                    .inner_margin(Margin::same(14))
                    .stroke(Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.06))),
            )
            .show(ui, |ui| {
                // what to show
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                    for (label, flag) in [
                        ("On this PC", filter::ON_THIS_PC),
                        ("Mod installed", filter::MOD_INSTALLED),
                        ("Favourites", filter::FAVORITES),
                    ] {
                        let on = self.prefs.filter & flag != 0;
                        if filter_chip(ui, label, on, self.accent).clicked() {
                            self.prefs.filter ^= flag;
                            self.save_prefs();
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let before = self.prefs.grouping;
                    egui::ComboBox::from_id_salt("grouping")
                        .selected_text(self.prefs.grouping.key())
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for g in [Grouping::Title, Grouping::Developer, Grouping::Franchise] {
                                ui.selectable_value(&mut self.prefs.grouping, g, g.key());
                            }
                        });
                    if before != self.prefs.grouping {
                        self.save_prefs();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tool_button(ui, "A-Z", !self.prefs.descending, "Order: A to Z (click for Z to A)").clicked() {
                            self.prefs.descending = !self.prefs.descending;
                            self.save_prefs();
                        }
                        if tool_button(ui, "Recent", self.prefs.recent_first, "Recently played first").clicked() {
                            self.prefs.recent_first = !self.prefs.recent_first;
                            self.save_prefs();
                        }
                        if tool_button(ui, "Close", false, "Close all folders").clicked() {
                            self.set_all_folders(false);
                        }
                        if tool_button(ui, "Open", false, "Open all folders").clicked() {
                            self.set_all_folders(true);
                        }
                    });
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.game_list_ui(ui));
            });
    }

    fn set_all_folders(&mut self, open: bool) {
        let groups = self.build_groups();
        let grouping = self.prefs.grouping;
        for h in groups.iter().map(|g| game_list::folder_key(grouping, &g.0)) {
            self.prefs.collapsed.retain(|c| *c != h);
            if !open {
                self.prefs.collapsed.push(h);
            }
        }
        self.save_prefs();
    }

    /// The folders of the list as (header, game ids).
    fn build_groups(&mut self) -> Vec<(String, Vec<String>)> {
        let games: Vec<GameProfile> = self
            .registry
            .all()
            .into_iter()
            .filter(|g| self.prefs.in_library(&g.id))
            .cloned()
            .collect();
        // what the list needs to know about each game is asked once
        let mut on_pc = std::collections::HashSet::new();
        let mut modded = std::collections::HashSet::new();
        for g in &games {
            if self.is_game_found(g) {
                on_pc.insert(g.id.clone());
            }
            if self.is_mod_installed(g) {
                modded.insert(g.id.clone());
            }
        }
        let favorites = self.prefs.favorites.clone();
        let refs: Vec<&GameProfile> = games.iter().collect();
        let (on_pc_f, mod_f) = (|g: &GameProfile| on_pc.contains(&g.id), |g: &GameProfile| modded.contains(&g.id));
        let fav_f = |g: &GameProfile| favorites.iter().any(|f| f.eq_ignore_ascii_case(&g.id));
        let last = |_: &GameProfile| None;
        let ctx = ListContext {
            on_this_pc: &on_pc_f,
            mod_installed: &mod_f,
            is_favorite: &fav_f,
            last_played: &last,
        };
        game_list::build(
            &refs,
            &self.search,
            self.prefs.filter,
            self.prefs.grouping,
            self.prefs.descending,
            self.prefs.recent_first,
            &ctx,
        )
        .into_iter()
        .map(|g| (g.header, g.games.into_iter().map(|p| p.id.clone()).collect()))
        .collect()
    }

    fn game_list_ui(&mut self, ui: &mut egui::Ui) {
        let groups = self.build_groups();
        if groups.is_empty() {
            let msg = if !self.search.trim().is_empty() {
                "No game matches your search."
            } else if self.prefs.filter != filter::ALL {
                "No game matches the filters."
            } else {
                "Your library is empty. Add games from the Store."
            };
            ui.add_space(20.0);
            ui.label(RichText::new(msg).color(theme::MUTED));
            return;
        }
        let grouping = self.prefs.grouping;
        let searching = !self.search.trim().is_empty();
        for (header, ids) in groups {
            let key = game_list::folder_key(grouping, &header);
            let open = searching || !self.prefs.collapsed.contains(&key);
            let head = folder_head(ui, &header, ids.len(), open);
            if head.clicked() && !searching {
                if open {
                    self.prefs.collapsed.push(key.clone());
                } else {
                    self.prefs.collapsed.retain(|c| *c != key);
                }
                self.save_prefs();
            }
            if open {
                for id in ids {
                    self.game_item(ui, &id);
                }
            }
            ui.add_space(4.0);
        }
    }

    fn game_item(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(game) = self.registry.get(id).cloned() else { return };
        let found = self.is_game_found(&game);
        let needs_mod = self.needs_mod(&game);
        let selected = self.selected_game.eq_ignore_ascii_case(id);
        let (state, tone) = if !game.is_supported {
            ("Coming soon", theme::MUTED)
        } else if !found {
            ("Not found", theme::WARN)
        } else if needs_mod {
            ("Mod needed", theme::WARN)
        } else {
            ("Ready", theme::ONLINE)
        };
        let accent = self.accent_for(Some(&game));
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::click());
        if selected || resp.hovered() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(10),
                theme::with_alpha(if selected { accent } else { Color32::WHITE }, if selected { 0.14 } else { 0.05 }),
            );
        }
        if selected {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(10),
                Stroke::new(1.0, theme::with_alpha(accent, 0.5)),
                egui::StrokeKind::Inside,
            );
        }
        let logo_rect = egui::Rect::from_min_size(rect.min + Vec2::new(8.0, 6.0), Vec2::new(60.0, 40.0));
        match self.images.get(&self.ctx, &game.logo_image) {
            Some(tex) => paint_contain(ui, &tex, logo_rect, Color32::WHITE),
            None => {
                ui.painter().text(
                    logo_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    game.id.to_uppercase(),
                    egui::FontId::proportional(11.0),
                    theme::MUTED,
                );
            }
        }
        let x = rect.left() + 78.0;
        ui.painter().text(
            egui::pos2(x, rect.top() + 16.0),
            egui::Align2::LEFT_CENTER,
            &game.display_name,
            egui::FontId::proportional(14.0),
            theme::PARCHMENT,
        );
        ui.painter().text(
            egui::pos2(x, rect.top() + 35.0),
            egui::Align2::LEFT_CENTER,
            state,
            egui::FontId::proportional(12.0),
            tone,
        );
        if self.prefs.is_favorite(&game.id) {
            icons::heart(ui.painter(), egui::pos2(rect.right() - 16.0, rect.center().y), 12.0, true, accent);
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.select_game(&game.id);
        }
    }

    pub fn select_game(&mut self, id: &str) {
        if !self.selected_game.eq_ignore_ascii_case(id) {
            self.pack_message = (String::new(), false); // a note about one game's download is not about the next game
        }
        if self.registry.get(id).is_some() {
            self.selected_game = id.to_string();
        }
    }

    // ---- the game's page ------------------------------------------------------------------------------------------------

    fn game_page(&mut self, ui: &mut egui::Ui, game: &GameProfile) {
        let accent = self.accent_for(Some(game));
        let full = ui.available_rect_before_wrap();
        egui::ScrollArea::vertical()
            .id_salt(("page", &game.id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // the picture is the top of the page: it scrolls with the content and fades into the dark below it
                let top = ui.cursor().min;
                let backdrop = egui::Rect::from_min_size(top, Vec2::new(full.width(), 560.0));
                if let Some(tex) = self.images.get(&self.ctx, &game.background_image) {
                    paint_cover(ui, &tex, backdrop, 0, Color32::from_white_alpha(235));
                } else {
                    ui.painter().rect_filled(backdrop, 0.0, theme::mix(theme::INK, accent, 0.08));
                }
                // a fade into the dark: bands of a few pixels that meet exactly (no overlap, so no seams)
                let ppp = ui.ctx().pixels_per_point();
                let snap = |y: f32| (y * ppp).round() / ppp;
                let band_h = 4.0;
                let n = (backdrop.height() / band_h).ceil() as i32;
                for i in 0..n {
                    let y0 = snap(backdrop.top() + i as f32 * band_h);
                    let y1 = snap((backdrop.top() + (i + 1) as f32 * band_h).min(backdrop.bottom()));
                    let t = (i as f32 + 0.5) / n as f32;
                    let a = (t * t * 1.05).min(1.0) * 0.99;
                    let band = egui::Rect::from_min_max(egui::pos2(backdrop.left(), y0), egui::pos2(backdrop.right(), y1));
                    ui.painter().rect_filled(band, 0.0, theme::with_alpha(theme::GROUND, a));
                }
                egui::Frame::new().inner_margin(Margin::symmetric(40, 28)).show(ui, |ui| {
                    ui.set_max_width(980.0_f32.min(full.width() - 80.0));
                    self.page_corner(ui, game, accent);
                    ui.add_space(26.0);
                    self.hero(ui, game, accent);
                    ui.add_space(18.0);
                    self.actions(ui, game, accent);
                    ui.add_space(18.0);
                    self.mod_row(ui, game, accent);
                    self.page_notices(ui, game);
                    ui.add_space(20.0);
                    self.how_it_works(ui, game, accent);
                    ui.add_space(16.0);
                    self.features_card(ui, game, accent);
                    ui.add_space(16.0);
                    self.screenshots(ui, game);
                    ui.add_space(40.0);
                });
            });
    }

    fn page_corner(&mut self, ui: &mut egui::Ui, game: &GameProfile, accent: Color32) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            let fav = self.prefs.is_favorite(&game.id);
            if corner_button(
                ui,
                if fav { "Favourited" } else { "Favourite" },
                Some(Icon::Heart(fav)),
                fav,
                accent,
            )
            .clicked()
            {
                let now = self.prefs.toggle_favorite(&game.id);
                self.save_prefs();
                self.toast(
                    &if now {
                        format!("{} added to your favourites.", game.display_name)
                    } else {
                        format!("{} removed from your favourites.", game.display_name)
                    },
                    false,
                );
            }
            if corner_button(ui, "Remove", Some(Icon::Cross), false, accent)
                .on_hover_text("Take this game out of your library (its mod stays on this PC)")
                .clicked()
            {
                let all: Vec<String> = self.registry.all().iter().map(|g| g.id.clone()).collect();
                self.prefs.remove_from_library(&game.id, &all);
                self.save_prefs();
                self.selected_game = self
                    .registry
                    .all()
                    .iter()
                    .find(|g| self.prefs.in_library(&g.id))
                    .map(|g| g.id.clone())
                    .unwrap_or_default();
            }
        });
    }

    fn hero(&mut self, ui: &mut egui::Ui, game: &GameProfile, accent: Color32) {
        match self.images.get(&self.ctx, &game.logo_image) {
            Some(tex) => {
                let size = tex.size_vec2();
                let scale = (320.0 / size.x).min(150.0 / size.y);
                ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), size * scale)));
            }
            None => {
                ui.label(RichText::new(&game.display_name).size(40.0).strong());
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let (label, tone) = (
                if game.is_supported { "Ready check" } else { "Coming soon" },
                if game.is_supported { accent } else { theme::MUTED },
            );
            chip(ui, label, tone);
            if !game.developer.is_empty() {
                chip(ui, &game.developer, theme::PARCHMENT_DIM);
            }
        });
        if !game.description.is_empty() {
            ui.add_space(8.0);
            ui.add(egui::Label::new(RichText::new(&game.description).size(15.0).color(theme::PARCHMENT_DIM)).wrap());
        }
    }

    fn actions(&mut self, ui: &mut egui::Ui, game: &GameProfile, accent: Color32) {
        let needs_mod = self.needs_mod(game);
        let external = self.is_external_mod(game);
        ui.horizontal(|ui| {
            if external {
                if primary_button(ui, "Play", accent, game.is_supported && !needs_mod).clicked() {
                    self.open_ready_check(&game.id);
                }
            } else {
                if primary_button(ui, "Host a game", accent, game.is_supported && !needs_mod).clicked() {
                    self.open_host(&game.id);
                }
                if secondary_button(ui, "Join a game", game.is_supported && !needs_mod).clicked() {
                    self.open_join(&game.id);
                }
            }
        });
    }

    fn mod_row(&mut self, ui: &mut egui::Ui, game: &GameProfile, accent: Color32) {
        let installed = self.installed_pack_of(&game.id);
        let offered = self.offers.of(&game.id).cloned();
        let active = self.downloads.item_of(&game.id);
        let external = self.is_external_mod(game);
        let mod_installed = self.is_mod_installed(game);
        let running = false;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("MOD").size(11.0).color(theme::MUTED).strong());
                ui.add_space(8.0);
                if let Some(item) = active {
                    let pct = (item.progress * 100.0) as i32;
                    ui.label(if item.state == crate::downloads::DownloadState::Installing {
                        "Installing…".to_string()
                    } else if item.is_waiting() {
                        "Waiting in the queue".to_string()
                    } else {
                        format!("Downloading {pct}%")
                    });
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(180.0, 6.0), Sense::hover());
                    ui.painter().rect_filled(rect, CornerRadius::same(3), theme::TRACK);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * item.progress as f32, rect.height())),
                        CornerRadius::same(3),
                        accent,
                    );
                    if ui.link("Cancel").clicked() {
                        self.downloads.cancel(&item.id);
                    }
                } else if let Some(pack) = &installed {
                    ui.label(
                        RichText::new(format!("{}{}", pack.version, if mod_installed { "" } else { " (not installed)" }))
                            .color(theme::ONLINE),
                    );
                    let newer = offered
                        .as_ref()
                        .filter(|o| cairn_setup::packs::index::update_for(Some(pack), Some(o)).is_some());
                    if ui.link("Remove").on_hover_text("Remove the downloaded mod").clicked() && !running {
                        self.remove_pack(&game.id);
                    }
                    if let Some(prev) = self.store.read_previous(&game.id) {
                        if ui
                            .link(format!("Go back to {}", prev.version))
                            .on_hover_text("Put the version before the last update back")
                            .clicked()
                        {
                            self.rollback_pack(&game.id);
                        }
                    }
                    if external && !mod_installed && ui.link(RichText::new("Install").strong()).clicked() {
                        self.install_mod(&game.id);
                    }
                    if let Some(n) = newer {
                        if ui.link(format!("Update to {}", n.version)).clicked() {
                            self.download_pack(&game.id);
                        }
                    }
                } else if let Some(o) = &offered {
                    if primary_button(ui, &format!("Download mod ({} KB)", o.size / 1024), accent, true).clicked() {
                        self.download_pack(&game.id);
                    }
                } else if !self.offers.tried {
                    ui.label(RichText::new("Checking for the mod…").color(theme::MUTED));
                } else {
                    ui.label(
                        RichText::new(if self.offers.offline {
                            "The mod catalog is not available right now."
                        } else {
                            "No mod for this game yet."
                        })
                        .color(theme::MUTED),
                    );
                    if self.offers.offline && ui.link("Retry").clicked() {
                        self.load_offers();
                    }
                }
            });
            if let Some(reason) = installed
                .as_ref()
                .and_then(|p| self.registry.catalog().block_of(&p.id, &p.version))
                .map(|b| b.reason.clone())
            {
                ui.label(RichText::new(format!("This version has a known problem: {reason}")).color(theme::DANGER_BRIGHT));
            }
            if let Some(held) = self.offers.blocked.get(&game.id.to_lowercase()) {
                ui.label(RichText::new(format!("The newest version ({held}) is on hold and is not offered.")).color(theme::DANGER_BRIGHT));
            }
            if !self.pack_message.0.is_empty() {
                ui.label(RichText::new(&self.pack_message.0).color(if self.pack_message.1 {
                    theme::DANGER_BRIGHT
                } else {
                    theme::PARCHMENT_DIM
                }));
            }
            if !game.requirements.is_empty() {
                ui.separator();
                for req in &game.requirements {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("REQUIRES").size(11.0).color(theme::MUTED).strong());
                        ui.label(&req.name);
                        if ui.link("Steam library").clicked() {
                            crate::open::open_url(&format!("steam://nav/games/details/{}", req.app_id));
                        }
                    });
                }
            }
        });
    }

    fn page_notices(&mut self, ui: &mut egui::Ui, game: &GameProfile) {
        let found = self.is_game_found(game);
        if !found && game.is_supported {
            ui.add_space(12.0);
            let hint = if game.exe_is_setting {
                "set its path in Settings"
            } else {
                "use Locate game… or set its path in Settings"
            };
            egui::Frame::new().fill(theme::with_alpha(theme::WARN, 0.1)).stroke(Stroke::new(1.0, theme::with_alpha(theme::WARN, 0.4))).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(14)).show(ui, |ui| {
                ui.label(RichText::new(format!("The game was not found on this PC. Make sure it is installed (in your Steam library, or on this PC), then {hint}.")).color(theme::PARCHMENT));
            });
        }
    }

    fn how_it_works(&mut self, ui: &mut egui::Ui, game: &GameProfile, accent: Color32) {
        let external = self.is_external_mod(game);
        let host: Vec<String> = if game.host_steps.is_empty() {
            generic_host_steps(external)
        } else {
            game.host_steps.clone()
        };
        let join: Vec<String> = if game.join_steps.is_empty() && !external {
            generic_join_steps()
        } else {
            game.join_steps.clone()
        };
        card(ui, |ui| {
            ui.label(RichText::new("How it works").size(17.0).strong());
            ui.add_space(6.0);
            steps(ui, if external && join.is_empty() { "To play" } else { "To host" }, &host, accent);
            if !join.is_empty() {
                ui.add_space(8.0);
                steps(ui, "To join", &join, accent);
            }
        });
    }

    fn features_card(&mut self, ui: &mut egui::Ui, game: &GameProfile, accent: Color32) {
        if game.features.is_empty() {
            return;
        }
        card(ui, |ui| {
            ui.label(RichText::new("What the mod does").size(17.0).strong());
            for group in &game.features {
                ui.add_space(8.0);
                ui.label(RichText::new(&group.title).size(13.0).color(accent).strong());
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

    fn screenshots(&mut self, ui: &mut egui::Ui, game: &GameProfile) {
        if game.screenshots.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for url in &game.screenshots {
                let size = Vec2::new(300.0, 169.0);
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter().rect_filled(rect, CornerRadius::same(10), theme::INK);
                if let Some(tex) = self.images.get(&self.ctx, url) {
                    paint_cover(ui, &tex, rect, 10, Color32::WHITE);
                }
            }
        });
    }
}

fn steps(ui: &mut egui::Ui, title: &str, lines: &[String], accent: Color32) {
    ui.label(RichText::new(title).size(12.0).color(theme::MUTED).strong());
    for (i, line) in lines.iter().enumerate() {
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 11.0, theme::with_alpha(accent, 0.18));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                (i + 1).to_string(),
                egui::FontId::proportional(12.0),
                accent,
            );
            ui.add(egui::Label::new(line).wrap());
        });
    }
}

fn generic_host_steps(external: bool) -> Vec<String> {
    if external {
        return vec![
            "Download the game's mod above. Cairn installs it in the game's folder.".into(),
            "Press Play: Cairn starts the game, and the mod takes it from there.".into(),
        ];
    }
    vec![
        "Download the game's mod above, then press Host a game. Cairn starts the game for you.".into(),
        "Start or load the world you want to share.".into(),
        "Wait for the game to have loaded before the player joins.".into(),
    ]
}

fn generic_join_steps() -> Vec<String> {
    vec![
        "Download the game's mod above, then press Join a game.".into(),
        "Pick your friend in the list, or type the host's code. Cairn starts the game for you.".into(),
        "Join the host's world from the game once they are in it.".into(),
    ]
}

fn filter_chip(ui: &mut egui::Ui, text: &str, on: bool, accent: Color32) -> egui::Response {
    let font = egui::FontId::proportional(12.5);
    let color = if on { accent } else { theme::PARCHMENT_DIM };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(20.0, 12.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(255),
        if on {
            theme::with_alpha(accent, 0.15)
        } else if resp.hovered() {
            theme::with_alpha(Color32::WHITE, 0.06)
        } else {
            Color32::TRANSPARENT
        },
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(255),
        Stroke::new(
            1.0,
            if on {
                theme::with_alpha(accent, 0.6)
            } else {
                theme::with_alpha(Color32::WHITE, 0.14)
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn tool_button(ui: &mut egui::Ui, text: &str, active: bool, tip: &str) -> egui::Response {
    let font = egui::FontId::proportional(11.5);
    let color = if active { theme::PARCHMENT } else { theme::MUTED };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(12.0, 10.0), Sense::click());
    if active || resp.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(6),
            theme::with_alpha(Color32::WHITE, if active { 0.1 } else { 0.05 }),
        );
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn folder_head(ui: &mut egui::Ui, name: &str, count: usize, open: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), theme::with_alpha(Color32::WHITE, 0.04));
    }
    icons::chevron(
        ui.painter(),
        egui::pos2(rect.left() + 16.0, rect.center().y),
        9.0,
        open,
        theme::MUTED,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 28.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(13.5),
        theme::PARCHMENT_DIM,
    );
    ui.painter().text(
        egui::pos2(rect.right() - 12.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        count.to_string(),
        egui::FontId::proportional(12.0),
        theme::MUTED_DIM,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy)]
enum Icon {
    Heart(bool),
    Cross,
}

fn corner_button(ui: &mut egui::Ui, text: &str, icon: Option<Icon>, on: bool, accent: Color32) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let color = if on { accent } else { theme::PARCHMENT_DIM };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let icon_w = if icon.is_some() { 20.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(24.0 + icon_w, 14.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(255),
        theme::with_alpha(theme::INK, if resp.hovered() { 0.9 } else { 0.7 }),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(255),
        Stroke::new(
            1.0,
            theme::with_alpha(if on { accent } else { Color32::WHITE }, if on { 0.6 } else { 0.18 }),
        ),
        egui::StrokeKind::Inside,
    );
    let text_pos = egui::pos2(rect.left() + 12.0 + icon_w, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_pos, galley, color);
    let at = egui::pos2(rect.left() + 20.0, rect.center().y);
    match icon {
        Some(Icon::Heart(filled)) => icons::heart(ui.painter(), at, 12.0, filled, color),
        Some(Icon::Cross) => icons::cross(ui.painter(), at, 8.0, color),
        None => {}
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
