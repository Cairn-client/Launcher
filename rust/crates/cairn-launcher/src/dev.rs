//! Development aids: with `CAIRN_SCREENSHOT=<file>` the window saves a picture of itself after a few seconds and closes, so a page can be
//! looked at without a person. Nothing happens without the variable.

use eframe::egui::{self, ViewportCommand};
use std::sync::atomic::{AtomicBool, Ordering};

static ASKED: AtomicBool = AtomicBool::new(false);

/// Call once per frame.
pub fn screenshot_tick(ctx: &egui::Context) {
    let Ok(path) = std::env::var("CAIRN_SCREENSHOT") else { return };
    let after_secs: f64 = std::env::var("CAIRN_SCREENSHOT_AFTER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5.0);
    ctx.request_repaint_after(std::time::Duration::from_millis(200));
    if ctx.input(|i| i.time) > after_secs && !ASKED.swap(true, Ordering::Relaxed) {
        let done = ctx.clone();
        ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
            let (w, h) = (img.width() as u32, img.height() as u32);
            if let Some(buf) = image::RgbaImage::from_raw(w, h, img.as_raw().to_vec()) {
                let _ = buf.save(&path);
            }
            done.send_viewport_cmd(ViewportCommand::Close);
            done.request_repaint();
        })));
    }
}
