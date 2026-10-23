// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Small icons drawn with lines and shapes, so they need no font and look the same everywhere.

use eframe::egui::{pos2, Color32, Painter, Pos2, Rect, Shape, Stroke};

fn s(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

/// A chevron pointing left.
pub fn chevron_left(p: &Painter, c: Pos2, size: f32, color: Color32) {
    let h = size / 2.0;
    p.add(Shape::line(
        vec![pos2(c.x + h / 2.0, c.y - h), pos2(c.x - h / 2.0, c.y), pos2(c.x + h / 2.0, c.y + h)],
        s(1.8, color),
    ));
}

/// A chevron: `open` points down, otherwise right.
pub fn chevron(p: &Painter, c: Pos2, size: f32, open: bool, color: Color32) {
    let h = size / 2.0;
    let pts = if open {
        [pos2(c.x - h, c.y - h / 2.0), pos2(c.x, c.y + h / 2.0), pos2(c.x + h, c.y - h / 2.0)]
    } else {
        [pos2(c.x - h / 2.0, c.y - h), pos2(c.x + h / 2.0, c.y), pos2(c.x - h / 2.0, c.y + h)]
    };
    p.add(Shape::line(pts.to_vec(), s(1.8, color)));
}

pub fn cross(p: &Painter, c: Pos2, size: f32, color: Color32) {
    let h = size / 2.0;
    p.line_segment([pos2(c.x - h, c.y - h), pos2(c.x + h, c.y + h)], s(1.8, color));
    p.line_segment([pos2(c.x + h, c.y - h), pos2(c.x - h, c.y + h)], s(1.8, color));
}

pub fn check(p: &Painter, c: Pos2, size: f32, color: Color32) {
    let h = size / 2.0;
    p.add(Shape::line(
        vec![pos2(c.x - h, c.y), pos2(c.x - h * 0.3, c.y + h * 0.7), pos2(c.x + h, c.y - h * 0.6)],
        s(2.0, color),
    ));
}

pub fn play(p: &Painter, c: Pos2, size: f32, color: Color32) {
    let h = size / 2.0;
    p.add(Shape::convex_polygon(
        vec![pos2(c.x - h * 0.7, c.y - h), pos2(c.x + h, c.y), pos2(c.x - h * 0.7, c.y + h)],
        color,
        Stroke::NONE,
    ));
}

/// A heart: two round lobes and a point; `filled` or an outline.
pub fn heart(p: &Painter, c: Pos2, size: f32, filled: bool, color: Color32) {
    let r = size * 0.27;
    let lobes = [pos2(c.x - r, c.y - r * 0.55), pos2(c.x + r, c.y - r * 0.55)];
    let tip = pos2(c.x, c.y + size * 0.48);
    let (left, right) = (pos2(c.x - r * 1.95, c.y - r * 0.2), pos2(c.x + r * 1.95, c.y - r * 0.2));
    if filled {
        for l in lobes {
            p.circle_filled(l, r, color);
        }
        p.add(Shape::convex_polygon(vec![left, right, tip], color, Stroke::NONE));
    } else {
        let st = s(1.6, color);
        p.add(Shape::closed_line(
            vec![
                left,
                pos2(c.x - r * 1.5, c.y - r * 1.5),
                pos2(c.x - r * 0.4, c.y - r * 1.55),
                pos2(c.x, c.y - r * 0.55),
                pos2(c.x + r * 0.4, c.y - r * 1.55),
                pos2(c.x + r * 1.5, c.y - r * 1.5),
                right,
                tip,
            ],
            st,
        ));
    }
}

/// A tray with an arrow into it (the updates).
pub fn download(p: &Painter, c: Pos2, color: Color32) {
    let st = s(1.8, color);
    p.line_segment([pos2(c.x, c.y - 7.0), pos2(c.x, c.y + 3.0)], st);
    p.line_segment([pos2(c.x - 4.5, c.y - 1.5), pos2(c.x, c.y + 3.0)], st);
    p.line_segment([pos2(c.x + 4.5, c.y - 1.5), pos2(c.x, c.y + 3.0)], st);
    p.line_segment([pos2(c.x - 7.0, c.y + 7.0), pos2(c.x + 7.0, c.y + 7.0)], st);
}

/// A magnifying glass.
pub fn search(p: &Painter, c: Pos2, size: f32, color: Color32) {
    let r = size * 0.32;
    p.circle_stroke(pos2(c.x - r * 0.3, c.y - r * 0.3), r, s(1.6, color));
    p.line_segment(
        [pos2(c.x + r * 0.7, c.y + r * 0.7), pos2(c.x + size * 0.48, c.y + size * 0.48)],
        s(1.8, color),
    );
}

/// A small filled disc (a state dot).
pub fn dot(p: &Painter, c: Pos2, r: f32, color: Color32) {
    p.circle_filled(c, r, color);
}

/// A four-line "sliders" mark for settings.
pub fn sliders(p: &Painter, rect: Rect, color: Color32) {
    let st = s(1.6, color);
    let c = rect.center();
    for (i, off) in [-5.0_f32, 0.0, 5.0].iter().enumerate() {
        p.line_segment([pos2(c.x - 7.0, c.y + off), pos2(c.x + 7.0, c.y + off)], st);
        p.circle_filled(pos2(c.x + [-3.0, 3.0, -1.0][i], c.y + off), 2.2, color);
    }
}

/// A person (the friends).
pub fn person(p: &Painter, c: Pos2, color: Color32) {
    p.circle_stroke(pos2(c.x, c.y - 4.0), 3.6, s(1.6, color));
    p.add(Shape::line(
        vec![
            pos2(c.x - 7.0, c.y + 8.0),
            pos2(c.x - 6.0, c.y + 3.0),
            pos2(c.x + 6.0, c.y + 3.0),
            pos2(c.x + 7.0, c.y + 8.0),
        ],
        s(1.6, color),
    ));
}
