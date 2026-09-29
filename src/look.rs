//! The look: wood frames, parchment panels, dark-brown ink, one green for the button to press.
//! Palette, fonts and the pixel widgets every screen is drawn from.

use eframe::egui::{self, Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use std::sync::Arc;

pub const WOOD: Color32 = Color32::from_rgb(0x7a, 0x4a, 0x26);
pub const WOOD_HI: Color32 = Color32::from_rgb(0xb0, 0x73, 0x3c);
pub const WOOD_LO: Color32 = Color32::from_rgb(0x3b, 0x21, 0x12);
pub const WOOD_SHADE: Color32 = Color32::from_rgb(0x8a, 0x55, 0x28);
pub const GRAIN: Color32 = Color32::from_rgb(0x6d, 0x41, 0x21);
pub const PARCH: Color32 = Color32::from_rgb(0xf3, 0xd9, 0xa4);
pub const PARCH_LO: Color32 = Color32::from_rgb(0xd9, 0xb5, 0x74);
pub const PARCH_LT: Color32 = Color32::from_rgb(0xfb, 0xe8, 0xbf);
pub const INK: Color32 = Color32::from_rgb(0x3b, 0x21, 0x12);
pub const INK_SOFT: Color32 = Color32::from_rgb(0x6b, 0x44, 0x24);
pub const GREEN: Color32 = Color32::from_rgb(0x5d, 0xbb, 0x4f);
pub const GREEN_LO: Color32 = Color32::from_rgb(0x2f, 0x7a, 0x2b);
pub const GOLD: Color32 = Color32::from_rgb(0xf6, 0xc3, 0x43);
pub const GOLD_LO: Color32 = Color32::from_rgb(0xc0, 0x8a, 0x17);
pub const RED: Color32 = Color32::from_rgb(0xe0, 0x48, 0x3e);
pub const BLUE: Color32 = Color32::from_rgb(0x7a, 0xa2, 0xff);
pub const BUBBLE: Color32 = Color32::from_rgb(0xff, 0xf8, 0xe6);
pub const HOT: Color32 = Color32::from_rgb(0xff, 0xf3, 0xc4);
// Effect colours, drawn over the dark rooms.
pub const LEAF: Color32 = Color32::from_rgb(0x7e, 0xd9, 0x57);
pub const PINK: Color32 = Color32::from_rgb(0xff, 0x8f, 0xc7);
pub const VIOLET: Color32 = Color32::from_rgb(0x8b, 0x7b, 0xff);
pub const MIST: Color32 = Color32::from_rgb(0xe8, 0xe4, 0xf0);
pub const DIM: Color32 = Color32::from_rgb(0x9a, 0x95, 0xa8);
pub const NIGHT: Color32 = Color32::from_rgb(0x1b, 0x16, 0x26);

/// Nunito Black, for titles and labels (plain proportional text is Nunito SemiBold).
pub fn heavy() -> FontFamily {
    FontFamily::Name("heavy".into())
}

/// Fonts, and egui's own widgets (text fields, tooltips, menus, scrollbars) dressed in the palette.
pub fn style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let nunito = |weight: f32| {
        let mut d = egui::FontData::from_static(include_bytes!("../assets/fonts/Nunito.ttf"));
        d.tweak.coords = egui::epaint::text::VariationCoords::new([(b"wght", weight)]);
        Arc::new(d)
    };
    fonts.font_data.insert("nunito".into(), nunito(650.0));
    fonts.font_data.insert("nunito-heavy".into(), nunito(900.0));
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.get_mut(&FontFamily::Proportional).expect("default family").insert(0, "nunito".into());
    fonts.families.insert(heavy(), std::iter::once("nunito-heavy".to_string()).chain(fallback).collect());
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Light);
    ctx.style_mut_of(egui::Theme::Light, |s| {
        s.spacing.item_spacing = vec2(6.0, 6.0);
        s.spacing.button_padding = vec2(10.0, 5.0);
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.floating = false;
        for (ts, size) in [(egui::TextStyle::Body, 14.0), (egui::TextStyle::Button, 14.0), (egui::TextStyle::Small, 12.0)] {
            if let Some(f) = s.text_styles.get_mut(&ts) {
                f.size = size;
            }
        }
        let v = &mut s.visuals;
        v.panel_fill = WOOD;
        v.window_fill = PARCH_LT;
        v.window_stroke = Stroke::new(2.0, WOOD_LO);
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 3], blur: 6, spread: 0, color: Color32::from_black_alpha(60) };
        v.extreme_bg_color = PARCH_LT;
        v.override_text_color = Some(INK);
        v.hyperlink_color = GREEN_LO;
        v.selection.bg_fill = GREEN.gamma_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0, GREEN_LO);
        v.text_cursor.stroke = Stroke::new(2.0, INK);
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::ZERO;
            w.fg_stroke = Stroke::new(1.0, INK);
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, PARCH_LO);
        v.widgets.inactive.bg_fill = WOOD_HI;
        v.widgets.inactive.weak_bg_fill = PARCH_LT;
        v.widgets.inactive.bg_stroke = Stroke::new(2.0, WOOD_LO);
        v.widgets.hovered.bg_fill = WOOD;
        v.widgets.hovered.weak_bg_fill = PARCH;
        v.widgets.hovered.bg_stroke = Stroke::new(2.0, WOOD_LO);
        v.widgets.active.bg_fill = WOOD_LO;
        v.widgets.active.weak_bg_fill = PARCH_LO;
        v.widgets.active.bg_stroke = Stroke::new(2.0, WOOD_LO);
    });
}

/// The window: a dark outline, a highlight ring and planks of wood with a bit of grain.
pub fn window_frame(p: &Painter, r: Rect) {
    p.rect_filled(r, 0.0, WOOD_LO);
    p.rect_filled(r.shrink(4.0), 0.0, WOOD_HI);
    let wood = r.shrink(7.0);
    p.rect_filled(wood, 0.0, WOOD);
    let mut y = wood.min.y + 9.0;
    let mut k = 0u32;
    while y < wood.max.y - 4.0 {
        k = k.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let x = wood.min.x + (k % 97) as f32 * 3.0;
        let w = 30.0 + (k / 97 % 60) as f32 * 2.0;
        p.rect_filled(Rect::from_min_max(pos2(x, y), pos2((x + w).min(wood.max.x), y + 2.0)), 0.0, GRAIN);
        y += 13.0 + (k % 7) as f32 * 2.0;
    }
}

/// Parchment (or any) panel: 3px outline, flat fill, a darker lip along the bottom.
pub fn plate(p: &Painter, r: Rect, fill: Color32, lip: Color32) {
    p.rect_filled(r, 0.0, WOOD_LO);
    let inner = r.shrink(3.0);
    p.rect_filled(inner, 0.0, fill);
    p.rect_filled(Rect::from_min_max(pos2(inner.min.x, inner.max.y - 4.0), inner.max), 0.0, lip);
}

/// Item frame in the bag and loot lists: rarity-coloured border around a light well.
pub fn slot(p: &Painter, r: Rect, rim: Color32) {
    p.rect_filled(r, 0.0, WOOD_LO);
    p.rect_filled(r.shrink(2.0), 0.0, rim);
    p.rect_filled(r.shrink(4.0), 0.0, PARCH_LT);
}

#[derive(Clone, Copy)]
pub enum Skin {
    Wood,
    Parch,
    Green,
}

/// A chunky pixel button. Returns the click response and the rect to draw its content in
/// (it sinks while pressed, when the lip moves to the top).
pub fn button(ui: &mut Ui, r: Rect, id: egui::Id, skin: Skin, on: bool) -> (Response, Rect) {
    let resp = ui.interact(r, id, if on { Sense::click() } else { Sense::hover() });
    let resp = if on { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp };
    let down = on && resp.is_pointer_button_down_on();
    let (fill, lip) = match skin {
        Skin::Wood => (WOOD_HI, WOOD_SHADE),
        Skin::Parch => (PARCH, PARCH_LO),
        Skin::Green => (GREEN, GREEN_LO),
    };
    let fill = if on && resp.hovered() && !down { fill.lerp_to_gamma(Color32::WHITE, 0.12) } else { fill };
    let (fill, lip) = if on { (fill, lip) } else { (fill.lerp_to_gamma(PARCH_LO, 0.5), lip.lerp_to_gamma(PARCH_LO, 0.5)) };
    let p = ui.painter();
    p.rect_filled(r, 0.0, WOOD_LO);
    let inner = r.shrink(3.0);
    p.rect_filled(inner, 0.0, fill);
    let (lip_r, content) = if down {
        (Rect::from_min_max(inner.min, pos2(inner.max.x, inner.min.y + 3.0)), Rect::from_min_max(inner.min + vec2(0.0, 3.0), inner.max))
    } else {
        (Rect::from_min_max(pos2(inner.min.x, inner.max.y - 4.0), inner.max), Rect::from_min_max(inner.min, inner.max - vec2(0.0, 4.0)))
    };
    p.rect_filled(lip_r, 0.0, lip);
    if resp.has_focus() {
        p.rect_stroke(r.expand(1.0), 0.0, Stroke::new(2.0, GOLD), StrokeKind::Outside);
    }
    (resp, content)
}

/// One line of text, cut with an ellipsis at `max_w`. Returns where it landed.
pub fn text1(p: &Painter, anchor: Pos2, align: Align2, text: &str, font: FontId, color: Color32, max_w: f32) -> Rect {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(1.0));
    let g = p.layout_job(job);
    let r = align.anchor_size(anchor, g.size());
    p.galley(r.min, g, color);
    r
}

/// Pixel arrow (◀ / ▶) made of stepped 2px columns.
pub fn arrow(p: &Painter, c: Pos2, dir: f32, color: Color32) {
    for k in 0..6 {
        let h = 2.0 + k as f32 * 2.0;
        let x = c.x + dir * (5.0 - k as f32 * 2.0) - 1.0;
        p.rect_filled(Rect::from_min_size(pos2(x, c.y - h / 2.0), vec2(2.0, h)), 0.0, color);
    }
}

/// Four corner brackets; with `inward` a dot sits inside each, pointing the window smaller.
pub fn expand_icon(p: &Painter, c: Pos2, inward: bool) {
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let corner = c + vec2(sx * 6.0, sy * 5.0);
        p.rect_filled(Rect::from_two_pos(corner, corner - vec2(sx * 5.0, sy * 2.0)), 0.0, PARCH);
        p.rect_filled(Rect::from_two_pos(corner, corner - vec2(sx * 2.0, sy * 5.0)), 0.0, PARCH);
        if inward {
            p.rect_filled(Rect::from_center_size(corner - vec2(sx * 3.5, sy * 3.5), vec2(2.0, 2.0)), 0.0, PARCH);
        }
    }
}

pub fn cross(p: &Painter, c: Pos2, color: Color32) {
    for k in -2..=2 {
        let o = k as f32 * 2.0;
        p.rect_filled(Rect::from_center_size(c + vec2(o, o), vec2(2.0, 2.0)), 0.0, color);
        p.rect_filled(Rect::from_center_size(c + vec2(o, -o), vec2(2.0, 2.0)), 0.0, color);
    }
}

pub fn coin(p: &Painter, c: Pos2) {
    let r = Rect::from_center_size(c, vec2(14.0, 14.0));
    p.rect_filled(r, 0.0, WOOD_LO);
    p.rect_filled(r.shrink(2.0), 0.0, GOLD);
    p.rect_filled(Rect::from_min_max(pos2(r.min.x + 2.0, r.max.y - 5.0), r.max - vec2(2.0, 2.0)), 0.0, GOLD_LO);
    p.rect_filled(Rect::from_min_size(r.min + vec2(4.0, 4.0), vec2(2.0, 2.0)), 0.0, Color32::from_rgb(0xff, 0xf1, 0xb0));
}

pub fn die(p: &Painter, c: Pos2) {
    let r = Rect::from_center_size(c, vec2(16.0, 16.0));
    p.rect_filled(r, 0.0, WOOD_LO);
    p.rect_filled(r.shrink(2.0), 0.0, PARCH_LT);
    for d in [vec2(-3.5, -3.5), vec2(0.0, 0.0), vec2(3.5, 3.5)] {
        p.rect_filled(Rect::from_center_size(c + d, vec2(2.0, 2.0)), 0.0, INK);
    }
}

/// Speech bubble with a stepped pixel tail, kept inside `bounds`.
pub fn bubble(p: &Painter, head: Pos2, text: &str, bounds: Rect, pop: f32) {
    let g = p.layout(text.to_owned(), FontId::proportional(13.0), INK, 160.0);
    let size = g.size() + vec2(14.0, 8.0);
    let lift = 12.0 + (1.0 - pop) * 6.0;
    let mut r = Rect::from_min_size(pos2(head.x - size.x / 2.0, head.y - size.y - lift), size);
    r = r.translate(vec2((bounds.min.x + 4.0 - r.min.x).max(0.0) + (bounds.max.x - 4.0 - r.max.x).min(0.0), (bounds.min.y + 4.0 - r.min.y).max(0.0)));
    let tip_x = head.x.clamp(r.min.x + 8.0, r.max.x - 10.0);
    p.rect_filled(r.expand(2.0), 0.0, WOOD_LO);
    p.rect_filled(r, 0.0, BUBBLE);
    for (k, w) in [(0.0, 8.0), (2.0, 6.0), (4.0, 4.0), (6.0, 2.0)] {
        p.rect_filled(Rect::from_min_size(pos2(tip_x - w / 2.0 - 2.0, r.max.y + k), vec2(w + 4.0, 2.0)), 0.0, WOOD_LO);
        p.rect_filled(Rect::from_min_size(pos2(tip_x - w / 2.0, r.max.y + k - 2.0), vec2(w, 2.0)), 0.0, BUBBLE);
    }
    p.galley(r.min + vec2(7.0, 4.0), g, INK);
}
