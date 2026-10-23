// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The Library: the game list on the left and the selected game's page on the right.

use super::icons;
use super::theme;
use super::widgets::{card, chip, fade, paint_contain, paint_cover, primary_button, secondary_button};
use crate::app::CairnApp;
use crate::game_list::{self, ListContext};
use crate::games::GameProfile;
use crate::prefs::{filter, Grouping};
use cairn_setup::catalog::FeatureState;
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};

impl CairnApp {
    pub fn library(&mut self, ui: &mut egui::Ui) {
        // when the game that was open is not in the library (any more), open the first one that is
        if !self.prefs.in_library(&self.selected_game) {
            if let Some(id) = self
                .registry
                .all()
                .iter()
                .find(|g| self.prefs.in_library(&g.id))
                .map(|g| g.id.clone())
            {
                self.selected_game = id;
            }
        }
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
                });
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if tool_icon(ui, true, "Open all folders").clicked() {
                            self.set_all_folders(true);
                        }
                        if tool_icon(ui, false, "Close all folders").clicked() {
                            self.set_all_folders(false);
                        }
                        if tool_button(ui, "Recent", self.prefs.recent_first, "Recently played first").clicked() {
                            self.prefs.recent_first = true;
                            self.save_prefs();
                        }
                        if tool_button(
                            ui,
                            if self.prefs.descending { "Z-A" } else { "A-Z" },
                            !self.prefs.recent_first,
                            "Alphabetical order (click again to reverse it)",
                        )
                        .clicked()
                        {
                            // Recent and alphabetical are two orders, one at a time
                            if self.prefs.recent_first {
                                self.prefs.recent_first = false;
                            } else {
                                self.prefs.descending = !self.prefs.descending;
                            }
                            self.save_prefs();
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
                    let width = ui.available_width();
                    self.game_item(ui, &id, width);
                    ui.add_space(4.0);
                }
            }
            ui.add_space(4.0);
        }
    }

    fn game_item(&mut self, ui: &mut egui::Ui, id: &str, width: f32) {
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
        let accent = self.accent;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 84.0), Sense::click());
        if selected || resp.hovered() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(12),
                theme::with_alpha(if selected { accent } else { Color32::WHITE }, if selected { 0.14 } else { 0.07 }),
            );
        } else {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(12), theme::with_alpha(Color32::WHITE, 0.03));
        }
        if selected {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(12),
                Stroke::new(1.0, theme::with_alpha(accent, 0.5)),
                egui::StrokeKind::Inside,
            );
        }
        let logo_rect = egui::Rect::from_min_size(rect.min + Vec2::new(10.0, 8.0), Vec2::new(46.0, 68.0));
        let cover = if game.poster_image.is_empty() {
            &game.logo_image
        } else {
            &game.poster_image
        };
        match self.images.get(&self.ctx, cover) {
            Some(tex) if !game.poster_image.is_empty() => paint_cover(ui, &tex, logo_rect, 6, Color32::WHITE),
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
        let x = rect.left() + 66.0;
        // the name may take two lines
        let mut job = egui::text::LayoutJob::simple(
            game.display_name.clone(),
            egui::FontId::proportional(14.0),
            theme::PARCHMENT,
            rect.right() - x - 26.0,
        );
        job.wrap.max_rows = 2;
        job.wrap.break_anywhere = false;
        let name = ui.painter().layout_job(job);
        let name_h = name.size().y;
        ui.painter().galley(egui::pos2(x, rect.top() + 12.0), name, theme::PARCHMENT);
        ui.painter().text(
            egui::pos2(x, rect.top() + 14.0 + name_h + 8.0),
            egui::Align2::LEFT_CENTER,
            state,
            egui::FontId::proportional(12.0),
            tone,
        );
        if self.prefs.is_favorite(&game.id) {
            icons::heart(ui.painter(), egui::pos2(rect.right() - 16.0, rect.top() + 16.0), 11.0, true, accent);
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
        let accent = self.accent;
        let full = ui.available_rect_before_wrap();
        egui::ScrollArea::vertical()
            .id_salt(("page", &game.id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // the picture is the top of the page: it scrolls with the content and fades into the dark below it
                let top = ui.cursor().min;
                let backdrop = egui::Rect::from_min_size(top, Vec2::new(full.width(), (full.height() * 0.75).max(560.0)));
                if let Some(tex) = self.images.get(&self.ctx, &game.background_image) {
                    paint_cover(ui, &tex, backdrop, 0, Color32::from_white_alpha(200));
                } else {
                    ui.painter().rect_filled(backdrop, 0.0, theme::mix(theme::INK, accent, 0.08));
                }
                // a fade into the dark, and a shade on the left under the title, the text and the buttons
                fade(ui, backdrop, false, theme::GROUND, |t| (0.3 + t * t * 0.8).min(1.0) * 0.99);
                let shade = egui::Rect::from_min_size(backdrop.min, Vec2::new((backdrop.width() * 0.7).min(900.0), backdrop.height()));
                fade(ui, shade, true, theme::GROUND, |t| (1.0 - t).powf(1.5) * 0.7);
                egui::Frame::new().inner_margin(Margin::symmetric(40, 28)).show(ui, |ui| {
                    ui.set_max_width(full.width() - 80.0);
                    // Remove and Favourite sit at the right edge of the page, not in the centred column
                    let corner = egui::Rect::from_min_max(
                        egui::pos2(full.left() + 24.0, top.y + 16.0),
                        egui::pos2(full.right() - 24.0, top.y + 56.0),
                    );
                    let mut corner_ui = ui.new_child(egui::UiBuilder::new().max_rect(corner));
                    self.page_corner(&mut corner_ui, game, accent);
                    ui.add_space(0.0);
                    // the picture is left free: the logo and everything else sit at the bottom of it
                    ui.add_space((full.height() * 0.42).clamp(200.0, 520.0));
                    self.hero(ui, game);
                    ui.add_space(10.0);
                    self.game_meta(ui, game, accent);
                    ui.add_space(14.0);
                    self.actions(ui, game, accent);
                    self.page_notices(ui, game);
                    ui.add_space(18.0);
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

    fn hero(&mut self, ui: &mut egui::Ui, game: &GameProfile) {
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
    }

    /// The state chips and the description, under the picture.
    fn game_meta(&mut self, ui: &mut egui::Ui, game: &GameProfile, accent: Color32) {
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
        ui.scope(|ui| {
            // the state on the left, what can be done about it on the right
            let mut cancel = false;
            let mut retry = false;
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("MOD").size(11.0).color(theme::MUTED).strong());
                    if let Some(item) = &active {
                        let pct = (item.progress * 100.0) as i32;
                        ui.label(
                            RichText::new(if item.state == crate::downloads::DownloadState::Installing {
                                "Installing…".to_string()
                            } else if item.is_waiting() {
                                "Waiting in the queue".to_string()
                            } else {
                                format!("Downloading {pct}%")
                            })
                            .size(16.0)
                            .strong(),
                        );
                    } else if let Some(pack) = &installed {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&pack.version).size(16.0).strong().color(theme::ONLINE));
                            if !mod_installed {
                                chip(ui, "not installed", theme::WARN);
                            }
                        });
                    } else if let Some(o) = &offered {
                        ui.label(RichText::new("Not downloaded").size(16.0).strong());
                        ui.label(
                            RichText::new(format!("{} available · {}", o.version, super::drawer::size_text(o.size))).color(theme::MUTED),
                        );
                    } else if !self.offers.tried {
                        ui.label(RichText::new("Checking for the mod…").size(16.0).color(theme::MUTED));
                    } else {
                        ui.label(
                            RichText::new(if self.offers.offline {
                                "The mod catalog is not available right now."
                            } else {
                                "No mod for this game yet."
                            })
                            .size(16.0)
                            .color(theme::MUTED),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(item) = &active {
                        if secondary_button(ui, "Cancel", true).clicked() {
                            cancel = true;
                        }
                        let _ = item;
                    } else if let Some(pack) = &installed {
                        let newer = offered
                            .as_ref()
                            .filter(|o| cairn_setup::packs::index::update_for(Some(pack), Some(o)).is_some());
                        if let Some(n) = newer {
                            if primary_button(ui, &format!("Update to {}", n.version), accent, true).clicked() {
                                self.download_pack(&game.id);
                            }
                        }
                        if external && !mod_installed && primary_button(ui, "Install", accent, true).clicked() {
                            self.install_mod(&game.id);
                        }
                        if secondary_button(ui, "Remove", true)
                            .on_hover_text("Remove the downloaded mod")
                            .clicked()
                            && !running
                        {
                            self.remove_pack(&game.id);
                        }
                        if let Some(prev) = self.store.read_previous(&game.id) {
                            if secondary_button(ui, &format!("Go back to {}", prev.version), true)
                                .on_hover_text("Put the version before the last update back")
                                .clicked()
                            {
                                self.rollback_pack(&game.id);
                            }
                        }
                    } else if let Some(o) = &offered {
                        if primary_button(ui, "Download mod", accent, true).clicked() {
                            self.download_pack(&game.id);
                        }
                        let _ = o;
                    } else if self.offers.tried && self.offers.offline && secondary_button(ui, "Retry", true).clicked() {
                        retry = true;
                    }
                });
            });
            if let Some(item) = &active {
                ui.add_space(8.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
                ui.painter().rect_filled(rect, CornerRadius::same(3), theme::TRACK);
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(rect.min, Vec2::new(rect.width() * item.progress as f32, rect.height())),
                    CornerRadius::same(3),
                    accent,
                );
            }
            if cancel {
                if let Some(item) = &active {
                    self.downloads.cancel(&item.id);
                }
            }
            if retry {
                self.load_offers();
            }
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
            // the mod first: its state and the button to get it
            self.mod_row(ui, game, accent);
            ui.add_space(14.0);
            let line = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(line, 1.0), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, theme::with_alpha(Color32::WHITE, 0.08));
            ui.add_space(14.0);
            ui.label(RichText::new("How it works").size(17.0).strong());
            ui.add_space(6.0);
            let host_title = if external && join.is_empty() { "To play" } else { "To host" };
            if join.is_empty() {
                steps(ui, host_title, &host, accent);
            } else {
                ui.columns(2, |cols| {
                    steps(&mut cols[0], host_title, &host, accent);
                    steps(&mut cols[1], "To join", &join, accent);
                });
            }
        });
    }

    fn features_card(&mut self, ui: &mut egui::Ui, game: &GameProfile, accent: Color32) {
        if game.features.is_empty() {
            return;
        }
        // the category chosen, kept for the page
        let tab_id = ui.make_persistent_id(("feature-tab", &game.id));
        let mut tab: usize = ui.data(|d| d.get_temp(tab_id)).unwrap_or(0);
        if tab >= game.features.len() {
            tab = 0;
        }
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("What the mod does").size(17.0).strong());
                for (state, label, tone) in [
                    (FeatureState::Ready, "ready", theme::ONLINE),
                    (FeatureState::InTest, "in test", theme::WARN),
                    (FeatureState::Planned, "planned", theme::MUTED),
                ] {
                    let n: usize = game.features.iter().flat_map(|f| &f.items).filter(|i| i.state == state).count();
                    if n > 0 {
                        chip(ui, &format!("{n} {label}"), tone);
                    }
                }
            });
            ui.add_space(8.0);
            // the categories are buttons
            ui.horizontal_wrapped(|ui| {
                for (i, group) in game.features.iter().enumerate() {
                    if filter_chip(ui, &format!("{}  {}", group.title, group.items.len()), i == tab, accent).clicked() {
                        tab = i;
                    }
                }
            });
            ui.add_space(10.0);
            // the chosen category's features, in two columns across the whole card
            let items = &game.features[tab].items;
            ui.columns(2, |cols| {
                for (i, f) in items.iter().enumerate() {
                    let ui = &mut cols[i % 2];
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
                    ui.add_space(8.0);
                }
            });
        });
        ui.data_mut(|d| d.insert_temp(tab_id, tab));
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

/// A small icon button for the list: two chevrons down to open every folder, two up to close them.
fn tool_icon(ui: &mut egui::Ui, expand: bool, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 24.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::with_alpha(Color32::WHITE, 0.07));
    }
    let color = if resp.hovered() { theme::PARCHMENT } else { theme::MUTED };
    let c = rect.center();
    for dy in [-3.5_f32, 3.5] {
        let (a, b) = if expand { (-3.0_f32, 3.0_f32) } else { (3.0, -3.0) };
        ui.painter().add(egui::Shape::line(
            vec![
                egui::pos2(c.x - 4.5, c.y + dy + a / 2.0),
                egui::pos2(c.x, c.y + dy + b / 2.0),
                egui::pos2(c.x + 4.5, c.y + dy + a / 2.0),
            ],
            Stroke::new(1.6, color),
        ));
    }
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
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
