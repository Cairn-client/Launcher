// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Pictures: the logos, backgrounds and screenshots of the games. A picture is fetched on a worker thread (or read from the copy kept on
//! disk, or from the ones that ship inside the launcher), decoded there, and turned into a texture on the window's thread when it arrives.

use crate::games;
use cairn_setup::http::Http;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

enum Slot {
    Loading,
    Ready(TextureHandle),
    Failed,
}

pub struct ImageCache {
    slots: HashMap<String, Slot>,
    tx: Sender<(String, Option<ColorImage>)>,
    rx: Receiver<(String, Option<ColorImage>)>,
    http: Arc<dyn Http>,
    dir: PathBuf,
    /// A few threads at a time are enough for a game list.
    in_flight: usize,
    queue: Vec<String>,
}

const MAX_IN_FLIGHT: usize = 6;
const MAX_BYTES: u64 = 12 * 1024 * 1024;

impl ImageCache {
    pub fn new(http: Arc<dyn Http>, dir: PathBuf) -> Self {
        let (tx, rx) = channel();
        Self {
            slots: HashMap::new(),
            tx,
            rx,
            http,
            dir,
            in_flight: 0,
            queue: Vec::new(),
        }
    }

    /// The texture of a picture, when it is there; asks for it to be loaded the first time. `None` while it loads, or when it could not be had.
    pub fn get(&mut self, ctx: &egui::Context, url: &str) -> Option<TextureHandle> {
        if url.is_empty() {
            return None;
        }
        self.receive(ctx);
        match self.slots.get(url) {
            Some(Slot::Ready(t)) => return Some(t.clone()),
            Some(_) => return None,
            None => {}
        }
        self.slots.insert(url.to_string(), Slot::Loading);
        self.queue.push(url.to_string());
        self.start_more(ctx);
        None
    }

    /// Whether the picture could not be loaded (so the caller can show something else instead of waiting).
    pub fn failed(&self, url: &str) -> bool {
        matches!(self.slots.get(url), Some(Slot::Failed))
    }

    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok((url, image)) = self.rx.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            let slot = match image {
                Some(img) => Slot::Ready(ctx.load_texture(&url, img, TextureOptions::LINEAR)),
                None => Slot::Failed,
            };
            self.slots.insert(url, slot);
            ctx.request_repaint();
        }
        self.start_more(ctx);
    }

    fn start_more(&mut self, ctx: &egui::Context) {
        while self.in_flight < MAX_IN_FLIGHT && !self.queue.is_empty() {
            let url = self.queue.remove(0);
            self.in_flight += 1;
            let (tx, http, dir, ctx) = (self.tx.clone(), self.http.clone(), self.dir.clone(), ctx.clone());
            std::thread::spawn(move || {
                let image = load(&url, http.as_ref(), &dir);
                let _ = tx.send((url, image));
                ctx.request_repaint();
            });
        }
    }
}

/// The bytes of a picture: shipped, kept on disk, or fetched (and then kept).
fn load(url: &str, http: &dyn Http, dir: &std::path::Path) -> Option<ColorImage> {
    let bytes = if let Some(name) = url.strip_prefix(games::SEED_BASE) {
        games::seed_picture(name)?.to_vec()
    } else {
        let file = dir.join(cache_name(url));
        match fs::read(&file) {
            Ok(b) if !b.is_empty() => b,
            _ => {
                let resp = http.get(url).ok().filter(|r| r.is_success())?;
                if resp.content_length.is_some_and(|n| n > MAX_BYTES) {
                    return None;
                }
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut std::io::Read::take(resp.body, MAX_BYTES + 1), &mut bytes).ok()?;
                if bytes.len() as u64 > MAX_BYTES {
                    return None;
                }
                if fs::create_dir_all(dir).is_ok() {
                    let _ = fs::write(&file, &bytes);
                }
                bytes
            }
        }
    };
    decode(&bytes)
}

/// A picture as pixels; a very large one is brought down to what a window can show.
pub fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let mut img = image::load_from_memory(bytes).ok()?;
    if img.width() > 2560 || img.height() > 2560 {
        img = img.resize(2560, 2560, image::imageops::FilterType::Triangle);
    }
    let rgba = img.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

fn cache_name(url: &str) -> String {
    let h: String = Sha256::digest(url.as_bytes()).iter().take(12).map(|b| format!("{b:02x}")).collect();
    let ext = url
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 4 && e.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or("img");
    format!("{h}.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shipped_picture_decodes_and_garbage_does_not() {
        let img = decode(games::seed_picture("kcd2-logo.png").unwrap()).unwrap();
        assert!(img.width() > 10 && img.height() > 10);
        assert!(decode(b"not a picture").is_none());
    }

    #[test]
    fn a_cache_name_is_plain_and_stable() {
        let a = cache_name("https://x.test/a/b.png");
        assert_eq!(a, cache_name("https://x.test/a/b.png"));
        assert_ne!(a, cache_name("https://x.test/a/c.png"));
        assert!(a.ends_with(".png") && a.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.'));
        assert!(cache_name("https://x.test/noext").ends_with(".img") || cache_name("https://x.test/noext").len() > 4);
    }
}
