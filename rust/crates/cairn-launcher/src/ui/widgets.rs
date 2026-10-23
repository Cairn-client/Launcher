// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The small pieces every page uses: buttons, chips, cards and a picture that covers a box.

use super::theme;
use eframe::egui::{self, Color32, CornerRadius, Rect, Response, Sense, Stroke, TextureHandle, Ui, Vec2};

/// A text button of the top bar: the page the player is on has the accent colour and a line under it.
/// A smooth fade of `color` over `rect`: from the left edge to the right when `horizontal`, else from the top to the bottom. `alpha` gives the
/// opacity at 0 (the start) to 1 (the end). One mesh with a colour at each stop, so there are no stripes.
pub fn fade(ui: &Ui, rect: Rect, horizontal: bool, color: Color32, alpha: impl Fn(f32) -> f32) {
    let steps = 32;
    let mut mesh = egui::Mesh::default();
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let c = theme::with_alpha(color, alpha(t));
        let (a, b) = if horizontal {
            let x = rect.left() + rect.width() * t;
            (egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom()))
        } else {
            let y = rect.top() + rect.height() * t;
            (egui::pos2(rect.left(), y), egui::pos2(rect.right(), y))
        };
        mesh.colored_vertex(a, c);
        mesh.colored_vertex(b, c);
        if i > 0 {
            let k = (i * 2) as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// A page's column: at most `max` wide, centred, with `pad` at least on each side and `vertical` above and below.
pub fn column<R>(ui: &mut Ui, max: f32, pad: f32, vertical: i8, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_width();
    let width = (avail - 2.0 * pad).min(max).max(0.0);
    let left = ((avail - width) / 2.0).max(pad);
    let spacing = ui.spacing().item_spacing;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(left);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = spacing;
            ui.set_width(width);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(0, vertical))
                .show(ui, add)
                .inner
        })
        .inner
    })
    .inner
}

pub fn nav_button(ui: &mut Ui, text: &str, active: bool, accent: Color32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), egui::FontId::proportional(14.5), Color32::WHITE);
    let size = Vec2::new(galley.size().x + 28.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let color = if active {
        accent
    } else if hovered {
        theme::PARCHMENT
    } else {
        theme::PARCHMENT_DIM
    };
    if active || hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            theme::with_alpha(if active { accent } else { Color32::WHITE }, if active { 0.12 } else { 0.05 }),
        );
    }
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), egui::FontId::proportional(14.5), color);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    if active {
        let y = rect.bottom() - 1.0;
        ui.painter().line_segment(
            [egui::pos2(rect.left() + 14.0, y), egui::pos2(rect.right() - 14.0, y)],
            Stroke::new(2.0, accent),
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A filled button in the accent colour (the main action of a page).
pub fn primary_button(ui: &mut Ui, text: &str, accent: Color32, enabled: bool) -> Response {
    let font = egui::FontId::proportional(13.5);
    let text = text.to_uppercase();
    let galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), Color32::BLACK);
    let size = Vec2::new(galley.size().x + 32.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let fill = if !enabled {
        theme::with_alpha(accent, 0.25)
    } else if resp.hovered() {
        theme::mix(accent, Color32::WHITE, 0.15)
    } else {
        accent
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    let color = if enabled {
        Color32::from_rgb(0x14, 0x12, 0x10)
    } else {
        theme::with_alpha(Color32::WHITE, 0.45)
    };
    let galley = ui.painter().layout_no_wrap(text, font, color);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// A quiet button: a hairline outline.
pub fn secondary_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    let font = egui::FontId::proportional(13.5);
    let color = if enabled { theme::PARCHMENT } else { theme::MUTED_DIM };
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), font.clone(), color);
    let size = Vec2::new(galley.size().x + 32.0, 36.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hovered = enabled && resp.hovered();
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(10),
        if hovered {
            theme::OAK_LIT
        } else {
            theme::with_alpha(Color32::WHITE, 0.04)
        },
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, theme::with_alpha(Color32::WHITE, if hovered { 0.22 } else { 0.12 })),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// A small pill of text (a state, a count).
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    let font = egui::FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), color);
    let (rect, _) = ui.allocate_exact_size(galley.size() + Vec2::new(18.0, 10.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(255), theme::with_alpha(color, 0.14));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
}

/// Paints the picture so that it covers `rect` (cropped, not stretched), optionally with rounded corners.
pub fn paint_cover(ui: &Ui, texture: &TextureHandle, rect: Rect, rounding: u8, tint: Color32) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let scale = (rect.width() / size.x).max(rect.height() / size.y);
    let shown = Vec2::new(
        rect.width() / scale,
        rect.height() / size.y.max(1.0) * (size.y / (rect.height() / scale).max(1.0)) * 0.0 + rect.height() / scale,
    );
    let uv = Rect::from_center_size(egui::pos2(0.5, 0.5), Vec2::new(shown.x / size.x, shown.y / size.y));
    let mut mesh = egui::Mesh::with_texture(texture.id());
    if rounding == 0 {
        mesh.add_rect_with_uv(rect, uv, tint);
        ui.painter().add(egui::Shape::mesh(mesh));
    } else {
        ui.painter()
            .add(egui::epaint::RectShape::filled(rect, CornerRadius::same(rounding), tint).with_texture(texture.id(), uv));
    }
}

/// Paints the picture whole inside `rect` (kept in proportion, centred).
pub fn paint_contain(ui: &Ui, texture: &TextureHandle, rect: Rect, tint: Color32) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let target = Rect::from_center_size(rect.center(), size * scale);
    ui.painter().image(
        texture.id(),
        target,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        tint,
    );
}

/// A card: a near-black box with a white hairline.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let inner_w = (ui.available_width() - 38.0).max(0.0);
    egui::Frame::new()
        .fill(theme::INK)
        .stroke(Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.08)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            ui.set_min_width(inner_w);
            add(ui)
        })
        .inner
}

/// What a poster of a game shows.
pub struct PosterInfo<'a> {
    pub name: &'a str,
    pub sub: &'a str,
    pub flag: Option<&'a str>,
    pub favorite: bool,
    /// Whether the favourite heart is there: it belongs to the Library's games, not the Store's.
    pub heart: bool,
    pub accent: Color32,
}

/// A tall poster: the game's picture with its name over a shade at the bottom. `art` is the picture when it is there. Returns (opened, heart pressed).
/// A poster's size: it grows with the window, so on a wide screen the covers are not thumbnails.
pub fn poster_size(ui: &Ui) -> Vec2 {
    let scale = (ui.ctx().content_rect().width() / 1300.0).clamp(1.0, 1.6);
    Vec2::new(172.0, 258.0) * scale
}

pub fn poster(ui: &mut Ui, art: Option<&TextureHandle>, info: &PosterInfo) -> (bool, bool) {
    let size = poster_size(ui);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let round = CornerRadius::same(12);
    ui.painter().rect_filled(rect, round, theme::INK);
    if let Some(tex) = art {
        paint_cover(ui, tex, rect, 12, Color32::WHITE);
    }
    // the shade under the name
    let shade = Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 96.0), rect.max);
    let steps = 12;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let y0 = shade.top() + shade.height() * t;
        let y1 = shade.top() + shade.height() * (t + 1.0 / steps as f32);
        let band = Rect::from_min_max(egui::pos2(shade.left(), y0), egui::pos2(shade.right(), y1));
        let bottom_round = if i == steps - 1 {
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 12,
                se: 12,
            }
        } else {
            CornerRadius::ZERO
        };
        ui.painter()
            .rect_filled(band, bottom_round, theme::with_alpha(Color32::BLACK, (t * 0.86 + 0.04).min(0.9)));
    }
    ui.painter().text(
        egui::pos2(rect.left() + 12.0, rect.bottom() - 36.0),
        egui::Align2::LEFT_CENTER,
        info.name,
        egui::FontId::proportional(14.5),
        theme::PARCHMENT,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 12.0, rect.bottom() - 16.0),
        egui::Align2::LEFT_CENTER,
        info.sub,
        egui::FontId::proportional(12.0),
        theme::PARCHMENT_DIM,
    );
    if resp.hovered() {
        ui.painter()
            .rect_stroke(rect, round, Stroke::new(2.0, info.accent), egui::StrokeKind::Inside);
    } else {
        ui.painter().rect_stroke(
            rect,
            round,
            Stroke::new(1.0, theme::with_alpha(Color32::WHITE, 0.1)),
            egui::StrokeKind::Inside,
        );
    }
    if let Some(flag) = info.flag {
        let galley = ui
            .painter()
            .layout_no_wrap(flag.to_string(), egui::FontId::proportional(11.0), Color32::BLACK);
        let tag = Rect::from_min_size(
            egui::pos2(rect.left() + 8.0, rect.top() + 8.0),
            galley.size() + Vec2::new(14.0, 8.0),
        );
        ui.painter().rect_filled(tag, CornerRadius::same(255), info.accent);
        ui.painter().galley(tag.center() - galley.size() / 2.0, galley, Color32::BLACK);
    }
    if !info.heart {
        return (resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked(), false);
    }
    // the heart is a corner of the poster's own click area (no widget on top of another), so one press does one thing
    let heart = Rect::from_center_size(egui::pos2(rect.right() - 22.0, rect.top() + 22.0), Vec2::splat(34.0));
    let over_heart = resp.hover_pos().is_some_and(|p| heart.contains(p));
    ui.painter().circle_filled(
        heart.center(),
        15.0,
        theme::with_alpha(Color32::BLACK, if over_heart { 0.75 } else { 0.5 }),
    );
    super::icons::heart(
        ui.painter(),
        heart.center(),
        13.0,
        info.favorite,
        if info.favorite { info.accent } else { theme::PARCHMENT },
    );
    let heart_clicked = resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| heart.contains(p));
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if over_heart {
        resp.clone().on_hover_text(if info.favorite {
            "Remove from favourites"
        } else {
            "Add to favourites"
        });
    }
    (resp.clicked() && !heart_clicked, heart_clicked)
}
