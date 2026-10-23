// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The look of the launcher: an onyx ground, near-black cards with a white hairline, cool-grey text steps and one accent colour (the player's
//! choice in Settings, or the game's own on its page). The values are those of the C# launcher's style sheet.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle};

pub const GROUND: Color32 = Color32::from_rgb(0x0C, 0x0C, 0x0C);
pub const INK: Color32 = Color32::from_rgb(0x0B, 0x0C, 0x0D);
pub const OAK: Color32 = Color32::from_rgb(0x1D, 0x21, 0x25);
pub const OAK_LIT: Color32 = Color32::from_rgb(0x2B, 0x30, 0x36);
pub const PARCHMENT: Color32 = Color32::from_rgb(0xFB, 0xFB, 0xFA);
pub const PARCHMENT_DIM: Color32 = Color32::from_rgb(0xB5, 0xBC, 0xC0);
pub const MUTED: Color32 = Color32::from_rgb(0x8A, 0x94, 0x9B);
pub const MUTED_DIM: Color32 = Color32::from_rgb(0x64, 0x6D, 0x74);
pub const BLOOD: Color32 = Color32::from_rgb(0xC2, 0x4D, 0x43);
pub const DANGER_BRIGHT: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);
pub const ONLINE: Color32 = Color32::from_rgb(0x6F, 0xCF, 0x8A);
pub const TRACK: Color32 = Color32::from_rgb(0x06, 0x07, 0x07);
pub const WARN: Color32 = Color32::from_rgb(0xE8, 0xA3, 0x5A);
pub const FOCUS: Color32 = Color32::from_rgb(0x7A, 0xAE, 0xDC);

/// The accent colours of Settings > Appearance: id, the colour, the brighter one beside it.
pub const ACCENTS: [(&str, &str, &str); 6] = [
    ("gold", "#E8A35A", "#F2BA80"),
    ("blue", "#5AA9E8", "#80C0F2"),
    ("green", "#6FCF8A", "#92DEA8"),
    ("pink", "#E86AA8", "#F28FC0"),
    ("violet", "#9B7BE8", "#B7A0F2"),
    ("red", "#E8635A", "#F2857E"),
];

/// `#rrggbb` as a colour.
pub fn hex(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// The accent the player chose (gold when the id is not one of the six).
pub fn accent_of(id: &str) -> (Color32, Color32) {
    let (_, c, b) = ACCENTS.iter().find(|a| a.0 == id).unwrap_or(&ACCENTS[0]);
    (hex(c).unwrap_or(WARN), hex(b).unwrap_or(WARN))
}

pub fn with_alpha(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Mixes `a` towards `b` (0 = a, 1 = b).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Installs the dark visuals and the type sizes on the context.
pub fn install(ctx: &egui::Context, accent: Color32) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = GROUND;
    v.window_fill = OAK;
    v.extreme_bg_color = TRACK;
    v.faint_bg_color = INK;
    v.override_text_color = Some(PARCHMENT);
    v.hyperlink_color = accent;
    v.selection.bg_fill = with_alpha(accent, 0.35);
    v.selection.stroke = Stroke::new(1.0, accent);
    v.window_stroke = Stroke::new(1.0, OAK_LIT);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    let round = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = round;
    }
    v.widgets.noninteractive.bg_fill = OAK;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, with_alpha(Color32::WHITE, 0.08));
    v.widgets.inactive.bg_fill = OAK;
    v.widgets.inactive.weak_bg_fill = OAK;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, with_alpha(Color32::WHITE, 0.10));
    v.widgets.hovered.bg_fill = OAK_LIT;
    v.widgets.hovered.weak_bg_fill = OAK_LIT;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, with_alpha(accent, 0.6));
    v.widgets.active.bg_fill = OAK_LIT;
    v.widgets.active.weak_bg_fill = OAK_LIT;
    v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    ctx.set_visuals(v);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(28.0)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.interaction.selectable_labels = false;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_read_from_hex_and_the_accent_falls_back_to_gold() {
        assert_eq!(hex("#E8A35A"), Some(Color32::from_rgb(0xE8, 0xA3, 0x5A)));
        assert_eq!(hex("E8A35A"), None);
        assert_eq!(hex("#E8A3"), None);
        assert_eq!(accent_of("blue").0, hex("#5AA9E8").unwrap());
        assert_eq!(accent_of("nope").0, hex("#E8A35A").unwrap());
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 0.5), Color32::from_rgb(128, 128, 128));
    }
}
