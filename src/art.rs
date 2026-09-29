//! Pixel art made by `tools/gen_assets.py`, embedded in the binary and drawn with nearest filtering.

use crate::game::{Item, Species};
use eframe::egui::{Color32, ColorImage, Context, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2};
use serde::Deserialize;
use std::collections::HashMap;

/// Rows of every species sheet, in `STATES` order from the generator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anim {
    Idle,
    Walk,
    Run,
    Sleep,
    Eat,
    Happy,
    Train,
    Study,
    Attack,
    Hurt,
    Sad,
    Blink,
    Egg,
}
const ANIM_NAMES: [&str; 13] = ["idle", "walk", "run", "sleep", "eat", "happy", "train", "study", "attack", "hurt", "sad", "blink", "egg"];

/// Rooms, in `ROOMS` order from the generator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Room {
    Home,
    Bedroom,
    Kitchen,
    Library,
    Gym,
    Portal,
    Arena,
    Shop,
}

impl Room {
    pub const ALL: [Room; 8] = [Room::Home, Room::Bedroom, Room::Kitchen, Room::Library, Room::Gym, Room::Portal, Room::Arena, Room::Shop];
    pub fn name(self) -> &'static str {
        ["Home", "Bedroom", "Kitchen", "Library", "Gym", "Portal", "Arena", "Shop"][self as usize]
    }
}

#[derive(Deserialize)]
struct StateMeta {
    name: String,
    frames: usize,
    fps: f64,
}

#[derive(Deserialize)]
struct RoomMeta {
    spot: [f32; 2],
    spot2: Option<[f32; 2]>,
    slots: Vec<[f32; 2]>,
}

#[derive(Deserialize)]
struct Meta {
    frame: f32,
    states: Vec<StateMeta>,
    head: HashMap<String, HashMap<String, Vec<[f32; 2]>>>,
    hats: Vec<Item>,
    hat_size: [f32; 2],
    hat_anchor: [f32; 2],
    items: Vec<Item>,
    item_size: f32,
    room_size: [f32; 2],
    rooms: Vec<RoomMeta>,
}

pub struct Art {
    meta: Meta,
    pets: Vec<(TextureHandle, TextureHandle)>, // (sprite, white silhouette) per species
    hats: TextureHandle,
    items: TextureHandle,
    rooms: TextureHandle,
}

pub struct PetDraw {
    pub species: Species,
    pub hat: Option<Item>,
    pub anim: Anim,
    pub frame: usize,
    /// Bottom-centre of the sprite, in screen space.
    pub feet: Pos2,
    pub scale: f32,
    /// Squash & stretch multipliers.
    pub squash: Vec2,
    pub flip: bool,
    pub flash: f32,
}

fn texture(ctx: &Context, name: &str, png: &[u8], white: bool) -> TextureHandle {
    let img = image::load_from_memory(png).expect("embedded asset is a valid png").to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let mut px = img.into_raw();
    if white {
        for p in px.chunks_mut(4) {
            p[..3].fill(255);
        }
    }
    ctx.load_texture(name, ColorImage::from_rgba_unmultiplied(size, &px), TextureOptions::NEAREST)
}

fn uv(tex: &TextureHandle, px: Rect) -> Rect {
    let s = tex.size_vec2();
    Rect::from_min_max(pos2(px.min.x / s.x, px.min.y / s.y), pos2(px.max.x / s.x, px.max.y / s.y))
}

impl Art {
    pub fn load(ctx: &Context) -> Art {
        let meta: Meta = serde_json::from_str(include_str!("../assets/meta.json")).expect("assets/meta.json matches art.rs");
        assert!(meta.states.iter().map(|s| s.name.as_str()).eq(ANIM_NAMES), "regenerate assets: animation rows changed");
        let sheets: [&[u8]; 5] = [
            include_bytes!("../assets/dino.png"),
            include_bytes!("../assets/lizard.png"),
            include_bytes!("../assets/monkey.png"),
            include_bytes!("../assets/frog.png"),
            include_bytes!("../assets/wolf.png"),
        ];
        let pets = Species::ALL
            .iter()
            .zip(sheets)
            .map(|(s, png)| (texture(ctx, s.key(), png, false), texture(ctx, s.key(), png, true)))
            .collect();
        Art {
            pets,
            hats: texture(ctx, "hats", include_bytes!("../assets/hats.png"), false),
            items: texture(ctx, "items", include_bytes!("../assets/items.png"), false),
            rooms: texture(ctx, "rooms", include_bytes!("../assets/rooms.png"), false),
            meta,
        }
    }

    pub fn frame(&self, anim: Anim, t: f64) -> usize {
        let s = &self.meta.states[anim as usize];
        (t * s.fps) as usize % s.frames
    }

    /// Draws the pet; returns the screen position of the top of its head (for bubbles & particles).
    pub fn draw_pet(&self, p: &Painter, d: &PetDraw) -> Pos2 {
        let f = self.meta.frame;
        let (sx, sy) = (d.scale * d.squash.x, d.scale * d.squash.y);
        let rect = Rect::from_min_size(pos2(d.feet.x - f * sx / 2.0, d.feet.y - f * sy), vec2(f * sx, f * sy));
        let (tex, white) = &self.pets[d.species as usize];
        let src = Rect::from_min_size(pos2(d.frame as f32 * f, d.anim as usize as f32 * f), vec2(f, f));
        let mut suv = uv(tex, src);
        if d.flip {
            std::mem::swap(&mut suv.min.x, &mut suv.max.x);
        }
        p.image(tex.id(), rect, suv, Color32::WHITE);
        if d.flash > 0.0 {
            p.image(white.id(), rect, suv, Color32::WHITE.gamma_multiply(d.flash.min(1.0)));
        }
        let head = self.meta.head[d.species.key()][ANIM_NAMES[d.anim as usize]]
            .get(d.frame)
            .copied()
            .unwrap_or([16.0, 5.0]);
        let hx = if d.flip { f - head[0] } else { head[0] };
        let head_pos = pos2(rect.min.x + hx * sx, rect.min.y + head[1] * sy);
        if let (Some(hat), true) = (d.hat, d.anim != Anim::Egg) {
            if let Some(i) = self.meta.hats.iter().position(|h| *h == hat) {
                let [hw, hh] = self.meta.hat_size;
                let [ax, ay] = self.meta.hat_anchor;
                let r = Rect::from_min_size(pos2(head_pos.x - ax * sx, head_pos.y - ay * sy), vec2(hw * sx, hh * sy));
                let mut huv = uv(&self.hats, Rect::from_min_size(pos2(i as f32 * hw, 0.0), vec2(hw, hh)));
                if d.flip {
                    std::mem::swap(&mut huv.min.x, &mut huv.max.x);
                }
                p.image(self.hats.id(), r, huv, Color32::WHITE);
                return pos2(head_pos.x, r.min.y + 6.0 * sy);
            }
        }
        head_pos
    }

    /// Item icon (hats use their worn sprite) fitted into `rect`.
    pub fn item(&self, p: &Painter, it: Item, rect: Rect) {
        if let Some(i) = self.meta.items.iter().position(|x| *x == it) {
            let s = self.meta.item_size;
            let cols = (self.items.size()[0] as f32 / s) as usize;
            let src = Rect::from_min_size(pos2((i % cols) as f32 * s, (i / cols) as f32 * s), vec2(s, s));
            p.image(self.items.id(), rect, uv(&self.items, src), Color32::WHITE);
        } else if let Some(i) = self.meta.hats.iter().position(|x| *x == it) {
            let [hw, hh] = self.meta.hat_size;
            let src = Rect::from_min_size(pos2(i as f32 * hw, 0.0), vec2(hw, hh - 8.0));
            let r = Rect::from_center_size(rect.center(), vec2(rect.width(), rect.width() * (hh - 8.0) / hw));
            p.image(self.hats.id(), r, uv(&self.hats, src), Color32::WHITE);
        }
    }

    pub fn room_size(&self) -> Vec2 {
        Vec2::from(self.meta.room_size)
    }

    /// Paint the part `src` (room pixels) of a room into `rect`.
    pub fn room(&self, p: &Painter, room: Room, rect: Rect, src: Rect, tint: Color32) {
        let src = src.translate(vec2(0.0, room as usize as f32 * self.meta.room_size[1]));
        p.image(self.rooms.id(), rect, uv(&self.rooms, src), tint);
    }

    /// Where the room's activity happens (room pixels). `second` picks the alternate spot (treadmill, arena foe).
    pub fn spot(&self, room: Room, second: bool) -> Pos2 {
        let m = &self.meta.rooms[room as usize];
        let s = if second { m.spot2.unwrap_or(m.spot) } else { m.spot };
        pos2(s[0], s[1])
    }

    /// Shared-room slot positions (room pixels).
    pub fn slots(&self, room: Room) -> impl Iterator<Item = Pos2> + '_ {
        self.meta.rooms[room as usize].slots.iter().map(|s| pos2(s[0], s[1]))
    }
}

/// Deterministic slot assignment so every LAN client lays out a shared room identically:
/// pets that want a furniture slot (bed, treadmill…) claim it first, lowest id wins; the rest
/// fill free slots in id order. Returns the slot per pet (same order as input); None = room full.
pub fn assign_slots(pets: &[(u64, Option<usize>)], n: usize) -> Vec<Option<usize>> {
    let mut order: Vec<usize> = (0..pets.len()).collect();
    order.sort_by_key(|&i| pets[i].0);
    let mut taken = vec![false; n];
    let mut out = vec![None; pets.len()];
    for &i in &order {
        if let Some(p) = pets[i].1.filter(|&p| p < n && !taken[p]) {
            taken[p] = true;
            out[i] = Some(p);
        }
    }
    for &i in &order {
        if out[i].is_none() {
            out[i] = taken.iter().position(|t| !t);
            if let Some(s) = out[i] {
                taken[s] = true;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn slots_are_order_independent_and_respect_furniture() {
        let a = [(9, None), (3, Some(1)), (5, Some(1)), (1, None)];
        let mut b = a;
        b.reverse();
        let sa = super::assign_slots(&a, 3);
        let sb = super::assign_slots(&b, 3);
        for (i, p) in a.iter().enumerate() {
            let j = b.iter().position(|q| q == p).unwrap();
            assert_eq!(sa[i], sb[j]);
        }
        assert_eq!(sa, vec![None, Some(1), Some(2), Some(0)]); // id 3 gets the treadmill, id 9 overflows
    }
}
