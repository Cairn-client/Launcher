// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Home: the landing page, like a game launcher's: notices, a slideshow of the games Cairn covers, where you left off, your numbers, and
//! shelves of posters. Opening a game goes to its page in the Library.

use super::theme;
use super::widgets::{card, fade, paint_cover, poster, primary_button, secondary_button, PosterInfo};
use crate::app::{CairnApp, View};
use crate::games::GameProfile;
use crate::home_data;
use cairn_setup::catalog::KNOWN_ISSUE;
use cairn_setup::time_util;
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};
use std::time::Duration;

/// What the page shows, worked out once per frame.
type Tile = Box<dyn FnOnce(&mut CairnApp, &mut egui::Ui)>;

struct HomeSnapshot {
    slides: Vec<GameProfile>,
    favorites: Vec<GameProfile>,
    playable: Vec<GameProfile>,
    recent_games: Vec<GameProfile>,
    upcoming: Vec<GameProfile>,
    suggestions: Vec<GameProfile>,
    on_pc: std::collections::HashSet<String>,
    games_on_pc: usize,
    mods_installed: usize,
}

impl CairnApp {
    fn home_snapshot(&mut self) -> HomeSnapshot {
        let all: Vec<GameProfile> = self.registry.all().into_iter().cloned().collect();
        let mut on_pc = std::collections::HashSet::new();
        let (mut mods_installed, mut suggestions) = (0, Vec::new());
        for g in &all {
            if self.is_game_found(g) {
                on_pc.insert(g.id.clone());
                if g.is_supported && !self.is_mod_downloaded(g) && self.offers.of(&g.id).is_some() {
                    suggestions.push(g.clone());
                }
            }
            if self.is_mod_installed(g) {
                mods_installed += 1;
            }
        }
        let in_library: Vec<&GameProfile> = all.iter().filter(|g| self.prefs.in_library(&g.id)).collect();
        let favorites = in_library
            .iter()
            .filter(|g| self.prefs.is_favorite(&g.id))
            .map(|g| (*g).clone())
            .collect();
        // every game with a mod is on the shelf, in the library or not
        let playable = all.iter().filter(|g| g.is_supported).cloned().collect();
        let upcoming = all.iter().filter(|g| !g.is_supported).cloned().collect();
        let mut recents: Vec<&crate::home_data::RecentSession> = self.recent.entries.iter().collect();
        recents.sort_by_key(|r| std::cmp::Reverse(r.last_secs()));
        let recent_games = recents
            .iter()
            .filter_map(|r| all.iter().find(|g| g.id.eq_ignore_ascii_case(&r.game_id)))
            .take(8)
            .cloned()
            .collect();
        let slides = all
            .iter()
            .filter(|g| g.is_supported && (!g.background_image.is_empty() || !g.hero_image.is_empty()))
            .cloned()
            .collect();
        HomeSnapshot {
            slides,
            favorites,
            playable,
            recent_games,
            upcoming,
            suggestions,
            games_on_pc: on_pc.len(),
            on_pc,
            mods_installed,
        }
    }

    pub fn home(&mut self, ui: &mut egui::Ui) {
        let snap = self.home_snapshot();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::GROUND))
            .show(ui, |ui| {
                // the page scrolls with the wheel and the bar; a drag is for the shelves
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_source(egui::scroll_area::ScrollSource {
                        drag: egui::scroll_area::DragScroll::Never,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        self.notices_ui(ui);
                        self.slideshow(ui, &snap);
                        // the content is a centred column, so a wide window has no empty right half
                        egui::Frame::new().inner_margin(Margin::symmetric(32, 20)).show(ui, |ui| {
                            self.dashboard(ui, &snap);
                            ui.add_space(8.0);
                            self.shelf(ui, "Favourites", &snap.favorites, &snap);
                            self.shelf(ui, "Playable now", &snap.playable, &snap);
                            self.shelf(ui, "Recently played", &snap.recent_games, &snap);
                            self.tools_and_tip(ui);
                            self.shelf(ui, "Coming soon", &snap.upcoming, &snap);
                            ui.add_space(30.0);
                        });
                    });
            });
    }

    fn notices_ui(&mut self, ui: &mut egui::Ui) {
        let launcher = crate::app::VERSION;
        let now = time_util::now_utc();
        let game_found: std::collections::HashSet<String> = self
            .registry
            .all()
            .iter()
            .map(|g| g.id.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .filter(|id| self.registry.get(id).cloned().is_some_and(|g| self.is_game_found(&g)))
            .collect();
        let has_game = |id: &str| game_found.contains(&id.to_lowercase()) || game_found.iter().any(|g| g.eq_ignore_ascii_case(id));
        let notices: Vec<_> = self
            .registry
            .catalog()
            .notices
            .iter()
            .filter(|n| n.is_current(Some(launcher), now, Some(&has_game)))
            .filter(|n| !self.prefs.dismissed_notices.contains(&n.id))
            .cloned()
            .collect();
        for n in notices {
            let tone = if n.kind == KNOWN_ISSUE { theme::WARN } else { self.accent };
            egui::Frame::new()
                .fill(theme::with_alpha(tone, 0.12))
                .stroke(Stroke::new(1.0, theme::with_alpha(tone, 0.35)))
                .inner_margin(Margin::symmetric(32, 12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&n.title).strong().color(tone));
                        if !n.text.is_empty() {
                            ui.add(egui::Label::new(RichText::new(&n.text).color(theme::PARCHMENT_DIM)).wrap());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.link("Hide").on_hover_text("Hide this message (it stays under Help)").clicked() {
                                self.prefs.dismissed_notices.push(n.id.clone());
                                self.save_prefs();
                            }
                        });
                    });
                });
        }
    }

    fn slideshow(&mut self, ui: &mut egui::Ui, snap: &HomeSnapshot) {
        if snap.slides.is_empty() {
            return;
        }
        self.slide %= snap.slides.len();
        if self.settings.slideshow_auto && self.slide_since.elapsed() > Duration::from_secs(8) {
            self.slide = (self.slide + 1) % snap.slides.len();
            self.slide_since = std::time::Instant::now();
        }
        let game = snap.slides[self.slide].clone();
        let accent = self.accent;
        // tall enough to be the page's hero: about half the window, never a thin strip
        let size = Vec2::new(ui.available_width(), (ui.ctx().content_rect().height() * 0.5).clamp(380.0, 640.0));
        let (rect, _) = ui.allocate_exact_size(size, Sense::click());
        ui.painter().rect_filled(rect, 0.0, theme::INK);
        let art = if game.hero_image.is_empty() {
            &game.background_image
        } else {
            &game.hero_image
        };
        if let Some(tex) = self.images.get(&self.ctx, art) {
            paint_cover(ui, &tex, rect, 0, Color32::WHITE);
        }
        // the shade on the left, under the text
        let shade = egui::Rect::from_min_size(rect.min, Vec2::new((rect.width() * 0.62).round(), rect.height()));
        fade(ui, shade, true, theme::GROUND, |t| (1.0 - t).powf(1.4) * 0.92);
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
            rect.min + Vec2::new(84.0, 56.0),
            Vec2::new(520.0, rect.height() - 90.0),
        )));
        match self.images.get(&self.ctx, &game.logo_image) {
            Some(tex) => {
                let s = tex.size_vec2();
                let scale = (340.0 / s.x).min(130.0 / s.y);
                body.add(egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), s * scale)));
            }
            None => {
                body.label(RichText::new(&game.display_name).size(40.0).strong());
            }
        }
        body.add_space(8.0);
        body.add(egui::Label::new(RichText::new(&game.description).size(15.0).color(theme::PARCHMENT_DIM)).wrap());
        body.add_space(14.0);
        if primary_button(&mut body, "View game", accent, true).clicked() {
            self.open_game(&game.id);
        }
        // the arrows and the dots
        for (dir, x) in [(-1i32, rect.left() + 38.0), (1, rect.right() - 38.0)] {
            let c = egui::pos2(x, rect.center().y);
            let arrow = egui::Rect::from_center_size(c, Vec2::splat(40.0));
            let r = ui.interact(arrow, ui.make_persistent_id(("arrow", dir)), Sense::click());
            ui.painter()
                .circle_filled(c, 20.0, theme::with_alpha(Color32::BLACK, if r.hovered() { 0.7 } else { 0.4 }));
            if dir < 0 {
                super::icons::chevron_left(ui.painter(), c + Vec2::new(-1.0, 0.0), 9.0, theme::PARCHMENT);
            } else {
                super::icons::chevron(ui.painter(), c + Vec2::new(1.0, 0.0), 9.0, false, theme::PARCHMENT);
            }
            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                let len = snap.slides.len() as i32;
                self.slide = ((self.slide as i32 + dir).rem_euclid(len)) as usize;
                self.slide_since = std::time::Instant::now();
            }
        }
        // one segment a slide at the bottom right; the one shown is longer and fills as its time runs out, always in Cairn's colour
        let (short, long, gap, height) = (16.0_f32, 52.0_f32, 6.0_f32, 5.0_f32);
        let n = snap.slides.len();
        let total = (n - 1) as f32 * (short + gap) + long;
        let progress = if self.settings.slideshow_auto {
            (self.slide_since.elapsed().as_secs_f32() / 8.0).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let mut x = rect.right() - 56.0 - total;
        for i in 0..n {
            let on = i == self.slide;
            let w = if on { long } else { short };
            let bar = egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - 20.0 - height / 2.0), Vec2::new(w, height));
            let r = ui.interact(
                bar.expand2(Vec2::new(gap / 2.0, 7.0)),
                ui.make_persistent_id(("dot", i)),
                Sense::click(),
            );
            ui.painter().rect_filled(
                bar,
                CornerRadius::same(3),
                theme::with_alpha(Color32::WHITE, if on { 0.3 } else { 0.4 }),
            );
            if on {
                let fill = egui::Rect::from_min_size(bar.min, Vec2::new(w * progress, height));
                ui.painter().rect_filled(fill, CornerRadius::same(3), self.accent);
            }
            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.slide = i;
                self.slide_since = std::time::Instant::now();
            }
            x += w + gap;
        }
        if self.settings.slideshow_auto {
            ui.ctx().request_repaint_after(Duration::from_millis(60));
        }
    }

    /// Opens a game's page in the Library.
    pub fn open_game(&mut self, id: &str) {
        self.select_game(id);
        self.view = View::Library;
    }

    fn dashboard(&mut self, ui: &mut egui::Ui, snap: &HomeSnapshot) {
        let now = time_util::now_utc();
        let width = ui.available_width();
        let cols = if width > 1000.0 { 3 } else { 2 };
        let tiles: Vec<Tile> = {
            let last = self.recent.last().cloned();
            let recent_game = last.as_ref().and_then(|l| self.registry.get(&l.game_id).cloned());
            let total_sessions: i32 = self.recent.entries.iter().map(|e| e.sessions).sum();
            let total_minutes: i32 = self.recent.entries.iter().map(|e| e.minutes).sum();
            let most = self
                .recent
                .entries
                .iter()
                .max_by_key(|e| e.sessions)
                .and_then(|e| self.registry.get(&e.game_id))
                .map(|g| g.display_name.clone())
                .unwrap_or_default();
            let suggestions = snap.suggestions.clone();
            let latest = home_data::newer_release(crate::app::VERSION, self.release_notes.as_deref())
                .cloned()
                .or_else(|| self.release_notes.as_ref().and_then(|n| n.iter().find(|r| !r.prerelease).cloned()));
            let (games_on_pc, mods_installed, favs) = (snap.games_on_pc, snap.mods_installed, snap.favorites.len());
            vec![
                Box::new(move |app: &mut CairnApp, ui: &mut egui::Ui| {
                    ui.label(RichText::new("Continue").size(17.0).strong());
                    match (recent_game, last) {
                        (Some(g), Some(l)) => {
                            let who = if l.role == "host" {
                                "You hosted".to_string()
                            } else if l.with.is_empty() {
                                "You joined".to_string()
                            } else {
                                format!("You joined {}", l.with)
                            };
                            ui.label(RichText::new(&g.display_name).strong().size(15.0));
                            ui.label(RichText::new(format!("{who} · {}", home_data::ago(l.last_secs(), now))).color(theme::PARCHMENT_DIM));
                            ui.label(
                                RichText::new(format!(
                                    "{}{}",
                                    if l.sessions == 1 {
                                        "1 session".to_string()
                                    } else {
                                        format!("{} sessions", l.sessions)
                                    },
                                    if l.minutes > 0 {
                                        format!(" · {} played", home_data::played(l.minutes))
                                    } else {
                                        " so far".into()
                                    }
                                ))
                                .color(theme::MUTED),
                            );
                            ui.add_space(6.0);
                            if secondary_button(ui, "Open", true).clicked() {
                                app.open_game(&g.id);
                            }
                        }
                        _ => {
                            ui.label(RichText::new("Nothing played yet. Pick a game from the slideshow or the shelves below, download its mod, and start a session.").color(theme::MUTED));
                            if ui.link("Browse the Library").clicked() {
                                app.view = View::Library;
                            }
                        }
                    }
                }),
                Box::new(move |app: &mut CairnApp, ui: &mut egui::Ui| {
                    ui.label(RichText::new("Your numbers").size(17.0).strong());
                    if total_sessions == 0 {
                        ui.label(
                            RichText::new("Your sessions and play time show up here once you host or join a game.").color(theme::MUTED),
                        );
                    } else {
                        stat_row(
                            ui,
                            &total_sessions.to_string(),
                            if total_sessions == 1 { "session" } else { "sessions" },
                        );
                        stat_row(
                            ui,
                            &if total_minutes > 0 {
                                home_data::played(total_minutes)
                            } else {
                                "-".into()
                            },
                            "played",
                        );
                        stat_row(ui, &most, "most played");
                        if ui.link("Open your account").clicked() {
                            app.view = View::Profile;
                        }
                    }
                }),
                Box::new(move |app: &mut CairnApp, ui: &mut egui::Ui| {
                    ui.label(RichText::new("Your library").size(17.0).strong());
                    stat_row(
                        ui,
                        &games_on_pc.to_string(),
                        if games_on_pc == 1 { "game on this PC" } else { "games on this PC" },
                    );
                    stat_row(
                        ui,
                        &mods_installed.to_string(),
                        if mods_installed == 1 { "mod installed" } else { "mods installed" },
                    );
                    stat_row(ui, &favs.to_string(), if favs == 1 { "favourite" } else { "favourites" });
                    if ui.link("Open the Library").clicked() {
                        app.view = View::Library;
                    }
                }),
                Box::new(move |app: &mut CairnApp, ui: &mut egui::Ui| {
                    if suggestions.is_empty() {
                        ui.label(RichText::new("Ready to set up").size(17.0).strong());
                        ui.label(RichText::new("Nothing to set up: every game you have has its mod, or has none yet.").color(theme::MUTED));
                    } else {
                        ui.label(RichText::new("Ready to set up").size(17.0).strong());
                        ui.label(RichText::new("You have these games, and Cairn has a mod for each.").color(theme::MUTED));
                        for g in suggestions {
                            ui.horizontal(|ui| {
                                ui.label(&g.display_name);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.link("Get the mod").clicked() {
                                        app.download_pack(&g.id);
                                    }
                                });
                            });
                        }
                    }
                }),
                Box::new(move |app: &mut CairnApp, ui: &mut egui::Ui| match latest {
                    Some(news) => {
                        ui.label(RichText::new(format!("New in {}", news.tag)).size(17.0).strong());
                        if news.items.is_empty() {
                            ui.label(RichText::new("Maintenance and stability improvements.").color(theme::MUTED));
                        }
                        for line in news.items.iter().take(3) {
                            ui.add(egui::Label::new(RichText::new(format!("•  {line}")).color(theme::PARCHMENT_DIM)).wrap());
                        }
                        if ui.link("All the news").clicked() {
                            app.view = View::Versions;
                        }
                    }
                    None => {
                        ui.label(RichText::new("New in Cairn").size(17.0).strong());
                        ui.label(RichText::new("The news could not be read.").color(theme::MUTED));
                    }
                }),
            ]
        };
        let mut tiles = tiles.into_iter();
        loop {
            let row: Vec<_> = tiles.by_ref().take(cols).collect();
            if row.is_empty() {
                break;
            }
            // a short last row stretches to the full width instead of leaving a hole
            let tile_w = (width - 16.0 * (row.len() as f32 - 1.0)) / row.len() as f32;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for tile in row {
                    ui.allocate_ui(Vec2::new(tile_w, 0.0), |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(tile_w);
                            card(ui, |ui| {
                                ui.set_width(tile_w - 38.0);
                                ui.set_min_height(150.0);
                                tile(self, ui);
                            });
                        });
                    });
                }
            });
            ui.add_space(16.0);
        }
    }

    fn shelf(&mut self, ui: &mut egui::Ui, title: &str, games: &[GameProfile], snap: &HomeSnapshot) {
        if games.is_empty() {
            return;
        }
        ui.add_space(14.0);
        ui.label(RichText::new(title).size(19.0).strong());
        ui.add_space(6.0);
        // the bar has a row of its own under the posters, not floating over them
        ui.scope(|ui| {
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            ui.spacing_mut().scroll.bar_inner_margin = 10.0;
            // the shelf can be grabbed and pulled, like on a touch screen
            let drag_id = ui.make_persistent_id(("shelf-drag", title));
            let mut st: ShelfDrag = ui.data(|d| d.get_temp(drag_id)).unwrap_or_default();
            let (down, origin, delta, released) = ui.input(|i| {
                (
                    i.pointer.primary_down(),
                    i.pointer.press_origin(),
                    i.pointer.delta().x,
                    i.pointer.any_released(),
                )
            });
            if down && origin.is_some_and(|p| st.rect.contains(p)) {
                let moved = ui
                    .input(|i| i.pointer.latest_pos())
                    .zip(origin)
                    .is_some_and(|(a, b)| (a.x - b.x).abs() > 6.0);
                if moved || st.dragging {
                    st.dragging = true;
                    st.offset -= delta;
                    // remembered, so that the shelf keeps gliding when it is let go
                    st.glide = 0.6 * st.glide - 0.4 * delta;
                }
            }
            // a pull that ends on a poster must not also open it
            let pulled = st.dragging;
            if !down {
                st.dragging = false;
            }
            if pulled {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            let _ = released;
            let mut area = egui::ScrollArea::horizontal()
                .id_salt(("shelf", title))
                .auto_shrink([false, true])
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: egui::scroll_area::DragScroll::Never,
                    ..Default::default()
                });
            let gliding = !down && st.glide.abs() > 0.4;
            if gliding {
                st.offset += st.glide;
                st.glide *= 0.94;
                ui.ctx().request_repaint();
            } else if !pulled {
                st.glide = 0.0;
            }
            if (pulled && down) || gliding {
                area = area.horizontal_scroll_offset(st.offset.max(0.0));
            }
            let out = area.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    for g in games {
                        let url = if g.poster_image.is_empty() {
                            &g.background_image
                        } else {
                            &g.poster_image
                        };
                        let art = self.images.get(&self.ctx, url);
                        let accent = self.accent;
                        let sub = if !g.developer.is_empty() {
                            g.developer.clone()
                        } else if g.is_supported {
                            "Playable now".into()
                        } else {
                            "Coming soon".into()
                        };
                        let info = PosterInfo {
                            name: &g.display_name,
                            sub: &sub,
                            flag: snap.on_pc.contains(&g.id).then_some("On this PC"),
                            favorite: self.prefs.is_favorite(&g.id),
                            heart: self.prefs.in_library(&g.id),
                            accent,
                        };
                        let (open, heart) = poster(ui, art.as_ref(), &info);
                        if open && !pulled {
                            // a game in the library opens there, any other in the Store
                            if self.prefs.in_library(&g.id) {
                                self.open_game(&g.id);
                            } else {
                                self.store_chosen = Some(g.id.clone());
                                self.view = View::Store;
                            }
                        }
                        if heart && !pulled {
                            let now = self.prefs.toggle_favorite(&g.id);
                            self.save_prefs();
                            self.toast(
                                &if now {
                                    format!("{} added to your favourites.", g.display_name)
                                } else {
                                    format!("{} removed from your favourites.", g.display_name)
                                },
                                false,
                            );
                        }
                    }
                });
            });
            st.offset = out.state.clamped_offset().x;
            st.rect = out.inner_rect;
            ui.data_mut(|d| d.insert_temp(drag_id, st));
        });
    }

    fn tools_and_tip(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.label(RichText::new("Tools").size(19.0).strong());
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(14.0, 14.0);
            for (title, text, action) in [
                ("Check setup", "Find what is missing for your games, and fix it.", 0),
                ("Save the logs", "One zip with the game's, the mod's and the launcher's logs.", 1),
                ("Report a problem", "Open the issue page, with what to attach.", 2),
                ("Welcome guide", "The short walk through Cairn, again.", 3),
            ] {
                if tool_card(ui, title, text).clicked() {
                    match action {
                        2 => crate::open::open_url("https://github.com/Cairn-client/Launcher/issues/new"),
                        3 => {
                            self.prefs.tour_seen = false;
                            self.save_prefs();
                        }
                        1 => self.save_logs(),
                        _ => {
                            let id = self.selected_game.clone();
                            self.open_setup(&id);
                        }
                    }
                }
            }
        });
    }
}

/// Where a shelf is, how far it is scrolled, and whether it is being pulled with the pointer.
#[derive(Clone, Copy)]
struct ShelfDrag {
    offset: f32,
    dragging: bool,
    rect: egui::Rect,
    /// How fast the shelf is still gliding after it was let go (points a frame).
    glide: f32,
}

impl Default for ShelfDrag {
    fn default() -> Self {
        Self {
            offset: 0.0,
            dragging: false,
            rect: egui::Rect::NOTHING,
            glide: 0.0,
        }
    }
}

fn stat_row(ui: &mut egui::Ui, value: &str, label: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(value).size(18.0).strong());
        ui.label(RichText::new(label).color(theme::MUTED));
    });
}

fn tool_card(ui: &mut egui::Ui, title: &str, text: &str) -> egui::Response {
    let size = Vec2::new(280.0, 82.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(12),
        if resp.hovered() {
            theme::OAK_LIT
        } else {
            theme::with_alpha(theme::INK, 0.85)
        },
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.09)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 16.0, rect.top() + 22.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(15.0),
        theme::PARCHMENT,
    );
    let galley = ui
        .painter()
        .layout(text.to_string(), egui::FontId::proportional(12.5), theme::MUTED, size.x - 32.0);
    ui.painter()
        .galley(egui::pos2(rect.left() + 16.0, rect.top() + 40.0), galley, theme::MUTED);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
