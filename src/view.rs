//! Drawing: the room with everyone in it, the popover (ribbon, room, vitals, buttons), the expanded
//! view (room cards, side panel), the desktop pet and the hatch screen.

use crate::actions::{Btn, life_line};
use crate::art::{Anim, Art, PetDraw, Room};
use crate::game::{DAY, HitKind, Item, Job, Slot, Species, Stage, ZONES, now, xp_needed};
use crate::look::*;
use crate::window::{ROAM, ROAM_SCALE};
use crate::{Act, App, Fx, PX, Peer, Tab, View, game, job_anim, job_room};
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Key, Painter, PointerButton, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, TextEdit, TextFormat,
    Ui, UiBuilder, Vec2, ViewportCommand, pos2, text::LayoutJob, vec2,
};

/// Frame outline + padding around the content.
const PAD: f32 = 12.0;
/// Main column (ribbon, room, vitals, actions), its height, and the expanded view's side panel.
const COL: f32 = 416.0;
const COL_H: f32 = 330.0;
const SIDE: f32 = 290.0;
const CARD_H: f32 = 76.0;
/// Item frames by rarity: common, rare, epic, legendary.
const RARITY: [Color32; 4] = [PARCH_LO, BLUE, Color32::from_rgb(0xb2, 0x7c, 0xff), GOLD];
/// Room card thumbnails centre on each room's signature furniture (room pixels).
const CARD_FOCUS: [f32; 8] = [150.0, 146.0, 160.0, 100.0, 150.0, 111.0, 100.0, 100.0];

impl App {
    // -------------------------------------------------------------------------------------- the room

    /// Draws a room with everyone in it, its clickable furniture, and speech bubbles.
    fn scene(&mut self, ui: &mut Ui, rect: Rect, room: Room, t: f64, acts: &mut Vec<Act>) {
        self.scene_rect = rect;
        let rs = self.art.room_size();
        let mut bodies = self.bodies(room, t);
        let shake = if self.shake > 0.3 { vec2(self.rng.f32() - 0.5, self.rng.f32() - 0.5) * self.shake } else { Vec2::ZERO };
        let origin = rect.min + shake;
        let to = |p: Pos2| origin + p.to_vec2() * PX;
        let painter = ui.painter_at(rect);
        self.art.room(&painter, room, Rect::from_min_size(origin, rs * PX), Rect::from_min_size(Pos2::ZERO, rs), Color32::WHITE);

        bodies.sort_by(|a, b| a.feet.y.total_cmp(&b.feet.y));
        let mut hits = Vec::new();
        self.heads.clear();
        self.me_head = None;
        for b in &bodies {
            let feet = to(b.feet);
            if b.anim != Anim::Ghost {
                let shadow = Rect::from_center_size(feet + vec2(0.0, -1.0), vec2(20.0 * PX * b.squash.x * b.stage.size(), 4.0 * PX));
                painter.rect_filled(shadow, 8.0, Color32::from_black_alpha(70));
            }
            let head = self.art.draw_pet(
                &painter,
                &PetDraw { species: b.species, stage: b.stage, hat: b.hat, anim: b.anim, frame: b.frame, feet, scale: PX, squash: b.squash, flip: b.flip, flash: b.flash },
            );
            if let Some(name) = &b.name {
                let sel = self.selected == Some(b.id);
                let g = painter.layout_no_wrap(name.clone(), FontId::new(11.0, heavy()), if sel { INK } else { PARCH });
                let r = Rect::from_center_size(feet + vec2(0.0, 8.0), g.size() + vec2(10.0, 2.0));
                painter.rect_filled(r, 0.0, if sel { GOLD } else { WOOD_LO });
                painter.galley(r.min + vec2(5.0, 1.0), g, INK);
            }
            if b.me {
                self.me_head = Some(head);
            }
            if let Some(f) = &mut self.fight
                && room == Room::Arena
            {
                let i = if b.me { f.me } else { 1 - f.me };
                f.heads[i] = head;
            }
            self.heads.push((b.id, head, feet, b.anim));
            hits.push((b.id, b.me, Rect::from_min_max(pos2(feet.x - 13.0 * PX, feet.y - 27.0 * PX), pos2(feet.x + 13.0 * PX, feet.y))));
        }
        let fresh = std::mem::take(&mut self.fresh);
        for id in fresh {
            if let Some(&(_, _, feet, _)) = self.heads.iter().find(|h| h.0 == id) {
                self.burst(feet + vec2(0.0, -20.0), Fx::Spark(MIST), 10, 90.0);
            }
        }

        let resp = ui.interact(rect, ui.id().with("scene"), Sense::click());
        let hover = resp.hover_pos();
        let body = hover.and_then(|p| hits.iter().rev().find(|h| h.2.contains(p)).map(|h| (h.0, h.1)));
        let hot = match (hover, body, &self.fight, &self.save.pet) {
            (Some(p), None, None, Some(_)) => self.art.hot(room).map(|(r, a)| (Rect::from_min_max(to(r.min), to(r.max)), a.to_string())).find(|(r, _)| r.contains(p)),
            _ => None,
        };
        if let Some((r, act)) = &hot {
            let (label, _) = self.hot_act(act);
            painter.rect_stroke(*r, 0.0, Stroke::new(2.0, HOT), StrokeKind::Inside);
            let g = painter.layout_no_wrap(label, FontId::new(12.0, heavy()), PARCH);
            let above = r.min.y - g.size().y - 8.0 > rect.min.y;
            let y = if above { r.min.y - 3.0 - (g.size().y + 4.0) } else { r.max.y + 3.0 };
            let x = (r.center().x - g.size().x / 2.0 - 6.0).clamp(rect.min.x + 2.0, rect.max.x - g.size().x - 14.0);
            let chip = Rect::from_min_size(pos2(x, y), g.size() + vec2(12.0, 4.0));
            painter.rect_filled(chip, 0.0, WOOD_LO);
            painter.galley(chip.min + vec2(6.0, 2.0), g, PARCH);
        }
        let over_me = matches!(body, Some((_, true)));
        if over_me && !self.m.hovered {
            self.m.sv -= 3.0;
        }
        self.m.hovered = over_me;
        if body.is_some() || hot.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if resp.clicked() {
            match (body, &hot) {
                (Some((_, true)), _) => acts.push(Act::PetIt),
                (Some((id, false)), _) if self.fight.is_none() && self.peers.contains_key(&id) => acts.push(Act::Select(Some(id))),
                (None, Some((_, act))) => acts.extend(self.hot_act(act).1),
                _ => acts.push(Act::Select(None)),
            }
        }

        let heads: Vec<(u64, Pos2)> = self.heads.iter().map(|h| (h.0, h.1)).collect();
        for (id, head) in heads {
            if let Some((text, until)) = self.said.get(&id) {
                let pop = ((until - t) as f32).min(0.25) / 0.25;
                bubble(&painter, head, text, rect, pop);
            }
        }
        if self.cut > 0.0 {
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(((self.cut * 1.6).min(1.0) * 255.0) as u8));
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
        self.column(ui, win.min + vec2(PAD, PAD), t, acts);
    }

    pub(crate) fn expanded_view(&mut self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        let win = ui.max_rect();
        window_frame(ui.painter(), win);
        let top = win.min + vec2(PAD, PAD);
        self.room_cards(ui, Rect::from_min_size(top, vec2(win.width() - 2.0 * PAD, CARD_H)), t, acts);
        let y = top.y + CARD_H + 8.0;
        self.column(ui, pos2(top.x, y), t, acts);
        self.side(ui, Rect::from_min_size(pos2(top.x + COL + 10.0, y), vec2(SIDE, COL_H)), t, acts);
    }

    /// Ribbon, coin row, the room, vitals and the action buttons: the whole popover, COL × COL_H.
    fn column(&mut self, ui: &mut Ui, at: Pos2, t: f64, acts: &mut Vec<Act>) {
        self.ribbon(ui, Rect::from_min_size(at, vec2(COL, 36.0)), acts);
        self.coin_row(ui, Rect::from_min_size(at + vec2(0.0, 42.0), vec2(COL, 24.0)), acts);
        let scene = room_frame(ui.painter(), at + vec2(0.0, 72.0));
        let room = self.view_room();
        self.scene(ui, scene, room, t, acts);
        if self.fight.is_some() {
            self.fight_overlay(ui.painter(), scene);
        }
        self.paint_fx(&ui.painter_at(scene));
        self.vitals(ui, Rect::from_min_size(at + vec2(0.0, 256.0), vec2(COL, 16.0)), t);
        self.action_row(ui, Rect::from_min_size(at + vec2(0.0, 278.0), vec2(COL, 52.0)), t, acts);
    }

    fn ribbon(&mut self, ui: &mut Ui, r: Rect, acts: &mut Vec<Act>) {
        let i = self.room as usize;
        let (prev, next) = (Room::ALL[(i + 7) % 8], Room::ALL[(i + 1) % 8]);
        let can_go = self.fight.is_none();
        for (dir, room, x) in [(-1.0, prev, r.min.x), (1.0, next, r.max.x - 36.0)] {
            let br = Rect::from_min_size(pos2(x, r.min.y), vec2(36.0, 36.0));
            let (resp, c) = button(ui, br, ui.id().with(("arrow", dir as i32)), Skin::Wood, can_go);
            arrow(ui.painter(), c.center(), dir, if can_go { PARCH } else { PARCH.gamma_multiply(0.5) });
            if resp.on_hover_text(format!("Go to the {}", room.name())).clicked() {
                acts.push(Act::Go(room));
            }
        }
        let title = Rect::from_min_max(pos2(r.min.x + 42.0, r.min.y), pos2(r.max.x - 42.0, r.max.y));
        self.title_plate(ui, title, acts);
        let p = ui.painter();
        let dy = -7.0 * self.kick * self.kick;
        let w = title.width() - 16.0;
        text1(p, pos2(title.center().x, title.min.y + 12.0 + dy), Align2::CENTER_CENTER, self.view_room().name(), FontId::new(18.0, heavy()), INK, w);
        text1(p, pos2(title.center().x, title.min.y + 25.5 + dy * 0.4), Align2::CENTER_CENTER, &self.status_line(), FontId::proportional(12.0), INK_SOFT, w);
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
            self.art.need(p, i, pos2(cell.min.x + 10.0, cell.center().y), PX);
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

    /// The expanded view's row of rooms: a live peek into each one, who's in it, and its name.
    fn room_cards(&self, ui: &mut Ui, r: Rect, t: f64, acts: &mut Vec<Act>) {
        let (cw, gap) = (84.0, 6.0);
        let x0 = r.min.x + ((r.width() - (8.0 * cw + 7.0 * gap)) / 2.0).floor();
        let here = self.pet_room();
        let task = self.save.pet.as_ref().and_then(|p| p.task);
        for (i, &room) in Room::ALL.iter().enumerate() {
            let card = Rect::from_min_size(pos2(x0 + i as f32 * (cw + gap), r.min.y), vec2(cw, CARD_H));
            let resp = ui.interact(card, ui.id().with(("room", i)), Sense::click());
            let cur = room == self.view_room();
            let p = ui.painter();
            p.rect_filled(card, 0.0, if cur { GOLD } else if resp.hovered() { WOOD_HI } else { WOOD_LO });
            let thumb = Rect::from_min_size(card.min + vec2(3.0, 3.0), vec2(cw - 6.0, 46.0));
            let sx = (CARD_FOCUS[i] - thumb.width() / 2.0).clamp(0.0, 200.0 - thumb.width());
            let tint = if cur || resp.hovered() { Color32::WHITE } else { Color32::from_gray(205) };
            self.art.room(p, room, thumb, Rect::from_min_size(pos2(sx, 30.0), thumb.size()), tint);
            // who's in there, drawn at 1x standing on the thumbnail's floor
            let mut who: Vec<(Species, Stage, Option<Item>, Anim)> = Vec::new();
            if here == Some(room)
                && let Some(pet) = &self.save.pet
            {
                who.push((pet.species, pet.stage(), pet.hat, self.my_anim(t)));
            }
            let peers: Vec<&Peer> = self.peers_in(room).collect();
            who.extend(peers.iter().take(2).map(|p| (p.card.species, p.card.stage, p.card.hat, p.card.job.map_or(Anim::Idle, job_anim))));
            let n = who.len() as f32;
            let tp = ui.painter_at(thumb);
            for (k, (species, stage, hat, anim)) in who.into_iter().enumerate() {
                let x = thumb.center().x + (k as f32 - (n - 1.0) / 2.0) * 22.0;
                let frame = self.art.frame(anim, t + k as f64 * 0.4);
                self.art.draw_pet(&tp, &PetDraw { species, stage, hat, anim, frame, feet: pos2(x, thumb.max.y - 1.0), scale: 1.0, squash: Vec2::splat(1.0), flip: k > 0, flash: 0.0 });
            }
            if !peers.is_empty() {
                let chip = Rect::from_min_size(pos2(thumb.max.x - 17.0, thumb.min.y + 2.0), vec2(15.0, 15.0));
                p.rect_filled(chip, 0.0, WOOD_LO);
                p.rect_filled(chip.shrink(2.0), 0.0, GREEN);
                p.text(chip.center(), Align2::CENTER_CENTER, peers.len().to_string(), FontId::new(10.5, heavy()), INK);
            }
            if let Some(task) = task.filter(|tk| job_room(tk.job) == room) {
                let frac = 1.0 - task.end.saturating_sub(now()) as f32 / task.job.secs() as f32;
                p.rect_filled(Rect::from_min_size(pos2(thumb.min.x, thumb.max.y - 3.0), vec2(thumb.width() * frac.clamp(0.0, 1.0), 3.0)), 0.0, GOLD);
            }
            let name = Rect::from_min_max(pos2(card.min.x + 3.0, thumb.max.y + 3.0), card.max - vec2(3.0, 3.0));
            p.rect_filled(name, 0.0, if cur { PARCH } else { PARCH_LO });
            text1(p, name.center(), Align2::CENTER_CENTER, room.name(), FontId::new(13.0, heavy()), INK, name.width() - 4.0);
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                acts.push(Act::Go(room));
            }
        }
    }

    /// The expanded view's right panel: this room's full list, the bag, and room chat.
    fn side(&mut self, ui: &mut Ui, r: Rect, t: f64, acts: &mut Vec<Act>) {
        plate(ui.painter(), r, PARCH, PARCH_LO);
        let inner = r.shrink(3.0);
        let tabs = [(Tab::Here, self.view_room().name()), (Tab::Bag, "Bag"), (Tab::Chat, "Chat")];
        let tw = (inner.width() - 2.0 * 4.0 - 8.0) / 3.0;
        for (k, (tab, label)) in tabs.into_iter().enumerate() {
            let tr = Rect::from_min_size(inner.min + vec2(4.0 + k as f32 * (tw + 4.0), 4.0), vec2(tw, 28.0));
            let resp = ui.interact(tr, ui.id().with(("tab", k)), Sense::click());
            let on = self.tab == tab;
            let p = ui.painter();
            p.rect_filled(tr, 0.0, WOOD_LO);
            p.rect_filled(tr.shrink(2.0), 0.0, if on { PARCH_LT } else if resp.hovered() { WOOD } else { WOOD_HI });
            text1(p, tr.center(), Align2::CENTER_CENTER, label, FontId::new(13.0, heavy()), if on { INK } else { PARCH }, tw - 8.0);
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
        if self.room == Room::Home && self.fight.is_none() && self.selected.is_none() {
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
        let room = self.view_room();
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
                ui.label(RichText::new("Pets in this room see what you say here.").color(INK_SOFT).size(13.0));
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

        let scene = room_frame(ui.painter(), at + vec2(0.0, 42.0));
        self.scene(ui, scene, Room::Home, t, acts);
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
        // the pet's own box sits at the bottom centre; above it, when grown, room for the log
        let pet_box = Rect::from_center_size(pos2(win.center().x, win.max.y - ROAM.y / 2.0), ROAM);
        self.scene_rect = pet_box;
        self.heads.clear();
        self.me_head = None;
        let painter = ui.painter_at(win);
        let hit = Rect::from_min_max(pos2(win.center().x - 16.0 * SCALE, win.max.y - 34.0 * SCALE), pos2(win.center().x + 16.0 * SCALE, win.max.y));
        let resp = ui.interact(hit, ui.id().with("roam"), Sense::click_and_drag()).on_hover_cursor(CursorIcon::PointingHand);
        if resp.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            self.roam.held = true;
            self.roam.still_at = t;
        }
        if resp.clicked() {
            acts.push(Act::View(View::Popover));
        }
        let feet = pos2(win.center().x, win.max.y - 6.0);
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
                let bounds = Rect::from_min_max(pos2(win.min.x, pet_box.min.y), win.max);
                top = bubble(&painter, head, text, bounds, ((until - t) as f32).min(0.25) / 0.25).min.y;
            }
        }
        self.paint_fx(&painter);

        // the log, newest nearest the pet, as many lines as fit (only once the window has grown for it)
        let mut log = Rect::NOTHING;
        if win.height() > ROAM.y + 1.0 {
            let mut y = top - 8.0;
            for l in self.roam_lines(t) {
                let a = self.roam_fade(l, t);
                let mut job = LayoutJob::default();
                job.wrap.max_width = win.width() - 20.0;
                if let Some(name) = &l.name {
                    job.append(name, 0.0, TextFormat::simple(FontId::new(12.5, heavy()), if l.mine { GREEN_LO } else { WOOD }.gamma_multiply(a)));
                }
                let (size, color) = if l.name.is_some() { (12.5, INK) } else { (12.0, INK_SOFT) };
                job.append(&l.text, if l.name.is_some() { 5.0 } else { 0.0 }, TextFormat::simple(FontId::proportional(size), color.gamma_multiply(a)));
                let g = painter.layout_job(job);
                let r = Rect::from_center_size(pos2(win.center().x, y - g.size().y / 2.0 - 3.0), g.size() + vec2(12.0, 6.0));
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
        // hovering the log keeps it open, so it can be read
        let hovered = resp.hovered() || ui.rect_contains_pointer(log);
        if resp.hovered() && !self.m.hovered {
            self.m.sv -= 3.0;
        }
        self.m.hovered = hovered;
    }

    /// ← → change room, Esc steps back (expanded → popover → closed). Ignored while typing.
    pub(crate) fn keys(&self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        if self.save.pet.is_none() || !self.open() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let i = self.room as usize;
        ctx.input(|inp| {
            if self.fight.is_none() && inp.key_pressed(Key::ArrowLeft) {
                acts.push(Act::Go(Room::ALL[(i + 7) % 8]));
            }
            if self.fight.is_none() && inp.key_pressed(Key::ArrowRight) {
                acts.push(Act::Go(Room::ALL[(i + 1) % 8]));
            }
            if inp.key_pressed(Key::Escape) {
                acts.push(Act::View(if self.view == View::Expanded { View::Popover } else { View::Tray }));
            }
        });
    }
}

/// The room's wood frame (410×178 at `at`, inset 3px in the column); returns the 400×168 room rect.
fn room_frame(p: &Painter, at: Pos2) -> Rect {
    let frame = Rect::from_min_size(at + vec2(3.0, 0.0), vec2(410.0, 178.0));
    p.rect_filled(frame, 0.0, WOOD_HI);
    p.rect_filled(frame.shrink(2.0), 0.0, WOOD_LO);
    frame.shrink(5.0)
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
