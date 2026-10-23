// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Help and About in one page. The credit to the projects Cairn continues and the disclaimer stay (GPLv3 section 7, AUTHORS.md).

use super::theme;
use super::widgets::{card, column, primary_button, secondary_button};
use crate::app::{CairnApp, View, VERSION};
use cairn_setup::catalog::KNOWN_ISSUE;
use eframe::egui::{self, RichText};

pub const REPO_URL: &str = "https://github.com/Cairn-client/Launcher";

impl CairnApp {
    pub fn help_page(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::GROUND)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                column(ui, 980.0, 40.0, 28, |ui| {
                    ui.set_max_width(980.0);
                    ui.label(RichText::new("Help").size(32.0).strong());
                    ui.label(RichText::new("Get unstuck, report a problem, and find out who made what.").color(theme::PARCHMENT_DIM));
                    ui.add_space(14.0);
                    let notices: Vec<_> = self.registry.catalog().notices.clone();
                    if !notices.is_empty() {
                        card(ui, |ui| {
                            ui.label(RichText::new("Known issues and news").size(18.0).strong());
                            for n in &notices {
                                ui.add(egui::Label::new(RichText::new(format!("{}{} {}", n.title, if n.kind == KNOWN_ISSUE { " (known issue)" } else { "" }, n.text))).wrap());
                            }
                        });
                        ui.add_space(12.0);
                    }
                    ui.columns(2, |cols| {
                        card(&mut cols[0], |ui| {
                            ui.label(RichText::new("Having trouble?").size(18.0).strong());
                            ui.add_space(4.0);
                            self.help_line(ui, "Check your setup.", "The checklist finds what is missing (Steam, the mod) and fixes most of it.", "Open the Library", |app| app.view = View::Library);
                            self.help_line(ui, "Mods.", "Each game's mod is downloaded from its page in the Library.", "Go to the Library", |app| app.view = View::Library);
                            self.help_line(ui, "Welcome tour.", "A quick walk through Cairn.", "Replay the tour", |app| {
                                app.prefs.tour_seen = false;
                                app.save_prefs();
                                app.view = View::Home;
                            });
                            self.help_line(ui, "Send the logs.", "Settings has a button that saves them in one zip; attach it to your report.", "Open Settings", |app| app.view = View::Settings);
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if primary_button(ui, "REPORT A BUG", self.accent, true).clicked() {
                                    crate::open::open_url(&format!("{REPO_URL}/issues/new"));
                                }
                                if secondary_button(ui, "OPEN THE REPOSITORY", true).clicked() {
                                    crate::open::open_url(REPO_URL);
                                }
                            });
                        });
                        card(&mut cols[1], |ui| {
                            ui.label(RichText::new("Keyboard").size(18.0).strong());
                            keys(ui, &[("F10", "Open or close Settings"), ("F5", "Refresh"), ("Esc", "Close a window, or leave Cairn")]);
                            ui.add_space(14.0);
                            ui.label(RichText::new("In the game").size(18.0).strong());
                            keys(ui, &[("Insert", "Open or close the mod menu"), ("Up Down", "Choose an item"), ("Left Right", "Change the chosen value"), ("Enter", "Do the chosen action"), ("Esc", "Close the mod menu")]);
                        });
                    });
                    ui.add_space(12.0);
                    card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("About Cairn Client").size(18.0).strong());
                            ui.label(RichText::new(format!("Version {VERSION}")).color(theme::MUTED));
                        });
                        ui.add(egui::Label::new("Cairn Client is a multiplayer platform for single-player games. One player hosts, a friend joins through Steam, and you share the same world.").wrap());
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(RichText::new("Cairn Client is a modified version, derived from Kingdom Come: Together by DeepFriedDepp, which continues marczukmichal/kcd2-multiplayer; their authors keep the copyright on their code. It is free software under the GNU General Public License version 3 (see LICENSE and AUTHORS.md for the complete list of contributors).").color(theme::MUTED)).wrap());
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(RichText::new("Every game shown here, its assets and its artwork belong to its studio and publisher. Cairn Client is unofficial and free, and is not affiliated with or endorsed by any of them.").color(theme::MUTED)).wrap());
                    });
                });
            });
        });
    }

    fn help_line(&mut self, ui: &mut egui::Ui, title: &str, text: &str, link: &str, action: impl FnOnce(&mut CairnApp)) {
        ui.add(egui::Label::new(RichText::new(title).strong()).wrap());
        ui.add(egui::Label::new(RichText::new(text).color(theme::PARCHMENT_DIM)).wrap());
        if ui.link(link).clicked() {
            action(self);
        }
        ui.add_space(6.0);
    }
}

fn keys(ui: &mut egui::Ui, rows: &[(&str, &str)]) {
    for (key, what) in rows {
        ui.horizontal(|ui| {
            for k in key.split(' ') {
                let galley = ui
                    .painter()
                    .layout_no_wrap(k.to_string(), egui::FontId::monospace(12.5), theme::PARCHMENT);
                let (rect, _) = ui.allocate_exact_size(galley.size() + egui::vec2(14.0, 8.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, egui::CornerRadius::same(5), theme::OAK_LIT);
                ui.painter().galley(rect.center() - galley.size() / 2.0, galley, theme::PARCHMENT);
            }
            ui.label(RichText::new(*what).color(theme::PARCHMENT_DIM));
        });
    }
}
