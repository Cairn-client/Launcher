// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The updates drawer: the download in progress with its speed, what waits, the updates on offer and what finished.

use crate::app::CairnApp;
use crate::downloads::DownloadState;
use crate::ui::theme;
use crate::ui::widgets::{card, primary_button, secondary_button};
use eframe::egui::{self, RichText, Vec2};

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

impl CairnApp {
    #[allow(deprecated)]
    pub fn updates_drawer(&mut self, ctx: &egui::Context) {
        if !self.updates_open {
            return;
        }
        let items = self.downloads.snapshot();
        let active = items.iter().find(|i| i.is_active()).cloned();
        let waiting: Vec<_> = items.iter().filter(|i| i.is_waiting()).cloned().collect();
        let finished: Vec<_> = items.iter().filter(|i| i.is_finished()).rev().cloned().collect();
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
        let speed = self.downloads.speed();
        let left = self.downloads.remaining();
        let accent = self.accent;
        let (mut close, mut check, mut clear) = (false, false, false);
        let mut cancel = None;
        let mut first = None;
        let mut get = None;
        egui::Area::new(egui::Id::new("updates-scrim"))
            .order(egui::Order::Middle)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                let screen = ctx.content_rect();
                let r = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter().rect_filled(screen, 0.0, theme::with_alpha(egui::Color32::BLACK, 0.45));
                if r.clicked() {
                    close = true;
                }
            });
        egui::Window::new("updates-drawer")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-8.0, 56.0))
            .fixed_size(Vec2::new(400.0, ctx.content_rect().height() - 72.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Manage updates").size(18.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if secondary_button(ui, "CLOSE", true).clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    card(ui, |ui| {
                        if let Some(a) = &active {
                            ui.label(
                                RichText::new(if a.state == DownloadState::Installing {
                                    "Installing"
                                } else {
                                    "Downloading"
                                })
                                .color(theme::PARCHMENT_DIM),
                            );
                            ui.label(RichText::new(&a.game_name).size(17.0).strong());
                            ui.label(match &a.from_version {
                                Some(f) => format!("{f} -> {}", a.to_version()),
                                None => a.to_version().to_string(),
                            });
                            ui.add(egui::ProgressBar::new(a.progress.clamp(0.0, 1.0) as f32).fill(accent));
                            let mut line = if a.state == DownloadState::Installing {
                                "Putting the mod in the game...".to_string()
                            } else {
                                format!("{} of {}", size_text(a.bytes_done()), size_text(a.size()))
                            };
                            if a.state == DownloadState::Downloading && speed > 0.0 {
                                line += &format!(" - {}/s", size_text(speed as i64));
                            }
                            if let Some(t) = left {
                                line += &format!(" - {} min left", (t.as_secs() / 60).max(1));
                            }
                            ui.label(RichText::new(line).color(theme::PARCHMENT_DIM));
                            if a.state == DownloadState::Downloading && secondary_button(ui, "Cancel download", true).clicked() {
                                cancel = Some(a.id.clone());
                            }
                        } else {
                            ui.label(
                                RichText::new(if pending.is_empty() {
                                    "All your mods are up to date"
                                } else {
                                    "Some mods have an update"
                                })
                                .size(16.0)
                                .strong(),
                            );
                            if secondary_button(ui, "Check for updates", true).clicked() {
                                check = true;
                            }
                        }
                    });
                    if !waiting.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new("Up next").strong());
                        for w in &waiting {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}  {}", w.game_name, w.to_version()));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("Cancel").clicked() {
                                        cancel = Some(w.id.clone());
                                    }
                                    if ui.small_button("First").clicked() {
                                        first = Some(w.id.clone());
                                    }
                                });
                            });
                        }
                    }
                    if !pending.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new("Updates").strong());
                        for (g, info) in &pending {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}  {}", g.display_name, info.version));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if primary_button(ui, "UPDATE", accent, true).clicked() {
                                        get = Some(g.id.clone());
                                    }
                                });
                            });
                        }
                    }
                    if !finished.is_empty() {
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Finished").strong());
                            if ui.small_button("Clear").clicked() {
                                clear = true;
                            }
                        });
                        for f in &finished {
                            let (t, c) = match f.state {
                                DownloadState::Done => ("done", theme::ONLINE),
                                DownloadState::Failed => ("failed", theme::DANGER_BRIGHT),
                                _ => ("cancelled", theme::MUTED),
                            };
                            ui.horizontal(|ui| {
                                ui.label(format!("{}  {}", f.game_name, f.to_version()));
                                ui.label(RichText::new(t).color(c));
                            });
                        }
                    }
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
        if check {
            self.load_offers();
        }
        if clear {
            self.downloads.clear_finished();
        }
        if close {
            self.updates_open = false;
        }
    }
}
