// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use cairn_launcher::app::CairnApp;
use cairn_launcher::app_dirs;
use cairn_launcher::prefs::WindowPrefs;
use cairn_launcher::ui::theme;
use eframe::egui;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Cairn")
        .with_inner_size([1400.0, 860.0])
        .with_min_inner_size([1000.0, 640.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/app.png")) {
        viewport = viewport.with_icon(icon);
    }
    if let Some(w) =
        WindowPrefs::load(&app_dirs::window_file()).filter(|w| w.is_usable(1.0, |x, y| (0..8000).contains(&x) && (0..5000).contains(&y)))
    {
        viewport = viewport
            .with_inner_size([w.width as f32, w.height as f32])
            .with_position([w.x as f32, w.y as f32])
            .with_maximized(w.maximized);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Cairn",
        options,
        Box::new(|cc| {
            theme::install_fonts(&cc.egui_ctx);
            Ok(Box::new(CairnApp::new(cc.egui_ctx.clone())))
        }),
    )
}
