// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The launcher's own update, in a window: what is new, then the download's progress, then (if it fails) why.

use super::theme;
use super::widgets::{primary_button, secondary_button};
use crate::app::{CairnApp, Msg, UpdateModal};
use crate::update_check;
use eframe::egui::{self, RichText, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

impl CairnApp {
    pub fn open_update_modal(&mut self, stage: &str, text: &str) {
        self.update_modal = Some(UpdateModal {
            stage: stage.to_string(),
            text: text.to_string(),
            progress: None,
            cancel: Arc::new(AtomicBool::new(false)),
        });
    }

    pub fn update_failed(&mut self, text: &str) {
        match self.update_modal.as_mut() {
            Some(m) => {
                m.stage = "error".into();
                m.text = text.to_string();
                m.progress = None;
            }
            None => self.toast(text, true),
        }
    }

    /// Downloads the installer on a worker thread; when it is whole and checked the launcher hands over to it.
    fn run_update(&mut self) {
        let Some(info) = self.update.clone() else { return };
        if info.download_url.is_none() || info.sha256.is_none() {
            self.update_failed("This release has no installer file the launcher can check. Download it from the release page.");
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.update_modal = Some(UpdateModal {
            stage: "busy".into(),
            text: "Starting...".into(),
            progress: None,
            cancel: cancel.clone(),
        });
        let (tx, http, ctx) = (self.tx.clone(), self.http.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let (tx2, ctx2) = (tx.clone(), ctx.clone());
            let result = update_check::download(
                &info,
                http.as_ref(),
                &mut |p| {
                    let _ = tx2.send(Msg::UpdateProgress(p));
                    ctx2.request_repaint();
                },
                &cancel,
            );
            let _ = tx.send(Msg::UpdateDownloaded(result));
            ctx.request_repaint();
        });
    }

    pub fn update_window(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.update_modal.clone() else { return };
        let tag = self.update.as_ref().map(|u| u.tag.clone()).unwrap_or_default();
        let title = match modal.stage.as_str() {
            "busy" => "UPDATING".to_string(),
            "error" => "UPDATE FAILED".into(),
            "current" => "UP TO DATE".into(),
            "unreachable" => "CAN'T CHECK".into(),
            _ => format!("{tag} IS OUT"),
        };
        let (mut close, mut update, mut page) = (false, false, false);
        egui::Window::new(RichText::new(title).strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .min_width(460.0)
            .max_width(520.0)
            .show(ctx, |ui| match modal.stage.as_str() {
                "current" | "unreachable" => {
                    ui.label(if modal.stage == "current" {
                        "You have the newest version of Cairn."
                    } else {
                        "GitHub could not be reached. Check your connection and try again."
                    });
                    if primary_button(ui, "CLOSE", self.accent, true).clicked() {
                        close = true;
                    }
                }
                "offer" => {
                    if let Some(u) = &self.update {
                        if !u.notes.is_empty() {
                            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                                ui.add(egui::Label::new(RichText::new(&u.notes).size(14.0)).wrap());
                            });
                        }
                    }
                    ui.label(
                        RichText::new("The launcher closes, installs it over this copy and opens again. Your saves and settings stay.")
                            .color(theme::PARCHMENT_DIM),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if primary_button(ui, "UPDATE NOW", self.accent, true).clicked() {
                            update = true;
                        }
                        if secondary_button(ui, "LATER", true).clicked() {
                            close = true;
                        }
                    });
                }
                "busy" => {
                    ui.label(&modal.text);
                    if let Some(p) = modal.progress {
                        ui.add(egui::ProgressBar::new(p as f32).fill(self.accent));
                    }
                    ui.label(RichText::new("Please do not close the launcher.").color(theme::MUTED).size(12.5));
                    if secondary_button(ui, "CANCEL", true).clicked() {
                        modal.cancel.store(true, Ordering::Relaxed);
                    }
                }
                _ => {
                    ui.add(egui::Label::new(&modal.text).wrap());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if primary_button(ui, "OPEN THE RELEASE PAGE", self.accent, true).clicked() {
                            page = true;
                        }
                        if secondary_button(ui, "CLOSE", true).clicked() {
                            close = true;
                        }
                    });
                }
            });
        if update {
            self.run_update();
        }
        if page {
            if let Some(u) = &self.update {
                crate::open::open_url(&u.page_url);
            }
        }
        if close {
            self.update_modal = None;
        }
    }
}
