//! Drawing: the room with everyone in it, the popover (ribbon, room, vitals, buttons), the expanded
//! view (room cards, side panel), the desktop pet and the hatch screen.

use crate::actions::{Btn, life_line};
use crate::art::{Anim, Art, PetDraw};
use crate::game::{DAY, HitKind, Job, Placed, Slot, Species, Stage, ZONES, now, xp_needed};
use crate::look::*;
use crate::window::{ROAM, ROAM_LOG, ROAM_SCALE};
use crate::world::{Loc, TILE};
use crate::{Act, App, Fx, Tab, View, game};
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Key, Painter, PointerButton, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, TextEdit, TextFormat,
    Ui, UiBuilder, Vec2, ViewportCommand, pos2, text::LayoutJob, vec2,
};

/// Frame outline + padding around the content.
const PAD: f32 = 12.0;
/// The hatch screen's column, and the expanded view's side panel.
const COL: f32 = 416.0;
const SIDE: f32 = 290.0;
/// Item frames by rarity: common, rare, epic, legendary.
const RARITY: [Color32; 4] = [PARCH_LO, BLUE, Color32::from_rgb(0xb2, 0x7c, 0xff), GOLD];

impl App {
    // -------------------------------------------------------------------------------------- the room

    /// Draws a place around whatever the camera follows: its ground, props and everyone in it,
    /// sorted by their feet, plus name tags and speech bubbles. Clicking walks our pet there, or up
    /// to the thing clicked to use it (a door goes through).
    fn scene(&mut self, ui: &mut Ui, rect: Rect, loc: Loc, t: f64, acts: &mut Vec<Act>) {
        self.scene_rect = rect;
        let focus = self.focus(loc);
        let bodies = self.bodies(loc, t);
        let place = self.world.place(loc);
        let size = place.px();
        // A room is shown whole when it can be (at 1.5× in the popover); the town scrolls, at whole
        // screen pixels per place pixel that grow slowly with the window, so bigger shows more.
        let fit = (rect.width() / size.x).min(rect.height() / size.y);
        let s = match fit {
            f if f >= 2.0 => f.floor().min(4.0),
            f if f >= 1.5 => 1.5,
            _ => (rect.height() / 200.0).floor().max(2.0),
        };
        let whole = fit >= 1.5;
        let view = rect.size() / s;
        // a place smaller than the view sits in its middle; a bigger one scrolls with the focus
        let axis = |full: f32, seen: f32, at: f32| if full <= seen { (full - seen) / 2.0 } else { (at - seen / 2.0).clamp(0.0, full - seen) };
        let cam = vec2(axis(size.x, view.x, focus.x), axis(size.y, view.y, focus.y));
        let shake = if self.shake > 0.3 { vec2(self.rng.f32() - 0.5, self.rng.f32() - 0.5) * self.shake } else { Vec2::ZERO };
        let origin = (rect.min - cam * s).round() + shake;
        let to = |p: Pos2| origin + p.to_vec2() * s;
        let from = |p: Pos2| ((p - origin) / s).to_pos2();
        let ps = pet_scale(s);
        // the minimap, top right, where the place is bigger than the view and there's a pet out and about
        let mini = (!whole && self.save.pet.is_some() && self.fight.is_none()).then(|| {
            let [_, _, mw, mh] = place.mini;
            let k = (rect.width() * 0.26 / mw).min(rect.height() * 0.34 / mh).min(1.5);
            Rect::from_min_size(pos2(rect.max.x - mw * k - 7.0, rect.min.y + 7.0), vec2(mw, mh) * k)
        });
        let on_mini = |p: Pos2| mini.is_some_and(|m| m.expand(3.0).contains(p));
        let painter = ui.painter_at(rect);
        let [r, g, b] = place.bg;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
        self.art.ground(&painter, place, Rect::from_min_size(origin, size * s), Rect::from_min_size(Pos2::ZERO, size));
        for q in place.props.iter().filter(|q| q.low) {
            self.art.prop(&painter, q.sprite, to(Pos2::from(q.feet)), s, Color32::WHITE);
        }
        // a floor's front doors say whose they are
        if let Loc::Floor(n) = loc {
            for d in &place.doors {
                let Some(owner) = d.to.strip_prefix("apt").and_then(|i| self.resident(n, i.parse().ok()?)) else { continue };
                let name = if owner == self.save.id { "You".to_string() } else { self.pet_name(owner).unwrap_or_default() };
                let g = painter.layout_no_wrap(name, FontId::new(10.0, heavy()), PARCH);
                let r = Rect::from_center_size(to(pos2(d.centre().x, 13.0)), g.size() + vec2(8.0, 2.0));
                painter.rect_filled(r, 0.0, WOOD_LO);
                painter.galley(r.min + vec2(4.0, 1.0), g, PARCH);
            }
        }

        // props and pets in front-to-back order
        let mut order: Vec<(f32, Option<usize>, usize)> = place.props.iter().enumerate().filter(|(_, q)| !q.low).map(|(i, q)| (q.feet[1], Some(i), 0)).collect();
        order.extend(bodies.iter().enumerate().map(|(i, b)| (b.feet.y, None, i)));
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut hits = Vec::new();
        let mut tags = Vec::new();
        self.heads.clear();
        self.me_head = None;
        for (_, prop, i) in order {
            if let Some(i) = prop {
                let q = &place.props[i];
                self.art.prop(&painter, q.sprite, to(Pos2::from(q.feet)), s, Color32::WHITE);
                continue;
            }
            let b = &bodies[i];
            let feet = to(b.feet);
            if b.anim != Anim::Ghost {
                let shadow = Rect::from_center_size(feet + vec2(0.0, -1.0), vec2(20.0 * ps * b.squash.x * b.stage.size(), 4.0 * ps));
                painter.rect_filled(shadow, 8.0, Color32::from_black_alpha(70));
            }
            let head = self.art.draw_pet(
                &painter,
                &PetDraw { species: b.species, stage: b.stage, hat: b.hat, anim: b.anim, frame: b.frame, feet, scale: ps, squash: b.squash, flip: b.flip, flash: b.flash },
            );
            if let Some(name) = &b.name {
                tags.push((feet + vec2(0.0, 4.0 * s), name.clone(), self.selected == Some(b.id)));
            }
            if b.me {
                self.me_head = Some(head);
            }
            if let Some(f) = &mut self.fight
                && loc == Loc::Arena
            {
                let i = if b.me { f.me } else { 1 - f.me };
                f.heads[i] = head;
            }
            self.heads.push((b.id, head, feet, b.anim));
            hits.push((b.id, b.me, Rect::from_min_max(pos2(feet.x - 13.0 * ps, feet.y - 27.0 * ps), pos2(feet.x + 13.0 * ps, feet.y))));
        }

        // the time of day: a wash over the place (half as much indoors, where the lights are on),
        // then pools of light around lamps, doors and glowing things as it gets dark
        let (wash, dark) = daylight(game::local_hours(), loc != Loc::Town);
        painter.rect_filled(rect, 0.0, wash);
        if dark > 0.05 {
            for q in &place.props {
                if let Some([dx, dy, r, cr, cg, cb]) = q.light {
                    light(&painter, to(Pos2::from(q.feet) + vec2(dx, dy)), r * s, Color32::from_rgb(cr as u8, cg as u8, cb as u8), dark);
                }
            }
        }
        // name tags read the same at any hour
        for (at, name, sel) in tags {
            let g = painter.layout_no_wrap(name, FontId::new(11.0, heavy()), if sel { INK } else { PARCH });
            let r = Rect::from_center_size(at, g.size() + vec2(10.0, 2.0));
            painter.rect_filled(r, 0.0, if sel { GOLD } else { WOOD_LO });
            painter.galley(r.min + vec2(5.0, 1.0), g, INK);
        }

        // under the pointer: a pet, else something that does something (the front-most prop), else a
        // door; on the minimap, the spot it shows
        let resp = ui.interact(rect, ui.id().with("scene"), Sense::click_and_drag());
        let hover = resp.hover_pos();
        let body = hover.filter(|&p| !on_mini(p)).and_then(|p| hits.iter().rev().find(|h| h.2.contains(p)).map(|h| (h.0, h.1)));
        let decorating = self.deco.filter(|_| loc == Loc::Home(self.save.id));
        let playing = self.fight.is_none() && self.save.pet.is_some() && decorating.is_none();
        // decorating: the piece in hand follows the pointer, snapped to the floor, green where it
        // fits; with empty hands, furniture under the pointer can be picked up
        let mut deco: Option<(Rect, String, Act)> = None;
        if let (Some(hand), Some(p)) = (decorating, hover.filter(|&p| body.is_none() && !on_mini(p))) {
            let wp = from(p);
            match hand {
                Some(f) => {
                    let a = self.world.art(f);
                    let piece = Placed { f, x: (wp.x / TILE).floor() as i32 - (a.size[0] - 1) / 2, y: (wp.y / TILE).floor() as i32 - (a.size[1] - 1) };
                    let fits = self.world.fits(&self.save.home.placed, piece);
                    let foot = Rect::from_min_size(pos2(piece.x as f32 * TILE, piece.y as f32 * TILE), vec2(a.size[0] as f32, a.size[1] as f32) * TILE);
                    painter.rect_filled(Rect::from_min_max(to(foot.min), to(foot.max)), 0.0, if fits { GREEN } else { RED }.gamma_multiply(0.35));
                    let tint = if fits { Color32::from_rgba_unmultiplied(255, 255, 255, 215) } else { Color32::from_rgba_unmultiplied(255, 130, 130, 150) };
                    self.art.prop(&painter, a.sprite, to(pos2(foot.center().x, foot.max.y)), s, tint);
                    deco = fits.then(|| (Rect::NOTHING, String::new(), Act::Place(piece.x, piece.y)));
                }
                None => {
                    let under = place.props.iter().enumerate().filter(|(_, q)| q.hot_rect().is_some_and(|r| r.contains(wp))).max_by(|a, b| a.1.feet[1].total_cmp(&b.1.feet[1]));
                    if let (Some((i, q)), Some(piece)) = (under, under.and_then(|(i, _)| self.save.home.placed.get(i))) {
                        let r = q.hot_rect().unwrap_or(Rect::NOTHING);
                        deco = Some((Rect::from_min_max(to(r.min), to(r.max)), format!("Move the {}", piece.f.info().0.to_lowercase()), Act::PickUp(i)));
                    }
                }
            }
        }
        let hot = match (hover, body) {
            (Some(p), None) if playing && !on_mini(p) => {
                let wp = from(p);
                let prop = place.props.iter().filter_map(|q| Some((q, q.hot_rect().filter(|r| r.contains(wp))?, q.act.as_deref()?))).max_by(|a, b| a.0.feet[1].total_cmp(&b.0.feet[1]));
                match prop {
                    Some((q, r, act)) => {
                        let (label, then) = self.hot_act(act);
                        Some((Rect::from_min_max(to(r.min), to(r.max)), label, q.stand.map_or(Pos2::from(q.feet), Pos2::from), then))
                    }
                    None => place.door_at(wp).map(|d| {
                        let tile = Rect::from_center_size(d.centre(), Vec2::splat(TILE));
                        (Rect::from_min_max(to(tile.min), to(tile.max)), self.door_label(d), d.centre(), Vec::new())
                    }),
                }
            }
            _ => None,
        };
        for (r, label) in hot.iter().map(|h| (h.0, &h.1)).chain(deco.iter().map(|d| (d.0, &d.1))).filter(|(r, _)| r.is_positive()) {
            hover_chip(&painter, rect, r, label);
        }
        let mini_k = mini.map_or(1.0, |m| m.width() / size.x);
        let ground = hover
            .map(|p| match mini.filter(|_| on_mini(p)) {
                Some(m) => ((p - m.min) / mini_k).to_pos2(),
                None => from(p),
            })
            .filter(|&p| playing && place.open_at(p));
        let over_me = matches!(body, Some((_, true)));
        if over_me && !self.m.hovered {
            self.m.sv -= 3.0;
        }
        self.m.hovered = over_me;
        if body.is_some() || hot.is_some() || deco.is_some() || hover.is_some_and(on_mini) {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if resp.clicked() {
            match (body, hot, ground, deco) {
                (Some((_, true)), ..) => acts.push(Act::PetIt),
                (Some((id, false)), ..) if self.fight.is_none() && self.peers.contains_key(&id) => acts.push(Act::Select(Some(id))),
                (None, _, _, Some((.., act))) => acts.push(act),
                (None, Some((_, _, stand, then)), ..) => acts.push(Act::Walk(stand, then)),
                (None, None, Some(p), _) => acts.extend([Act::Select(None), Act::Walk(p, Vec::new())]),
                _ => acts.push(Act::Select(None)),
            }
        } else if resp.dragged()
            && let Some(p) = ground.filter(|_| t > self.m.steer_at && self.job().is_none())
        {
            // held down: the pet keeps following the pointer
            self.m.steer_at = t + 0.15;
            acts.push(Act::Walk(p, Vec::new()));
        }

        let heads: Vec<(u64, Pos2)> = self.heads.iter().map(|h| (h.0, h.1)).collect();
        for (id, head) in heads {
            if let Some((text, until)) = self.said.get(&id) {
                let pop = ((until - t) as f32).min(0.25) / 0.25;
                bubble(&painter, head, text, rect, pop);
            }
        }
        if let Some(m) = mini {
            // the whole place, the part on screen, and everyone in it
            painter.rect_filled(m.expand(3.0), 0.0, WOOD_LO);
            painter.rect_stroke(m.expand(3.0), 0.0, Stroke::new(1.0, WOOD_HI), StrokeKind::Inside);
            self.art.minimap(&painter, place, m);
            let at = |p: Pos2| m.min + p.to_vec2() * mini_k;
            if matches!(loc, Loc::Home(_)) {
                // homes are furnished by their owners, so their minimap is too
                for q in &place.props {
                    self.art.prop(&painter, q.sprite, at(Pos2::from(q.feet)), mini_k, Color32::WHITE);
                }
            }
            painter.rect_stroke(Rect::from_min_size(at(cam.to_pos2()), view * mini_k).intersect(m), 0.0, Stroke::new(1.0, PARCH_LT), StrokeKind::Inside);
            for b in bodies.iter().filter(|b| !b.me) {
                painter.circle(at(b.feet), 2.5, if self.selected == Some(b.id) { GOLD } else { PARCH_LT }, Stroke::new(1.0, INK));
            }
            if let Some(b) = bodies.iter().find(|b| b.me) {
                painter.circle(at(b.feet), 3.0, GREEN, Stroke::new(1.0, INK));
            }
        }
        if self.cut > 0.0 {
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(((self.cut * 1.6).min(1.0) * 255.0) as u8));
        }
        let fresh = std::mem::take(&mut self.fresh);
        for id in fresh {
            if let Some(&(_, _, feet, _)) = self.heads.iter().find(|h| h.0 == id) {
                self.burst(feet + vec2(0.0, -20.0), Fx::Spark(MIST), 10, 90.0);
            }
        }
    }

    fn paint_fx(&self, p: &Painter) {
        for q in &self.fx {
            let k = q.age / q.life;
            let a = 1.0 - k * k;
            let text = |s: &str, c: Color32, size: f32| {
                p.text(q.pos, Align2::CENTER_CENTER, s, FontId::proportional(size), c.gamma_multiply(a));
            };
            match q.kind {
                Fx::Heart => text("♥", PINK, q.size),
                Fx::Star => text("★", GOLD, q.size),
                Fx::Spark(c) => {
                    let (s, w) = (q.size * 0.5 * (1.0 - k * 0.5), q.size * 0.15);
                    let pts = |a: Vec2, b: Vec2| vec![q.pos - a, q.pos + b, q.pos + a, q.pos - b];
                    p.add(Shape::convex_polygon(pts(vec2(0.0, s), vec2(w, 0.0)), c.gamma_multiply(a), Stroke::NONE));
                    p.add(Shape::convex_polygon(pts(vec2(s, 0.0), vec2(0.0, -w)), c.gamma_multiply(a), Stroke::NONE));
                }
                Fx::Note => text("♪", MIST, q.size),
                Fx::Zzz => text("z", Color32::from_rgb(0xc8, 0xd0, 0xff), 10.0 + k * 10.0),
                Fx::Coin => {
                    let r = Rect::from_center_size(q.pos, Vec2::splat(q.size * 2.0));
                    p.rect_filled(r, 0.0, GOLD_LO.gamma_multiply(a));
                    p.rect_filled(r.shrink(1.5), 0.0, GOLD.gamma_multiply(a));
                }
                Fx::Drop => {
                    p.circle_filled(q.pos, 2.5, Color32::from_rgb(0x8f, 0xd8, 0xff).gamma_multiply(a));
                }
                Fx::Dust => {
                    p.circle_filled(q.pos, 2.0 + k * 6.0, Color32::from_gray(160).gamma_multiply(a * 0.5));
                }
                Fx::Confetti(c) => {
                    let w = q.size * (q.age * 12.0).sin().abs().max(0.2);
                    p.rect_filled(Rect::from_center_size(q.pos, vec2(w, q.size * 0.6)), 0.0, c.gamma_multiply(a));
                }
            }
        }
        for f in &self.floaters {
            let a = (1.0 - (f.age / 1.4).powi(3)).max(0.0);
            let size = f.size * (1.0 + 0.6 * (1.0 - (f.age * 7.0).min(1.0)));
            let font = FontId::new(size, heavy());
            p.text(f.pos + vec2(1.5, 1.5), Align2::CENTER_CENTER, &f.text, font.clone(), NIGHT.gamma_multiply(a));
            p.text(f.pos, Align2::CENTER_CENTER, &f.text, font, f.color.gamma_multiply(a));
        }
    }

    fn fight_overlay(&self, p: &Painter, scene: Rect) {
        let Some(f) = &self.fight else { return };
        for (side, i) in [(0.0, f.me), (1.0, 1 - f.me)] {
            let r = Rect::from_min_size(pos2(scene.min.x + 8.0 + side * (scene.width() / 2.0 + 20.0), scene.min.y + 8.0), vec2(scene.width() / 2.0 - 36.0, 30.0));
            p.rect_filled(r, 0.0, WOOD_LO);
            p.rect_filled(r.shrink(2.0), 0.0, PARCH);
            text1(p, r.min + vec2(7.0, 10.0), Align2::LEFT_CENTER, &format!("{}  Lv {}", f.f[i].name, f.f[i].level), FontId::new(12.0, heavy()), INK, r.width() - 14.0);
            let bar = Rect::from_min_size(r.min + vec2(7.0, 19.0), vec2(r.width() - 14.0, 6.0));
            p.rect_filled(bar, 0.0, WOOD_LO);
            let frac = (f.shown[i] / f.f[i].max_hp.max(1) as f32).clamp(0.0, 1.0);
            p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, bar.height())), 0.0, if frac < 0.3 { RED } else { GREEN });
        }
        let vs = pos2(scene.center().x, scene.min.y + 23.0);
        p.text(vs + vec2(1.5, 1.5), Align2::CENTER_CENTER, "VS", FontId::new(18.0, heavy()), NIGHT);
        p.text(vs, Align2::CENTER_CENTER, "VS", FontId::new(18.0, heavy()), GOLD);
    }

    // -------------------------------------------------------------------------------------- windows

    pub(crate) fn popover_view(&mut self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        let win = ui.max_rect();
        window_frame(ui.painter(), win);
        self.column(ui, win.shrink(PAD), t, acts);
    }

    /// The column with the side panel beside it, in a window you can drag bigger by its corner.
    pub(crate) fn expanded_view(&mut self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        let win = ui.max_rect();
        window_frame(ui.painter(), win);
        let inner = win.shrink(PAD);
        let col = Rect::from_min_max(inner.min, pos2(inner.max.x - SIDE - 10.0, inner.max.y));
        self.column(ui, col, t, acts);
        self.side(ui, Rect::from_min_max(pos2(col.max.x + 10.0, inner.min.y), inner.max), t, acts);
        let grip = Rect::from_min_max(win.max - vec2(PAD, PAD), win.max);
        let resp = ui.interact(grip, ui.id().with("grip"), Sense::drag()).on_hover_cursor(CursorIcon::ResizeNwSe);
        if resp.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(egui::viewport::ResizeDirection::SouthEast));
        }
        for d in [4.0, 8.0] {
            ui.painter().line_segment([win.max - vec2(d, 3.0), win.max - vec2(3.0, d)], Stroke::new(1.5, PARCH_LO));
        }
    }

    /// Ribbon, coin row, the place, vitals and the action buttons, filling `r`: the whole popover.
    fn column(&mut self, ui: &mut Ui, r: Rect, t: f64, acts: &mut Vec<Act>) {
        self.ribbon(ui, Rect::from_min_size(r.min, vec2(r.width(), 36.0)), acts);
        self.coin_row(ui, Rect::from_min_size(r.min + vec2(0.0, 42.0), vec2(r.width(), 24.0)), acts);
        // the pet's state sits with its scene; the buttons stand a little apart
        let scene = room_frame(ui.painter(), Rect::from_min_max(r.min + vec2(0.0, 72.0), pos2(r.max.x, r.max.y - 82.0)));
        let loc = self.view_loc();
        self.scene(ui, scene, loc, t, acts);
        if self.fight.is_some() {
            self.fight_overlay(ui.painter(), scene);
        }
        self.paint_fx(&ui.painter_at(scene));
        self.vitals(ui, Rect::from_min_size(pos2(r.min.x, r.max.y - 76.0), vec2(r.width(), 16.0)), t);
        self.action_row(ui, Rect::from_min_size(pos2(r.min.x, r.max.y - 52.0), vec2(r.width(), 52.0)), t, acts);
    }

    fn ribbon(&mut self, ui: &mut Ui, r: Rect, acts: &mut Vec<Act>) {
        self.title_plate(ui, r, acts);
        let p = ui.painter();
        let dy = -7.0 * self.kick * self.kick;
        let w = r.width() - 16.0;
        text1(p, pos2(r.center().x, r.min.y + 12.0 + dy), Align2::CENTER_CENTER, &self.place_name(self.view_loc()), FontId::new(18.0, heavy()), INK, w);
        text1(p, pos2(r.center().x, r.min.y + 25.5 + dy * 0.4), Align2::CENTER_CENTER, &self.status_line(), FontId::proportional(12.0), INK_SOFT, w);
    }

    /// The title plate doubles as the window's handle: drag to move, right-click for hide/quit.
    fn title_plate(&self, ui: &mut Ui, title: Rect, acts: &mut Vec<Act>) {
        let resp = ui.interact(title, ui.id().with("title"), Sense::click_and_drag());
        if resp.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        resp.context_menu(|ui| {
            if let Some(pet) = &self.save.pet {
                if ui.button(format!("Let {} out on the desktop", pet.name)).clicked() {
                    acts.push(Act::Out(true));
                }
                if self.tray.is_some() && ui.button("Hide to tray").clicked() {
                    acts.push(Act::Out(false));
                }
            }
            if ui.button("Quit LanPet").clicked() {
                acts.push(Act::Quit);
            }
        });
        plate(ui.painter(), title, PARCH, PARCH_LO);
    }

    /// Gold, level, XP and life stage on the left; expand/shrink and the two ways to close on the right.
    fn coin_row(&self, ui: &mut Ui, r: Rect, acts: &mut Vec<Act>) {
        let Some(pet) = &self.save.pet else { return };
        let p = ui.painter();
        coin(p, pos2(r.min.x + 9.0, r.center().y));
        let g = text1(p, pos2(r.min.x + 22.0, r.center().y), Align2::LEFT_CENTER, &pet.gold.to_string(), FontId::new(15.0, heavy()), PARCH, 80.0);
        let lv = text1(p, pos2(g.max.x + 16.0, r.center().y), Align2::LEFT_CENTER, &format!("Lv {}", pet.level), FontId::new(14.0, heavy()), PARCH, 60.0);
        let need = xp_needed(pet.level);
        let bar = Rect::from_min_size(pos2(lv.max.x + 8.0, r.center().y - 5.0), vec2(64.0, 10.0));
        p.rect_filled(bar, 0.0, WOOD_LO);
        let inner = bar.shrink(2.0);
        p.rect_filled(Rect::from_min_size(inner.min, vec2(inner.width() * (self.disp[5] / need).clamp(0.0, 1.0), inner.height())), 0.0, BLUE);
        let life = format!("{} · day {}", pet.stage().name(), pet.age / DAY + 1);
        let life = text1(p, pos2(bar.max.x + 12.0, r.center().y), Align2::LEFT_CENTER, &life, FontId::new(12.0, heavy()), PARCH, r.max.x - 110.0 - bar.max.x);
        ui.interact(bar.expand(4.0), ui.id().with("xp"), Sense::hover()).on_hover_text(format!("{:.0} / {need:.0} XP to Lv {}", pet.xp, pet.level + 1));
        ui.interact(life.expand(4.0), ui.id().with("life"), Sense::hover()).on_hover_text(format!("{}. Well-kept pets grow up stronger.", life_line(pet)));
        // the town keeps your clock: sun, dusk or moon
        let hour = game::local_hours();
        let sky = pos2(life.max.x + 14.0, r.center().y);
        if sky.x + 8.0 < r.max.x - 110.0 {
            sky_icon(p, sky, hour);
            let tip = format!("{:02}:{:02} in town", hour as u32, (hour.fract() * 60.0) as u32);
            ui.interact(Rect::from_center_size(sky, vec2(18.0, 18.0)), ui.id().with("sky"), Sense::hover()).on_hover_text(tip);
        }

        // right to left: hide to tray (when there is one), out on the desktop, expand/shrink
        let mut x = r.max.x + 4.0;
        let mut next = |ui: &mut Ui, id: &str| {
            x -= 34.0;
            button(ui, Rect::from_min_size(pos2(x, r.min.y), vec2(30.0, 24.0)), ui.id().with(id), Skin::Wood, true)
        };
        if self.tray.is_some() {
            let (resp, c) = next(ui, "hide");
            ui.painter().rect_filled(Rect::from_center_size(c.center() + vec2(0.0, 3.0), vec2(12.0, 3.0)), 0.0, PARCH);
            if resp.on_hover_text("Hide to tray").clicked() {
                acts.push(Act::Out(false));
            }
        }
        let (resp, c) = next(ui, "out");
        paw(ui.painter(), c.center(), PARCH);
        if resp.on_hover_text(format!("Let {} out on the desktop", pet.name)).clicked() {
            acts.push(Act::Out(true));
        }
        let expanded = self.view == View::Expanded;
        let (resp, c) = next(ui, "expand");
        expand_icon(ui.painter(), c.center(), expanded);
        if resp.on_hover_text(if expanded { "Shrink" } else { "Expand" }).clicked() {
            acts.push(Act::View(if expanded { View::Popover } else { View::Expanded }));
        }
    }

    /// Five bars, each with its pixel icon: HP, energy, food, water, mood. A low one pulses.
    fn vitals(&self, ui: &mut Ui, r: Rect, t: f64) {
        let Some(pet) = &self.save.pet else { return };
        let max = pet.total_max_hp();
        let rows = [
            ("HP", self.disp[0], pet.hp, max, RED),
            ("Energy", self.disp[1], pet.energy, 100.0, GOLD),
            ("Food", self.disp[2], pet.hunger, 100.0, GREEN),
            ("Water", self.disp[3], pet.thirst, 100.0, AQUA),
            ("Mood", self.disp[4], pet.mood, 100.0, PINK),
        ];
        let w = (r.width() - 4.0 * 8.0) / 5.0;
        for (i, (label, shown, real, max, c)) in rows.into_iter().enumerate() {
            let cell = Rect::from_min_size(pos2(r.min.x + i as f32 * (w + 8.0), r.min.y), vec2(w, r.height()));
            ui.interact(cell, ui.id().with(("vital", i)), Sense::hover()).on_hover_text(format!("{label} {real:.0} / {max:.0}"));
            let p = ui.painter();
            self.art.need(p, i, pos2(cell.min.x + 10.0, cell.center().y), 2.0);
            let bar = Rect::from_min_max(pos2(cell.min.x + 23.0, cell.min.y + 2.0), pos2(cell.max.x, cell.max.y - 2.0));
            p.rect_filled(bar, 0.0, WOOD_LO);
            let inner = bar.shrink(2.0);
            let fw = |v: f32| inner.width() * (v / max).clamp(0.0, 1.0);
            if real > shown + 0.5 {
                p.rect_filled(Rect::from_min_size(inner.min, vec2(fw(real), inner.height())), 0.0, PARCH_LT);
            }
            let low = real / max < 0.25;
            let a = if low { 0.55 + 0.45 * (t as f32 * 5.0).sin().abs() } else { 1.0 };
            p.rect_filled(Rect::from_min_size(inner.min, vec2(fw(shown), inner.height())), 0.0, c.gamma_multiply(a));
        }
    }

    fn action_row(&self, ui: &mut Ui, r: Rect, t: f64, acts: &mut Vec<Act>) {
        let btns = self.actions(false, t);
        let n = btns.len().max(1) as f32;
        let w = (r.width() - (n - 1.0) * 6.0) / n;
        for (i, b) in btns.iter().enumerate() {
            let br = Rect::from_min_size(pos2(r.min.x + i as f32 * (w + 6.0), r.min.y), vec2(w, r.height()));
            let (resp, c) = button(ui, br, ui.id().with(("act", i)), if b.primary { Skin::Green } else { Skin::Parch }, b.on);
            let p = ui.painter();
            let a = if b.on { 1.0 } else { 0.6 };
            let mut x = c.min.x + 8.0;
            if let Some(it) = b.item {
                self.art.item(p, it, Rect::from_min_size(pos2(x, c.center().y - 16.0), vec2(32.0, 32.0)));
                x += 38.0;
            }
            let mw = c.max.x - x - 6.0;
            text1(p, pos2(x, c.center().y - 8.0), Align2::LEFT_CENTER, &b.label, FontId::new(15.0, heavy()), INK.gamma_multiply(a), mw);
            text1(p, pos2(x, c.center().y + 9.0), Align2::LEFT_CENTER, &b.sub_now(), FontId::proportional(12.0), if b.primary { INK } else { INK_SOFT }.gamma_multiply(a), mw);
            if resp.clicked() {
                acts.extend(b.acts.iter().cloned());
            }
        }
    }

    // -------------------------------------------------------------------------------------- expanded extras

    /// The expanded view's right panel: everything to do here, the bag, and the place's chat.
    fn side(&mut self, ui: &mut Ui, r: Rect, t: f64, acts: &mut Vec<Act>) {
        plate(ui.painter(), r, PARCH, PARCH_LO);
        let inner = r.shrink(3.0);
        let tabs = [(Tab::Here, self.place_name(self.view_loc())), (Tab::Bag, "Bag".into()), (Tab::Chat, "Chat".into())];
        let tw = (inner.width() - 2.0 * 4.0 - 8.0) / 3.0;
        for (k, (tab, label)) in tabs.into_iter().enumerate() {
            let tr = Rect::from_min_size(inner.min + vec2(4.0 + k as f32 * (tw + 4.0), 4.0), vec2(tw, 28.0));
            let resp = ui.interact(tr, ui.id().with(("tab", k)), Sense::click());
            let on = self.tab == tab;
            let p = ui.painter();
            p.rect_filled(tr, 0.0, WOOD_LO);
            p.rect_filled(tr.shrink(2.0), 0.0, if on { PARCH_LT } else if resp.hovered() { WOOD } else { WOOD_HI });
            text1(p, tr.center(), Align2::CENTER_CENTER, &label, FontId::new(13.0, heavy()), if on { INK } else { PARCH }, tw - 8.0);
            if resp.has_focus() {
                p.rect_stroke(tr.expand(1.0), 0.0, Stroke::new(2.0, GOLD), StrokeKind::Outside);
            }
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                acts.push(Act::Tab(tab));
            }
        }
        let body = Rect::from_min_max(inner.min + vec2(8.0, 40.0), inner.max - vec2(8.0, 10.0));
        ui.scope_builder(UiBuilder::new().max_rect(body), |ui| match self.tab {
            Tab::Here => {
                egui::ScrollArea::vertical().id_salt("here").auto_shrink(false).show(ui, |ui| self.here_list(ui, t, acts));
            }
            Tab::Bag => {
                egui::ScrollArea::vertical().id_salt("bag").auto_shrink(false).show(ui, |ui| self.bag(ui, acts));
            }
            Tab::Chat => self.chat_tab(ui, acts),
        });
    }

    fn here_list(&self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        ui.spacing_mut().item_spacing.y = 6.0;
        let Some(pet) = &self.save.pet else { return };
        if let Some(f) = &self.fight {
            heading(ui, &format!("{} vs {}", f.f[f.me].name, f.f[1 - f.me].name));
            for h in f.hits[..f.step].iter().rev().take(6) {
                let who = &f.f[h.by].name;
                let line = match h.kind {
                    HitKind::Miss => format!("{who} missed!"),
                    HitKind::Crit => format!("{who} lands a CRIT for {}!", h.dmg),
                    HitKind::Special => format!("{who} uses {} for {}!", f.sp[h.by].special(), h.dmg),
                    HitKind::Hit => format!("{who} hits for {}.", h.dmg),
                };
                ui.label(RichText::new(line).color(if h.by == f.me { INK } else { INK_SOFT }).size(13.0));
            }
        }
        if let Some(rep) = &self.report {
            let zone = ZONES[rep.zone as usize].name;
            heading(ui, &if rep.fled { format!("Fled from {zone}") } else { format!("Back from {zone}") });
            ui.horizontal_wrapped(|ui| {
                for (foe, lvl, won) in &rep.fights {
                    ui.label(RichText::new(format!("{} {foe} Lv {lvl}", if *won { "Beat" } else { "Lost to" })).color(if *won { GREEN_LO } else { RED }).size(12.5));
                }
            });
            if !rep.loot.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for &it in &rep.loot {
                        let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                        slot(ui.painter(), r, RARITY[it.info().rarity as usize]);
                        self.art.item(ui.painter(), it, r.shrink(6.0));
                        resp.on_hover_text(it.info().name);
                    }
                });
            }
        }
        if let Some(peer) = self.selected.and_then(|id| self.peers.get(&id)) {
            let c = &peer.card;
            heading(ui, &c.name);
            ui.label(RichText::new(format!("Lv {} {} {} · {} · {}W / {}L", c.level, c.stage.name().to_lowercase(), c.species.name(), c.status, c.wins, c.losses)).color(INK_SOFT).size(12.5));
        }
        if matches!(self.view_loc(), Loc::Home(_)) && self.fight.is_none() && self.selected.is_none() {
            let f = pet.fighter(true);
            heading(ui, &format!("{} · Lv {} {} {}", pet.name, pet.level, pet.stage().name().to_lowercase(), pet.species.name()));
            ui.label(RichText::new(format!("Str {} · Mana {} · Def {} · Spd {} · {}W / {}L", f.str, f.mag, f.def, f.spd, pet.wins, pet.losses)).color(INK_SOFT).size(12.5));
            ui.label(RichText::new(life_line(pet)).color(INK_SOFT).size(12.5));
        }
        for b in self.actions(true, t) {
            row(ui, &self.art, &b, acts);
        }
    }

    fn bag(&self, ui: &mut Ui, acts: &mut Vec<Act>) {
        let Some(pet) = &self.save.pet else { return };
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        heading(ui, "Wearing");
        ui.horizontal(|ui| {
            for (s, it) in [(Slot::Weapon, pet.weapon), (Slot::Armor, pet.armor), (Slot::Charm, pet.charm), (Slot::Hat, pet.hat)] {
                let (r, resp) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
                slot(ui.painter(), r, it.map_or(PARCH_LO, |i| RARITY[i.info().rarity as usize]));
                match it {
                    Some(i) => {
                        self.art.item(ui.painter(), i, r.shrink(8.0));
                        if resp.on_hover_text(format!("{} · {}. Click to take off.", i.info().name, i.info().desc)).clicked() {
                            acts.push(Act::Unequip(s));
                        }
                    }
                    None => {
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, format!("{s:?}"), FontId::proportional(11.0), INK_SOFT);
                    }
                }
            }
        });
        heading(ui, "Bag");
        if pet.bag.is_empty() {
            ui.label(RichText::new("Empty. The Shop and the Portal fix that.").color(INK_SOFT).size(13.0));
        }
        ui.horizontal_wrapped(|ui| {
            for (&it, &n) in &pet.bag {
                let info = it.info();
                let (r, resp) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
                slot(ui.painter(), r, RARITY[info.rarity as usize]);
                self.art.item(ui.painter(), it, r.shrink(8.0));
                let g = ui.painter().layout_no_wrap(format!("{n}"), FontId::new(11.0, heavy()), PARCH);
                let chip = Rect::from_min_size(r.max - g.size() - vec2(8.0, 5.0), g.size() + vec2(5.0, 2.0));
                ui.painter().rect_filled(chip, 0.0, WOOD_LO);
                ui.painter().galley(chip.min + vec2(2.5, 1.0), g, PARCH);
                let verb = match info.slot {
                    Slot::Food => "eat",
                    Slot::Drink => "drink",
                    Slot::Hat => "wear",
                    _ => "equip",
                };
                let resp = resp.on_hover_text(format!("{} · {}\nClick to {verb}. Right-click to sell.", info.name, info.desc));
                if resp.clicked() {
                    acts.push(Act::Use(it));
                }
                resp.context_menu(|ui| {
                    if ui.button(format!("Sell for {} gold", it.sell_price())).clicked() {
                        acts.push(Act::Sell(it));
                    }
                });
            }
        });
    }

    fn chat_tab(&mut self, ui: &mut Ui, acts: &mut Vec<Act>) {
        let room = self.view_loc();
        let who = match self.peers_in(room).count() {
            0 => "nobody else here yet".to_string(),
            1 => "1 other pet here".to_string(),
            n => format!("{n} other pets here"),
        };
        ui.label(RichText::new(format!("{} chat · {who}", room.name())).color(INK_SOFT).size(12.5));
        let h = ui.available_height() - 40.0;
        egui::ScrollArea::vertical().id_salt("chat").max_height(h).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let mut any = false;
            for l in self.chat.iter().filter(|l| l.room.is_none_or(|r| r == room)) {
                any = true;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    match &l.name {
                        Some(name) => {
                            ui.label(RichText::new(name).family(heavy()).color(if l.mine { GREEN_LO } else { WOOD }).size(13.5));
                            ui.label(RichText::new(&l.text).color(INK).size(13.5));
                        }
                        None => {
                            ui.label(RichText::new(&l.text).color(INK_SOFT).size(12.5));
                        }
                    }
                });
            }
            if !any {
                ui.label(RichText::new("Pets here see what you say.").color(INK_SOFT).size(13.0));
            }
        });
        let area = ui.max_rect();
        let input = Rect::from_min_max(pos2(area.min.x, area.max.y - 32.0), pos2(area.max.x - 66.0, area.max.y));
        let r = ui.put(input, TextEdit::singleline(&mut self.chat_input).id_salt("say").hint_text("Say something").char_limit(120).margin(vec2(8.0, 7.0)));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let send = Rect::from_min_max(pos2(input.max.x + 6.0, input.min.y), area.max);
        let (resp, c) = button(ui, send, ui.id().with("send"), Skin::Green, true);
        text1(ui.painter(), c.center(), Align2::CENTER_CENTER, "Send", FontId::new(13.0, heavy()), INK, c.width());
        if (resp.clicked() || enter) && !self.chat_input.trim().is_empty() {
            acts.push(Act::Say(std::mem::take(&mut self.chat_input)));
            r.request_focus();
        }
    }

    // -------------------------------------------------------------------------------------- first run

    pub(crate) fn hatch_view(&mut self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        let win = ui.max_rect();
        window_frame(ui.painter(), win);
        let at = win.min + vec2(PAD, PAD);
        let title = Rect::from_min_size(at, vec2(COL, 36.0));
        self.title_plate(ui, title, acts);
        let (head, sub) = match &self.save.late {
            Some(old) => (format!("Goodbye, {}", old.name), format!("{} good days. The next egg inherits its gold and bag.", old.age / DAY)),
            None => ("Pick your buddy".into(), "Lives in your tray, meets coworkers' pets on the LAN".into()),
        };
        text1(ui.painter(), pos2(title.center().x, title.min.y + 12.0), Align2::CENTER_CENTER, &head, FontId::new(18.0, heavy()), INK, COL - 16.0);
        text1(ui.painter(), pos2(title.center().x, title.min.y + 25.5), Align2::CENTER_CENTER, &sub, FontId::proportional(12.0), INK_SOFT, COL - 16.0);

        let scene = room_frame(ui.painter(), Rect::from_min_size(at + vec2(0.0, 42.0), vec2(COL, 178.0)));
        self.scene(ui, scene, Loc::Home(self.save.id), t, acts);
        self.paint_fx(&ui.painter_at(scene));

        let hatching = self.hatch_at.is_some();
        let cw = (COL - 4.0 * 6.0) / 5.0;
        for (i, s) in Species::ALL.iter().enumerate() {
            let r = Rect::from_min_size(at + vec2(i as f32 * (cw + 6.0), 228.0), vec2(cw, 96.0));
            let sel = self.hatch_species == i;
            let (resp, c) = button(ui, r, ui.id().with(("species", i)), if sel { Skin::Green } else { Skin::Parch }, !hatching);
            let anim = if sel { Anim::Happy } else { Anim::Idle };
            let bob = if sel { -((t * 6.0).sin().abs() as f32) * 4.0 } else { 0.0 };
            let frame = self.art.frame(anim, t + i as f64 * 0.3);
            let pet = PetDraw { species: *s, stage: Stage::Adult, hat: None, anim, frame, feet: pos2(c.center().x, c.max.y - 22.0 + bob), scale: 2.0, squash: Vec2::splat(1.0), flip: false, flash: 0.0 };
            self.art.draw_pet(ui.painter(), &pet);
            text1(ui.painter(), pos2(c.center().x, c.max.y - 10.0), Align2::CENTER_CENTER, s.name(), FontId::new(13.0, heavy()), INK, cw);
            if resp.clicked() {
                self.hatch_species = i;
            }
        }
        let s = Species::ALL[self.hatch_species];
        text1(ui.painter(), at + vec2(0.0, 339.0), Align2::LEFT_CENTER, &format!("{}: {} Special move: {}.", s.name(), s.blurb(), s.special()), FontId::proportional(13.0), PARCH, COL);

        let row_y = at.y + 354.0;
        text1(ui.painter(), pos2(at.x, row_y + 15.0), Align2::LEFT_CENTER, "Name", FontId::new(14.0, heavy()), PARCH, 60.0);
        let field = Rect::from_min_max(pos2(at.x + 52.0, row_y), pos2(at.x + COL - 40.0, row_y + 30.0));
        ui.put(field, TextEdit::singleline(&mut self.hatch_name).id_salt("name").char_limit(16).margin(vec2(8.0, 6.0)));
        let dice = Rect::from_min_max(pos2(field.max.x + 6.0, row_y), pos2(at.x + COL, row_y + 30.0));
        let (resp, c) = button(ui, dice, ui.id().with("dice"), Skin::Wood, !hatching);
        die(ui.painter(), c.center());
        if resp.on_hover_text("Random name").clicked() {
            self.hatch_name = game::random_name(&mut self.rng);
        }
        let go = Rect::from_min_size(at + vec2(0.0, 392.0), vec2(COL, 44.0));
        let (resp, c) = button(ui, go, ui.id().with("hatch"), Skin::Green, !hatching);
        text1(ui.painter(), c.center(), Align2::CENTER_CENTER, if hatching { "Hatching…" } else { "Hatch!" }, FontId::new(19.0, heavy()), INK, COL);
        if resp.clicked() {
            acts.push(Act::Hatch);
        }
    }

    // -------------------------------------------------------------------------------------- desktop pet

    /// Just the pet, on a see-through window that `roam_step` walks along the screen, with the chat
    /// log popping up above it when there's news (all of it while hovered).
    /// Click the pet to open the popover beside it, the log to open the chat; drag to carry it somewhere else.
    pub(crate) fn roam_view(&mut self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        const SCALE: f32 = ROAM_SCALE;
        let win = ui.max_rect();
        // Window to screen. The pet is drawn where it stands on screen, not at the window's bottom
        // centre: while the OS catches up with a move or a resize the pet stays put rather than jumping.
        let to_screen = self.win_rect.min - win.min;
        let pet_box = Rect::from_min_size(self.roam.pos.round() - to_screen, ROAM);
        self.scene_rect = pet_box;
        self.heads.clear();
        self.me_head = None;
        let painter = ui.painter_at(win);
        let hit = Rect::from_min_max(pos2(pet_box.center().x - 16.0 * SCALE, pet_box.max.y - 34.0 * SCALE), pos2(pet_box.center().x + 16.0 * SCALE, pet_box.max.y));
        let resp = ui.interact(hit, ui.id().with("roam"), Sense::click_and_drag()).on_hover_cursor(CursorIcon::PointingHand);
        if resp.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            self.roam.held = true;
            self.roam.still_at = t;
        }
        if resp.clicked() {
            acts.push(Act::View(View::Popover));
        }
        let feet = pos2(pet_box.center().x, pet_box.max.y - 6.0);
        let mut top = feet.y - 34.0 * SCALE; // lowest the log may reach: above the pet, or its bubble
        let away = self.save.pet.as_ref().and_then(|p| p.task).filter(|task| matches!(task.job, Job::Explore(_)));
        if let Some(task) = away {
            // out exploring: a note where the pet was, counting down to its return
            let r = Rect::from_center_size(feet - vec2(0.0, 24.0), vec2(ROAM.x - 12.0, 40.0));
            top = r.min.y;
            plate(&painter, r, PARCH, PARCH_LO);
            let left = task.end.saturating_sub(now());
            text1(&painter, r.center() - vec2(0.0, 9.0), Align2::CENTER_CENTER, "Out exploring", FontId::new(13.0, heavy()), INK, r.width() - 10.0);
            text1(&painter, r.center() + vec2(0.0, 6.0), Align2::CENTER_CENTER, &format!("back in {}:{:02}", left / 60, left % 60), FontId::proportional(12.0), INK_SOFT, r.width() - 10.0);
        } else if let Some(b) = self.me_body(feet + vec2(0.0, self.m.hop * SCALE), t) {
            let shadow = Rect::from_center_size(feet + vec2(0.0, -1.0), vec2(20.0 * SCALE * b.squash.x * b.stage.size(), 4.0 * SCALE));
            painter.rect_filled(shadow, 12.0, Color32::from_black_alpha(70));
            let draw = PetDraw { species: b.species, stage: b.stage, hat: b.hat, anim: b.anim, frame: b.frame, feet: b.feet, scale: SCALE, squash: b.squash, flip: b.flip, flash: b.flash };
            let head = self.art.draw_pet(&painter, &draw);
            self.me_head = Some(head);
            self.heads.push((b.id, head, feet, b.anim));
            top = top.min(head.y);
            if let Some((text, until)) = self.said.get(&b.id) {
                let bounds = Rect::from_min_max(pos2(win.min.x, pet_box.min.y), pos2(win.max.x, pet_box.max.y));
                top = bubble(&painter, head, text, bounds, ((until - t) as f32).min(0.25) / 0.25).min.y;
            }
        }
        self.paint_fx(&painter);

        // the log, newest nearest the pet, as many lines as fit (only once the window has grown for it)
        let mut log = Rect::NOTHING;
        if self.roam.log {
            let mut y = top - 8.0;
            for (l, a) in self.roam_lines(t) {
                let mut job = LayoutJob::default();
                job.wrap.max_width = ROAM_LOG.x - 20.0;
                if let Some(name) = &l.name {
                    job.append(name, 0.0, TextFormat::simple(FontId::new(12.5, heavy()), if l.mine { GREEN_LO } else { WOOD }.gamma_multiply(a)));
                }
                let (size, color) = if l.name.is_some() { (12.5, INK) } else { (12.0, INK_SOFT) };
                job.append(&l.text, if l.name.is_some() { 5.0 } else { 0.0 }, TextFormat::simple(FontId::proportional(size), color.gamma_multiply(a)));
                let g = painter.layout_job(job);
                let r = Rect::from_center_size(pos2(pet_box.center().x, y - g.size().y / 2.0 - 3.0), g.size() + vec2(12.0, 6.0));
                if r.min.y < win.min.y + 2.0 {
                    break;
                }
                painter.rect_filled(r.expand(2.0), 0.0, WOOD_LO.gamma_multiply(a));
                painter.rect_filled(r, 0.0, BUBBLE.gamma_multiply(a));
                painter.galley(r.min + vec2(6.0, 3.0), g, INK);
                log = log.union(r.expand(2.0));
                y = r.min.y - 6.0;
            }
        }
        if log.is_positive() && ui.interact(log, ui.id().with("roam-log"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            acts.extend([Act::View(View::Expanded), Act::Tab(Tab::Chat)]);
        }
        let (local, moved) = ui.input(|i| (i.pointer.hover_pos(), i.events.iter().any(|e| matches!(e, egui::Event::PointerMoved(_)))));
        let was = self.m.hovered;
        self.roam_hover(local.map(|p| p + to_screen), moved, &[hit.translate(to_screen), log.translate(to_screen)], t);
        if self.m.hovered && !was {
            self.m.sv -= 3.0;
        }
    }

    /// Esc steps back (decorating → expanded → popover → closed). Ignored while typing.
    pub(crate) fn keys(&self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        if self.save.pet.is_none() || !self.open() || ctx.egui_wants_keyboard_input() || !ctx.input(|i| i.key_pressed(Key::Escape)) {
            return;
        }
        acts.push(match (self.deco, self.view) {
            (Some(Some(_)), _) => Act::PutAway,
            (Some(None), _) => Act::Decorate(false),
            (None, View::Expanded) => Act::View(View::Popover),
            _ => Act::View(View::Tray),
        });
    }
}

/// A cursor around `r` and a label chip above it (below, near the top of `scene`).
fn hover_chip(p: &Painter, scene: Rect, r: Rect, label: &str) {
    brackets(p, r, HOT);
    let g = p.layout_no_wrap(label.to_string(), FontId::new(12.0, heavy()), PARCH);
    let above = r.min.y - g.size().y - 8.0 > scene.min.y;
    let y = if above { r.min.y - 3.0 - (g.size().y + 4.0) } else { r.max.y + 3.0 };
    let x = (r.center().x - g.size().x / 2.0 - 6.0).clamp(scene.min.x + 2.0, scene.max.x - g.size().x - 14.0);
    let chip = Rect::from_min_size(pos2(x, y), g.size() + vec2(12.0, 4.0));
    p.rect_filled(chip, 0.0, WOOD_LO);
    p.galley(chip.min + vec2(6.0, 2.0), g, PARCH);
}

/// Pets are drawn at 3/4 of the world's scale, in half-pixel steps, so they stand smaller than the
/// doors and desks around them.
fn pet_scale(s: f32) -> f32 {
    (s * 1.5).round() / 2.0
}

/// The scene's wood frame filling `r`; returns the scene's rect inside it.
fn room_frame(p: &Painter, r: Rect) -> Rect {
    p.rect_filled(r, 0.0, WOOD_HI);
    p.rect_filled(r.shrink(2.0), 0.0, WOOD_LO);
    r.shrink(5.0)
}

/// A row in the expanded list: icon, label and detail, and a chip naming the action.
fn row(ui: &mut Ui, art: &Art, b: &Btn, acts: &mut Vec<Act>) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), if b.on { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let hover = b.on && resp.hovered();
    p.rect_filled(r, 0.0, if hover { WOOD_HI } else { PARCH_LO });
    p.rect_filled(r.shrink(2.0), 0.0, if hover { Color32::from_rgb(0xff, 0xf1, 0xd0) } else { PARCH_LT });
    let a = if b.on { 1.0 } else { 0.6 };
    let mut x = r.min.x + 8.0;
    if let Some(it) = b.item {
        art.item(p, it, Rect::from_min_size(pos2(x, r.center().y - 12.0), vec2(24.0, 24.0)));
        x += 30.0;
    }
    let g = p.layout_no_wrap(b.go.clone(), FontId::new(12.0, heavy()), INK);
    let chip = Rect::from_min_size(pos2(r.max.x - g.size().x - 20.0, r.center().y - 11.0), vec2(g.size().x + 12.0, 22.0));
    let (cf, cl) = if !b.on { (PARCH_LO, PARCH_LO) } else if b.primary { (GREEN, GREEN_LO) } else { (PARCH, PARCH_LO) };
    p.rect_filled(chip, 0.0, WOOD_LO);
    p.rect_filled(chip.shrink(2.0), 0.0, cf);
    p.rect_filled(Rect::from_min_max(pos2(chip.min.x + 2.0, chip.max.y - 4.0), chip.max - vec2(2.0, 2.0)), 0.0, cl);
    p.galley(chip.min + vec2(6.0, 3.0), g, INK);
    let mw = chip.min.x - x - 6.0;
    text1(p, pos2(x, r.center().y - 8.0), Align2::LEFT_CENTER, &b.label, FontId::new(13.5, heavy()), INK.gamma_multiply(a), mw);
    text1(p, pos2(x, r.center().y + 9.0), Align2::LEFT_CENTER, &b.sub_now(), FontId::proportional(12.0), INK_SOFT.gamma_multiply(a), mw);
    if b.on && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        acts.extend(b.acts.iter().cloned());
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).family(heavy()).size(14.0).color(INK));
}
