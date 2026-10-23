// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The updates drawer: the download in progress with its speed, what waits, the updates on offer and what finished.

use crate::app::CairnApp;
use crate::downloads::DownloadState;
use crate::games::GameProfile;
use crate::ui::theme;
use crate::ui::widgets::{card, paint_cover, primary_button, secondary_button};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};
use std::time::{Duration, Instant};

pub fn size_text(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b >= 1_073_741_824.0 {
        format!("{:.1} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else {
        format!("{:.0} KB", b / 1024.0)
    }
}

const WIDTH: f32 = 520.0;
/// The top bar's height: the drawer opens right under it.
const TOP: f32 = 56.0;
/// How many seconds of download speed the graph shows.
const HISTORY: usize = 90;

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(18.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.5).strong().color(theme::MUTED));
    ui.add_space(8.0);
}

fn speed_text(bytes_per_sec: f64) -> String {
    format!("{}/s", size_text(bytes_per_sec as i64))
}

/// The download speed over the last minute and a half, like the graph on Steam's downloads page: a grid, the peak at the top, the line and
/// the area under it.
fn speed_graph(ui: &mut egui::Ui, history: &[f32], accent: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 120.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(10), theme::TRACK);
    let plot = rect.shrink2(Vec2::new(10.0, 14.0));
    let peak = history.iter().cloned().fold(1_048_576.0_f32, f32::max);
    for i in 0..=3 {
        let y = plot.top() + plot.height() * i as f32 / 3.0;
        ui.painter().line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.06)),
        );
    }
    ui.painter().text(
        rect.left_top() + Vec2::new(12.0, 6.0),
        egui::Align2::LEFT_TOP,
        speed_text(peak as f64),
        egui::FontId::proportional(11.0),
        theme::MUTED,
    );
    let n = history.len();
    if n < 2 {
        return;
    }
    let point = |i: usize, v: f32| {
        let x = plot.right() - (n - 1 - i) as f32 / (HISTORY - 1) as f32 * plot.width();
        egui::pos2(x.max(plot.left()), plot.bottom() - (v / peak).min(1.0) * plot.height())
    };
    let line: Vec<egui::Pos2> = history.iter().enumerate().map(|(i, v)| point(i, *v)).collect();
    let mut mesh = egui::Mesh::default();
    for (i, p) in line.iter().enumerate() {
        mesh.colored_vertex(*p, theme::with_alpha(accent, 0.35));
        mesh.colored_vertex(egui::pos2(p.x, plot.bottom()), theme::with_alpha(accent, 0.0));
        if i > 0 {
            let k = (i * 2) as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().add(egui::Shape::line(line, Stroke::new(2.0, accent)));
}

/// One figure of the stats row: a small caption over the value.
fn stat(ui: &mut egui::Ui, width: f32, caption: &str, value: &str) {
    ui.allocate_ui(Vec2::new(width, 46.0), |ui| {
        ui.set_min_width(width);
        ui.vertical(|ui| {
            ui.label(RichText::new(caption.to_uppercase()).size(11.0).color(theme::MUTED));
            ui.label(RichText::new(value).size(17.0).strong());
        });
    });
}

impl CairnApp {
    /// A game in a list of the tab: its cover, its name, a line under it, and the buttons on the right (their labels; the bool is "main").
    /// Returns the index of the button pressed.
    fn drawer_row(
        &mut self,
        ui: &mut egui::Ui,
        game: Option<&GameProfile>,
        name: &str,
        sub: &str,
        tone: Color32,
        buttons: &[(&str, bool)],
    ) -> Option<usize> {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 68.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(12), theme::OAK);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(12),
            Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.06)),
            egui::StrokeKind::Inside,
        );
        let cover = egui::Rect::from_min_size(rect.min + Vec2::new(10.0, 9.0), Vec2::new(36.0, 50.0));
        ui.painter().rect_filled(cover, CornerRadius::same(5), theme::INK);
        if let Some(g) = game {
            let url = if g.poster_image.is_empty() {
                &g.logo_image
            } else {
                &g.poster_image
            };
            if let Some(tex) = self.images.get(&self.ctx, url) {
                paint_cover(ui, &tex, cover, 5, Color32::WHITE);
            }
        }
        ui.painter().text(
            egui::pos2(rect.left() + 58.0, rect.top() + 24.0),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(14.5),
            theme::PARCHMENT,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 58.0, rect.top() + 44.0),
            egui::Align2::LEFT_CENTER,
            sub,
            egui::FontId::proportional(12.0),
            tone,
        );
        let mut pressed = None;
        let mut buttons_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    rect.min + Vec2::new(220.0, 0.0),
                    rect.max - Vec2::new(10.0, 0.0),
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        for (i, (label, main)) in buttons.iter().enumerate() {
            let clicked = if *main {
                primary_button(&mut buttons_ui, label, self.accent, true).clicked()
            } else {
                secondary_button(&mut buttons_ui, label, true).clicked()
            };
            if clicked {
                pressed = Some(i);
            }
        }
        ui.add_space(6.0);
        pressed
    }

    /// The updates tab, like the downloads page of Steam or the Epic launcher: what downloads now with the speed graph and the figures, the
    /// games whose mod has an update, the queue, the mods the games' repositories offer, and what is installed.
    #[allow(deprecated)]
    pub fn updates_drawer(&mut self, ctx: &egui::Context) {
        if !self.updates_open {
            self.speed_history.clear();
            return;
        }
        let items = self.downloads.snapshot();
        let active = items.iter().find(|i| i.is_active()).cloned();
        let waiting: Vec<_> = items.iter().filter(|i| i.is_waiting()).cloned().collect();
        let finished: Vec<_> = items.iter().filter(|i| i.is_finished()).rev().cloned().collect();
        let session_bytes: i64 = items.iter().map(|i| i.bytes_done()).sum();
        let queued: Vec<String> = items
            .iter()
            .filter(|i| !i.is_finished())
            .map(|i| i.game_id.to_lowercase())
            .collect();
        let pending: Vec<_> = self
            .mod_updates()
            .into_iter()
            .filter(|(g, _)| !queued.contains(&g.id.to_lowercase()))
            .collect();
        let games: Vec<GameProfile> = self.registry.all().into_iter().cloned().collect();
        let mut installed: Vec<(GameProfile, String)> = Vec::new();
        let mut available: Vec<(GameProfile, cairn_setup::packs::PackInfo)> = Vec::new();
        for g in &games {
            if let Some(p) = self.installed_pack_of(&g.id) {
                if !pending.iter().any(|(u, _)| u.id == g.id) {
                    installed.push((g.clone(), p.version));
                }
            } else if g.is_supported && !queued.contains(&g.id.to_lowercase()) {
                if let Some(info) = self.offers.of(&g.id).cloned() {
                    available.push((g.clone(), info));
                }
            }
        }
        let speed = self.downloads.speed();
        let left = self.downloads.remaining();
        let accent = self.accent;
        if self.speed_at.elapsed() >= Duration::from_secs(1) {
            self.speed_at = Instant::now();
            self.speed_history.push(speed as f32);
            if self.speed_history.len() > HISTORY {
                self.speed_history.remove(0);
            }
        }
        if active.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        let refreshing = self.catalog_loading || self.offers_loading;
        let history = self.speed_history.clone();
        let peak = history.iter().cloned().fold(0.0_f32, f32::max) as f64;
        let (mut close, mut check, mut clear, mut get_all, mut reload_games) = (false, false, false, false, false);
        let game_count = games.len();
        let (mut cancel, mut first, mut get) = (None, None, None);
        egui::Area::new(egui::Id::new("updates-scrim"))
            .order(egui::Order::Middle)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                let screen = ctx.content_rect();
                let r = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter().rect_filled(screen, 0.0, theme::with_alpha(Color32::BLACK, 0.45));
                if r.clicked() {
                    close = true;
                }
            });
        let screen = ctx.content_rect();
        let panel = egui::Rect::from_min_max(egui::pos2(screen.right() - WIDTH, screen.top() + TOP), screen.max);
        egui::Area::new(egui::Id::new("updates-drawer"))
            .order(egui::Order::Foreground)
            .fixed_pos(panel.min)
            .show(ctx, |ui| {
                ui.allocate_rect(panel, egui::Sense::click());
                ui.painter().rect_filled(panel, 0.0, theme::INK);
                ui.painter().line_segment(
                    [panel.left_top(), panel.left_bottom()],
                    Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.1)),
                );
                // the games list is pinned to the bottom of the tab, under the scrolling content
                let inner = panel.shrink2(Vec2::new(22.0, 18.0));
                let footer_h = 76.0;
                let mut footer = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(
                    egui::pos2(inner.left(), inner.bottom() - footer_h),
                    inner.max,
                )));
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(
                    inner.min,
                    egui::pos2(inner.right(), inner.bottom() - footer_h - 12.0),
                )));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Downloads").size(22.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if secondary_button(ui, "Close", true).clicked() {
                            close = true;
                        }
                    });
                });
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut ui, |ui| {
                    ui.spacing_mut().scroll.bar_inner_margin = 8.0;
                    ui.set_width(WIDTH - 44.0 - 14.0);
                    ui.add_space(10.0);
                    // what downloads now
                    card(ui, |ui| {
                        match &active {
                            Some(a) => {
                                let game = self.registry.get(&a.game_id).cloned();
                                ui.horizontal(|ui| {
                                    let (cover, _) = ui.allocate_exact_size(Vec2::new(46.0, 64.0), egui::Sense::hover());
                                    ui.painter().rect_filled(cover, CornerRadius::same(6), theme::OAK);
                                    if let Some(g) = &game {
                                        let url = if g.poster_image.is_empty() {
                                            &g.logo_image
                                        } else {
                                            &g.poster_image
                                        };
                                        if let Some(tex) = self.images.get(&self.ctx, url) {
                                            paint_cover(ui, &tex, cover, 6, Color32::WHITE);
                                        }
                                    }
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(&a.game_name).size(17.0).strong());
                                        ui.label(
                                            RichText::new(match &a.from_version {
                                                Some(f) => format!("{f}  →  {}", a.to_version()),
                                                None => a.to_version().to_string(),
                                            })
                                            .color(theme::PARCHMENT_DIM),
                                        );
                                        let state = if a.state == DownloadState::Installing {
                                            "Putting the mod in the game...".to_string()
                                        } else {
                                            format!("{} of {}", size_text(a.bytes_done()), size_text(a.size()))
                                        };
                                        ui.label(RichText::new(state).color(theme::MUTED));
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if a.state == DownloadState::Downloading && secondary_button(ui, "Cancel", true).clicked() {
                                            cancel = Some(a.id.clone());
                                        }
                                    });
                                });
                                ui.add_space(8.0);
                                ui.add(egui::ProgressBar::new(a.progress.clamp(0.0, 1.0) as f32).fill(accent));
                            }
                            None => {
                                ui.label(RichText::new("Nothing is downloading").size(17.0).strong());
                                ui.label(
                                    RichText::new(if pending.is_empty() {
                                        "All your mods are up to date."
                                    } else {
                                        "Some of your mods have an update."
                                    })
                                    .color(theme::MUTED),
                                );
                            }
                        }
                        ui.add_space(12.0);
                        speed_graph(ui, &history, accent);
                        ui.add_space(12.0);
                        let w = (ui.available_width() - 3.0 * ui.spacing().item_spacing.x) / 4.0;
                        ui.horizontal(|ui| {
                            stat(ui, w, "Current", &speed_text(speed));
                            stat(ui, w, "Peak", &speed_text(peak));
                            stat(ui, w, "Downloaded", &size_text(session_bytes));
                            stat(
                                ui,
                                w,
                                "Time left",
                                &left
                                    .map(|t| format!("{} min", (t.as_secs() / 60).max(1)))
                                    .unwrap_or_else(|| "-".into()),
                            );
                        });
                    });
                    if !waiting.is_empty() {
                        heading(ui, &format!("Up next ({})", waiting.len()));
                        for w in &waiting {
                            let game = self.registry.get(&w.game_id).cloned();
                            let sub = format!("{} · {}", w.to_version(), size_text(w.size()));
                            match self.drawer_row(
                                ui,
                                game.as_ref(),
                                &w.game_name,
                                &sub,
                                theme::MUTED,
                                &[("Cancel", false), ("First", false)],
                            ) {
                                Some(0) => cancel = Some(w.id.clone()),
                                Some(1) => first = Some(w.id.clone()),
                                _ => {}
                            }
                        }
                    }
                    // the installed mods that have an update
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("UPDATES ({})", pending.len()))
                                .size(11.5)
                                .strong()
                                .color(theme::MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if pending.len() > 1 && primary_button(ui, "Update all", accent, true).clicked() {
                                get_all = true;
                            }
                        });
                    });
                    ui.add_space(8.0);
                    if pending.is_empty() {
                        ui.label(RichText::new("The mods you have installed are up to date.").color(theme::MUTED));
                    }
                    for (g, info) in &pending {
                        let now = self.installed_pack_of(&g.id).map(|p| p.version).unwrap_or_default();
                        let sub = format!("{now}  →  {} · {}", info.version, size_text(info.size));
                        if self
                            .drawer_row(ui, Some(g), &g.display_name, &sub, theme::MUTED, &[("Update", true)])
                            .is_some()
                        {
                            get = Some(g.id.clone());
                        }
                    }
                    // the mods the games' repositories offer
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("MODS TO DOWNLOAD ({})", available.len()))
                                .size(11.5)
                                .strong()
                                .color(theme::MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if secondary_button(ui, if refreshing { "Checking..." } else { "Check for mods" }, !refreshing).clicked() {
                                check = true;
                            }
                        });
                    });
                    ui.add_space(8.0);
                    for (g, info) in &available {
                        let sub = format!("{} · {}", info.version, size_text(info.size));
                        if self
                            .drawer_row(ui, Some(g), &g.display_name, &sub, theme::MUTED, &[("Download", false)])
                            .is_some()
                        {
                            get = Some(g.id.clone());
                        }
                    }
                    // the download history of this session
                    if !finished.is_empty() {
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("HISTORY ({})", finished.len()))
                                    .size(11.5)
                                    .strong()
                                    .color(theme::MUTED),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.link("Clear").clicked() {
                                    clear = true;
                                }
                            });
                        });
                        ui.add_space(8.0);
                        for f in &finished {
                            let game = self.registry.get(&f.game_id).cloned();
                            let (text, tone) = match f.state {
                                DownloadState::Done => (format!("{} · downloaded", f.to_version()), theme::ONLINE),
                                DownloadState::Failed => (format!("{} · failed", f.to_version()), theme::DANGER_BRIGHT),
                                _ => (format!("{} · cancelled", f.to_version()), theme::MUTED),
                            };
                            self.drawer_row(ui, game.as_ref(), &f.game_name, &text, tone, &[]);
                        }
                    }
                    ui.add_space(24.0);
                });
                footer.painter().line_segment(
                    [footer.max_rect().left_top(), footer.max_rect().right_top()],
                    Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.08)),
                );
                footer.add_space(12.0);
                footer.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("{game_count} games")).size(15.0).strong());
                        ui.label(RichText::new("From the games repository").color(theme::MUTED));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if self.catalog_loading { "Updating..." } else { "Update games list" };
                        if primary_button(ui, label, accent, !self.catalog_loading).clicked() {
                            reload_games = true;
                        }
                    });
                });
            });
        if let Some(id) = cancel {
            self.downloads.cancel(&id);
        }
        if let Some(id) = first {
            self.downloads.move_first(&id);
        }
        if let Some(id) = get {
            self.download_pack(&id);
        }
        if get_all {
            for (g, _) in &pending {
                self.download_pack(&g.id);
            }
        }
        if check {
            self.load_offers();
        }
        if reload_games {
            self.load_catalog();
        }
        if clear {
            self.downloads.clear_finished();
        }
        if close {
            self.updates_open = false;
        }
    }
}
