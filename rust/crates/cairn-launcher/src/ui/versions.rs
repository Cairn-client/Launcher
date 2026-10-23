// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Versions: what is running, the check for updates, and the changelog of every release.

use super::theme;
use super::widgets::{card, chip, column, primary_button};
use crate::app::{CairnApp, VERSION};
use crate::changelog;
use crate::update_check::normalize;
use cairn_setup::time_util;
use eframe::egui::{self, RichText};

impl CairnApp {
    pub fn versions_page(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::GROUND))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    column(ui, 860.0, 40.0, 28, |ui| {
                        ui.set_max_width(860.0);
                        ui.label(RichText::new("Versions").size(32.0).strong());
                        ui.label(RichText::new("What you are running, and what changed in each release.").color(theme::PARCHMENT_DIM));
                        ui.add_space(14.0);
                        card(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("CAIRN CLIENT").size(11.0).color(theme::MUTED).strong());
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(VERSION).size(20.0).strong());
                                        if self.update.is_some() {
                                            ui.painter()
                                                .circle_filled(ui.cursor().min + egui::vec2(6.0, 12.0), 5.0, self.accent);
                                            ui.add_space(14.0);
                                        }
                                    });
                                });
                                if let Some(u) = &self.update {
                                    ui.add_space(30.0);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new("NEWER VERSION").size(11.0).color(theme::MUTED).strong());
                                        ui.label(RichText::new(&u.tag).size(20.0).strong().color(self.accent));
                                    });
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let busy = self.update_checking;
                                    if primary_button(ui, if busy { "CHECKING..." } else { "CHECK FOR UPDATES" }, self.accent, !busy)
                                        .clicked()
                                    {
                                        self.check_for_update(true);
                                    }
                                });
                            });
                        });
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Changelog").size(18.0).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.link("Open on GitHub").clicked() {
                                    crate::open::open_url(changelog::PAGE_URL);
                                }
                            });
                        });
                        ui.add_space(6.0);
                        let Some(releases) = self.release_notes.clone() else {
                            card(ui, |ui| {
                                ui.label(
                                    RichText::new("The changelog could not be loaded (no connection, or GitHub did not answer).")
                                        .color(theme::MUTED),
                                );
                                if ui.link("Try again").clicked() {
                                    self.load_changelog();
                                }
                            });
                            return;
                        };
                        let current = normalize(VERSION);
                        for r in &releases {
                            let is_current = normalize(&r.tag) == current && current.is_some();
                            card(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 10.0;
                                    ui.label(RichText::new(&r.tag).size(17.0).strong().color(if is_current {
                                        self.accent
                                    } else {
                                        theme::PARCHMENT
                                    }));
                                    if is_current {
                                        chip(ui, "You are here", self.accent);
                                    }
                                    if r.prerelease {
                                        chip(ui, "Pre-release", theme::MUTED);
                                    }
                                    if let Some(t) = r.published {
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let (y, m, d) = time_util::civil_from_days(t.div_euclid(86_400));
                                            ui.label(RichText::new(format!("{d} {} {y}", MONTHS[(m - 1) as usize])).color(theme::MUTED));
                                        });
                                    }
                                });
                                ui.add_space(4.0);
                                if r.items.is_empty() {
                                    ui.label(RichText::new("Maintenance and stability improvements.").color(theme::MUTED));
                                }
                                for item in &r.items {
                                    ui.add(egui::Label::new(RichText::new(format!("•  {item}")).color(theme::PARCHMENT_DIM)).wrap());
                                }
                            });
                            ui.add_space(10.0);
                        }
                    });
                });
            });
    }
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
