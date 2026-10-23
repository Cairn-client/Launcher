// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use cairn_launcher::app::CairnApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cairn")
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([1000.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native("Cairn", options, Box::new(|cc| Ok(Box::new(CairnApp::new(cc.egui_ctx.clone())))))
}
