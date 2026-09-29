#![cfg_attr(windows, windows_subsystem = "windows")]
//! LanPet: a tiny pixel pet that lives on your desktop and hangs out with coworkers' pets over the LAN.

mod art;
mod game;
mod net;

use art::{Anim, Art, PetDraw, Room, assign_slots};
use eframe::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, FontId, Frame, Key, Layout, Painter, PointerButton, Pos2,
    Rect, RichText, Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, UiBuilder, Vec2, ViewportCommand, pos2, vec2,
};
use game::{Event, Fighter, Hit, HitKind, Item, Job, Report, Rng, Save, Slot, Species, ZONES, clean, now, xp_needed};
use net::{Card, Msg, Net};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

const PANEL: Vec2 = Vec2::new(400.0, 700.0);
const COMPACT: Vec2 = Vec2::new(220.0, 176.0);
/// Screen pixels per room/sprite pixel.
const PX: f32 = 2.0;
const FLOOR_Y: f32 = 74.0;

const BG: Color32 = Color32::from_rgb(0x17, 0x16, 0x1d);
const CARD: Color32 = Color32::from_rgb(0x22, 0x20, 0x2a);
const CARD2: Color32 = Color32::from_rgb(0x2e, 0x2b, 0x38);
const WELL: Color32 = Color32::from_rgb(0x12, 0x11, 0x17);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe4, 0xf0);
const DIM: Color32 = Color32::from_rgb(0x9a, 0x95, 0xa8);
const ACCENT: Color32 = Color32::from_rgb(0x8b, 0x7b, 0xff);
const ACCENT_DIM: Color32 = Color32::from_rgb(0x4a, 0x40, 0x8c);
const GOLD: Color32 = Color32::from_rgb(0xff, 0xcc, 0x4d);
const RED: Color32 = Color32::from_rgb(0xff, 0x5d, 0x73);
const GREEN: Color32 = Color32::from_rgb(0x7e, 0xd9, 0x57);
const PINK: Color32 = Color32::from_rgb(0xff, 0x8f, 0xc7);
const BLUE: Color32 = Color32::from_rgb(0x7a, 0xa2, 0xff);
const INK: Color32 = Color32::from_rgb(0x1b, 0x16, 0x26);

fn main() -> eframe::Result {
    let path = Save::path();
    let save = Save::load(&path);
    let size = if save.pet.is_some() { COMPACT } else { PANEL };
    let mut vp = egui::ViewportBuilder::default()
        .with_title("LanPet")
        .with_app_id("lanpet")
        .with_inner_size(size)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false);
    if let Some([x, y]) = save.pos {
        vp = vp.with_position(pos2(x, y) - size);
    }
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native("lanpet", options, Box::new(move |cc| Ok(Box::new(App::new(cc, save, path)))))
}

// ------------------------------------------------------------------------------------------ state

struct Peer {
    card: Card,
    addr: SocketAddr,
    seen: f64,
}

struct Pending {
    battle: u64,
    peer: u64,
    card: Card,
    until: f64,
}

struct Incoming {
    battle: u64,
    card: Card,
    addr: SocketAddr,
    until: f64,
}

struct ChatLine {
    room: Room,
    name: String,
    text: String,
    mine: bool,
}

struct Fight {
    f: [Fighter; 2],
    sp: [Species; 2],
    hats: [Option<Item>; 2],
    /// Which fighter is us (challenger is always 0 so both sides simulate identically).
    me: usize,
    hits: Vec<Hit>,
    winner: usize,
    step: usize,
    next: f64,
    hp: [i32; 2],
    shown: [f32; 2],
    lunge: [f32; 2],
    anim: [(Anim, f64); 2],
    flash: [f32; 2],
    heads: [Pos2; 2],
    reward: String,
    over: bool,
}

#[derive(Clone, Copy)]
enum Fx {
    Heart,
    Star,
    Coin,
    Zzz,
    Drop,
    Dust,
    Confetti(Color32),
    Spark(Color32),
    Note,
}

struct Particle {
    pos: Pos2,
    vel: Vec2,
    age: f32,
    life: f32,
    kind: Fx,
    size: f32,
    grav: f32,
}

struct Floater {
    pos: Pos2,
    text: String,
    color: Color32,
    age: f32,
    size: f32,
}

/// Our pet's on-screen body (room pixels).
struct Motion {
    x: f32,
    y: f32,
    hop: f32,
    vy: f32,
    squash: f32,
    sv: f32,
    flip: bool,
    moving: bool,
    target_x: f32,
    wander_at: f64,
    hop_at: f64,
    blink_at: f64,
    blink_until: f64,
    react: Anim,
    react_until: f64,
    flash: f32,
    chatter_at: f64,
    hovered: bool,
}

struct Body {
    id: u64,
    species: Species,
    hat: Option<Item>,
    anim: Anim,
    frame: usize,
    feet: Pos2,
    squash: Vec2,
    flash: f32,
    flip: bool,
    name: Option<String>,
    me: bool,
}

enum Act {
    Start(Job),
    Stop,
    Use(Item),
    Sell(Item),
    Buy(Item),
    Unequip(Slot),
    Go(Room),
    Select(Option<u64>),
    Challenge(u64),
    Wave(u64),
    Gift(u64, Item),
    Accept,
    Decline,
    Panel(bool),
    Quit,
    CloseReport,
    CloseFight,
    Hatch,
    PetIt,
    Say(String),
}

struct App {
    art: Art,
    save: Save,
    path: PathBuf,
    rng: Rng,
    net: Option<Net>,
    net_err: Option<String>,
    peers: BTreeMap<u64, Peer>,
    fresh: Vec<u64>,
    panel: bool,
    want_panel: Option<bool>,
    room: Room,
    selected: Option<u64>,
    m: Motion,
    fx: Vec<Particle>,
    floaters: Vec<Floater>,
    said: HashMap<u64, (String, f64)>,
    shake: f32,
    disp: [f32; 5],
    heads: Vec<(u64, Pos2, Pos2, Anim)>,
    me_head: Option<Pos2>,
    scene_rect: Rect,
    queued: Vec<Event>,
    pending: Option<Pending>,
    incoming: Option<Incoming>,
    fight: Option<Fight>,
    report: Option<Report>,
    chat: Vec<ChatLine>,
    chat_input: String,
    last_chat: f64,
    hatch_species: usize,
    hatch_name: String,
    hatch_at: Option<f64>,
    last_save: f64,
    last_hello: f64,
    last_pet: f64,
    last_wave: f64,
    win_br: Option<Pos2>,
    placed: bool,
    dirty: bool,
}

fn job_room(j: Job) -> Room {
    match j {
        Job::Study => Room::Library,
        Job::Lift | Job::Run => Room::Gym,
        Job::Sleep => Room::Bedroom,
        Job::Explore(_) => Room::Portal,
    }
}

/// Furniture slot a job wants in a shared room (see gen_assets.py SLOTS).
fn job_slot(j: Job) -> Option<usize> {
    match j {
        Job::Sleep | Job::Study | Job::Lift => Some(0),
        Job::Run => Some(1),
        Job::Explore(_) => None,
    }
}

fn job_anim(j: Job) -> Anim {
    match j {
        Job::Study => Anim::Study,
        Job::Lift => Anim::Train,
        Job::Run => Anim::Run,
        Job::Sleep => Anim::Sleep,
        Job::Explore(_) => Anim::Idle,
    }
}

fn mmss(s: u64) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

const RARITY: [Color32; 4] = [
    Color32::from_rgb(0x3a, 0x36, 0x45),
    Color32::from_rgb(0x2c, 0x4a, 0x70),
    Color32::from_rgb(0x55, 0x36, 0x7a),
    Color32::from_rgb(0x7a, 0x58, 0x1c),
];

const IDLE_LINES: [&str; 8] = ["♪ la la la", "What are we working on?", "Pet me!", "*stretches*", "You got this!", "Snack break soon?", "Boop!", "Stay hydrated!"];
const HUNGRY_LINES: [&str; 3] = ["I'm hungry...", "Food? Food!", "*tummy rumbles*"];
const TIRED_LINES: [&str; 3] = ["*yawn*", "So sleepy...", "Nap time?"];
const JOB_LINES: [(Job, [&str; 3]); 4] = [
    (Job::Study, ["Hmm, fascinating...", "Taking notes!", "Big brain time."]),
    (Job::Lift, ["One more rep!", "Feel the burn!", "GAINS!"]),
    (Job::Run, ["Zoom zoom!", "Cardio!", "Can't catch me!"]),
    (Job::Sleep, ["zzz...", "*snore*", "mmm... snacks..."]),
];

impl App {
    fn new(cc: &eframe::CreationContext<'_>, save: Save, path: PathBuf) -> Self {
        let ctx = cc.egui_ctx.clone();
        style(&ctx);
        let wake = {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        };
        let (net, net_err) = match Net::start(wake) {
            Ok(n) => (Some(n), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let mut rng = Rng::seeded();
        let hatch_name = game::random_name(&mut rng);
        App {
            art: Art::load(&ctx),
            panel: save.pet.is_none(),
            save,
            path,
            rng,
            net,
            net_err,
            peers: BTreeMap::new(),
            fresh: Vec::new(),
            want_panel: None,
            room: Room::Home,
            selected: None,
            m: Motion {
                x: 100.0,
                y: FLOOR_Y,
                hop: 0.0,
                vy: 0.0,
                squash: 0.0,
                sv: 0.0,
                flip: false,
                moving: false,
                target_x: 100.0,
                wander_at: 2.0,
                hop_at: 6.0,
                blink_at: 1.0,
                blink_until: 0.0,
                react: Anim::Happy,
                react_until: 0.0,
                flash: 0.0,
                chatter_at: 20.0,
                hovered: false,
            },
            fx: Vec::new(),
            floaters: Vec::new(),
            said: HashMap::new(),
            shake: 0.0,
            disp: [0.0; 5],
            heads: Vec::new(),
            me_head: None,
            scene_rect: Rect::NOTHING,
            queued: Vec::new(),
            pending: None,
            incoming: None,
            fight: None,
            report: None,
            chat: Vec::new(),
            chat_input: String::new(),
            last_chat: -1.0,
            hatch_species: 0,
            hatch_name,
            hatch_at: None,
            last_save: 0.0,
            last_hello: -10.0,
            last_pet: -10.0,
            last_wave: -10.0,
            win_br: None,
            placed: false,
            dirty: false,
        }
    }

    // -------------------------------------------------------------------------------------- where things are

    /// Room our pet is standing in; None while it's away on an expedition (or not hatched).
    fn pet_room(&self) -> Option<Room> {
        let p = self.save.pet.as_ref()?;
        if self.fight.is_some() {
            return Some(Room::Arena);
        }
        match p.task.map(|t| t.job) {
            Some(Job::Explore(_)) => None,
            Some(j) => Some(job_room(j)),
            None => Some(self.room),
        }
    }

    /// Room currently on screen.
    fn view_room(&self) -> Room {
        if self.fight.is_some() {
            Room::Arena
        } else if self.panel || self.save.pet.is_none() {
            self.room
        } else {
            self.pet_room().unwrap_or(Room::Portal)
        }
    }

    /// Shared-room layout: our slot (only when others are here) and every visible peer's slot.
    fn layout(&self, room: Room) -> (Option<Pos2>, Vec<(u64, Pos2)>) {
        let mut list: Vec<(u64, Option<usize>)> = self
            .peers
            .values()
            .filter(|p| p.card.room == Some(room as u8))
            .map(|p| (p.card.id, p.card.job.filter(|&j| job_room(j) == room).and_then(job_slot)))
            .collect();
        if list.is_empty() {
            return (None, Vec::new());
        }
        if self.pet_room() == Some(room) && self.fight.is_none() {
            let job = self.save.pet.as_ref().and_then(|p| p.task).map(|t| t.job);
            list.push((self.save.id, job.filter(|&j| job_room(j) == room).and_then(job_slot)));
        }
        let slots: Vec<Pos2> = self.art.slots(room).collect();
        let mut me = None;
        let mut out = Vec::new();
        for (&(id, _), s) in list.iter().zip(assign_slots(&list, slots.len())) {
            if let Some(s) = s {
                if id == self.save.id { me = Some(slots[s]) } else { out.push((id, slots[s])) }
            }
        }
        (me, out)
    }

    fn my_anim(&self, t: f64) -> Anim {
        let Some(p) = &self.save.pet else { return Anim::Egg };
        if t < self.m.react_until {
            return self.m.react;
        }
        let job = p.task.map(|t| t.job);
        if self.m.moving {
            return if job == Some(Job::Run) { Anim::Run } else { Anim::Walk };
        }
        if let Some(j) = job {
            return job_anim(j);
        }
        if p.hunger < 15.0 || p.mood < 20.0 || p.hp < p.total_max_hp() * 0.2 {
            Anim::Sad
        } else if t < self.m.blink_until {
            Anim::Blink
        } else {
            Anim::Idle
        }
    }

    fn bodies(&self, room: Room, t: f64) -> Vec<Body> {
        let mut v = Vec::new();
        if let (Some(f), Room::Arena) = (&self.fight, room) {
            for i in 0..2 {
                let left = i == f.me;
                let anim = if t < f.anim[i].1 {
                    f.anim[i].0
                } else if f.over {
                    if f.winner == i { Anim::Happy } else { Anim::Sad }
                } else {
                    Anim::Idle
                };
                let dir = if left { 1.0 } else { -1.0 };
                v.push(Body {
                    id: if left { self.save.id } else { u64::MAX - i as u64 },
                    species: f.sp[i],
                    hat: f.hats[i],
                    anim,
                    frame: self.art.frame(anim, t + i as f64 * 0.37),
                    feet: self.art.spot(Room::Arena, !left) + vec2(dir * f.lunge[i] * 24.0, 0.0),
                    squash: Vec2::splat(1.0),
                    flash: f.flash[i],
                    flip: !left,
                    name: None,
                    me: left,
                });
            }
            return v;
        }
        let Some(pet) = &self.save.pet else {
            if room == Room::Home {
                let (frame, shake) = match self.hatch_at {
                    Some(s) => (1 + (((t - s) / 0.5) as usize).min(2), ((t - s) * 3.0) as f32),
                    None => ((t * 1.2) as usize % 2, 0.0),
                };
                let wob = (t as f32 * 40.0).sin() * shake;
                v.push(Body {
                    id: 0,
                    species: Species::ALL[self.hatch_species],
                    hat: None,
                    anim: Anim::Egg,
                    frame,
                    feet: pos2(100.0 + wob, FLOOR_Y),
                    squash: Vec2::splat(1.0),
                    flash: 0.0,
                    flip: false,
                    name: None,
                    me: false,
                });
            }
            return v;
        };
        if self.pet_room() == Some(room) {
            let anim = self.my_anim(t);
            v.push(Body {
                id: self.save.id,
                species: pet.species,
                hat: pet.hat,
                anim,
                frame: self.art.frame(anim, t),
                feet: pos2(self.m.x, self.m.y + self.m.hop),
                squash: vec2(1.0 + self.m.squash * 0.25, 1.0 - self.m.squash * 0.25),
                flash: self.m.flash,
                flip: self.m.flip,
                name: None,
                me: true,
            });
        }
        for (id, feet) in self.layout(room).1 {
            let Some(p) = self.peers.get(&id) else { continue };
            let phase = (id % 997) as f64 * 0.37;
            let anim = p.card.job.map_or(if (t + phase) % 4.0 < 0.13 { Anim::Blink } else { Anim::Idle }, job_anim);
            v.push(Body {
                id,
                species: p.card.species,
                hat: p.card.hat,
                anim,
                frame: self.art.frame(anim, t + phase),
                feet,
                squash: Vec2::splat(1.0),
                flash: 0.0,
                flip: feet.x > 100.0,
                name: Some(p.card.name.clone()),
                me: false,
            });
        }
        v
    }

    fn card(&self) -> Option<Card> {
        let p = self.save.pet.as_ref()?;
        Some(Card {
            id: self.save.id,
            name: p.name.clone(),
            species: p.species,
            level: p.level,
            hat: p.hat,
            status: p.status(),
            wins: p.wins,
            losses: p.losses,
            fighter: p.fighter(true),
            room: self.pet_room().map(|r| r as u8),
            job: p.task.map(|t| t.job),
        })
    }

    // -------------------------------------------------------------------------------------- juice helpers

    fn anchor(&self) -> Pos2 {
        self.me_head.unwrap_or(self.scene_rect.center())
    }

    fn burst(&mut self, at: Pos2, kind: Fx, n: usize, speed: f32) {
        for _ in 0..n {
            let a = self.rng.f32() * std::f32::consts::TAU;
            let s = speed * (0.4 + self.rng.f32() * 0.8);
            let (size, grav, life) = match kind {
                Fx::Confetti(_) => (4.0 + self.rng.f32() * 3.0, 260.0, 1.2),
                Fx::Coin => (4.0, 380.0, 0.9),
                Fx::Heart => (12.0 + self.rng.f32() * 6.0, -30.0, 1.0),
                Fx::Star | Fx::Spark(_) => (10.0 + self.rng.f32() * 6.0, 60.0, 0.9),
                _ => (8.0, 0.0, 1.0),
            };
            self.fx.push(Particle { pos: at, vel: vec2(a.cos() * s, a.sin() * s - speed * 0.4), age: 0.0, life, kind, size, grav });
        }
    }

    fn confetti(&mut self, at: Pos2, n: usize) {
        for i in 0..n {
            let c = [GOLD, PINK, BLUE, GREEN, ACCENT, RED][i % 6];
            self.burst(at, Fx::Confetti(c), 1, 170.0);
        }
    }

    fn float(&mut self, at: Pos2, text: impl Into<String>, color: Color32, size: f32) {
        let jitter = (self.rng.f32() - 0.5) * 16.0;
        self.floaters.push(Floater { pos: at + vec2(jitter, -8.0), text: text.into(), color, age: 0.0, size });
    }

    fn say(&mut self, id: u64, text: impl Into<String>, t: f64, secs: f64) {
        self.said.insert(id, (text.into(), t + secs));
    }

    fn say_me(&mut self, text: impl Into<String>, t: f64) {
        let id = self.save.id;
        self.say(id, text, t, 3.5);
    }

    fn react(&mut self, anim: Anim, secs: f64, t: f64) {
        self.m.react = anim;
        self.m.react_until = t + secs;
    }

    fn hop(&mut self) {
        if self.m.hop == 0.0 {
            self.m.vy = -70.0;
            self.m.sv -= 5.0;
        }
    }

    // -------------------------------------------------------------------------------------- per-frame logic

    fn place(&mut self, ctx: &egui::Context) {
        let (outer, monitor) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().monitor_size));
        if let Some(r) = outer {
            self.win_br = Some(r.max);
        }
        if self.placed {
            return;
        }
        if self.save.pos.is_some() {
            self.placed = true;
        } else if let Some(m) = monitor {
            let size = if self.panel { PANEL } else { COMPACT };
            let pos = (m - size - vec2(24.0, 64.0)).max(Vec2::ZERO).to_pos2();
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
            self.win_br = Some(pos + size);
            self.placed = true;
        }
    }

    fn set_panel(&mut self, ctx: &egui::Context, on: bool) {
        if self.panel == on {
            return;
        }
        if on {
            self.room = self.pet_room().unwrap_or(self.room);
        }
        self.panel = on;
        let size = if on { PANEL } else { COMPACT };
        if let Some(br) = self.win_br {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition((br - size).max(Pos2::ZERO)));
        }
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        self.fx.clear();
        self.floaters.clear();
    }

    fn persist(&mut self) {
        if let Some(br) = self.win_br {
            self.save.pos = Some([br.x, br.y]);
        }
        if let Err(e) = self.save.store(&self.path) {
            eprintln!("lanpet: saving failed: {e}");
        }
        self.dirty = false;
    }

    fn poll_net(&mut self, t: f64) {
        let Some(net) = &self.net else { return };
        let msgs: Vec<_> = net.rx.try_iter().collect();
        for (from, m) in msgs {
            self.on_msg(from, m, t);
        }
    }

    fn send(&self, to: SocketAddr, m: &Msg) {
        if let Some(n) = &self.net {
            n.send(to, m);
        }
    }

    fn on_msg(&mut self, from: SocketAddr, m: Msg, t: f64) {
        match m {
            Msg::Hello { card } => {
                if card.id == self.save.id || (!self.peers.contains_key(&card.id) && self.peers.len() >= 64) {
                    return;
                }
                let before = self.peers.get(&card.id).and_then(|p| p.card.room);
                if card.room != before && card.room == Some(self.view_room() as u8) {
                    self.fresh.push(card.id);
                }
                self.peers.insert(card.id, Peer { card, addr: from, seen: t });
            }
            Msg::Challenge { battle, card } => {
                let away = self.pet_room().is_none();
                if self.fight.is_some() || self.incoming.is_some() || self.pending.is_some() || away {
                    let why = if away { "is out adventuring" } else { "is busy right now" };
                    self.send(from, &Msg::Decline { battle, why: why.into() });
                    return;
                }
                self.say_me(format!("⚔ {} wants to battle!", card.name), t);
                self.react(Anim::Happy, 1.0, t);
                self.hop();
                self.incoming = Some(Incoming { battle, card, addr: from, until: t + 20.0 });
            }
            Msg::Accept { battle, card } => {
                if let Some(p) = self.pending.take_if(|p| p.battle == battle && p.peer == card.id) {
                    self.start_fight([p.card, card], 0, battle, t);
                }
            }
            Msg::Decline { battle, why } => {
                if let Some(p) = self.pending.take_if(|p| p.battle == battle) {
                    let name = self.peers.get(&p.peer).map_or("They".to_string(), |x| x.card.name.clone());
                    self.say_me(format!("{name} {why}."), t);
                    self.react(Anim::Sad, 1.2, t);
                }
            }
            Msg::Wave { from } => {
                self.say_me(format!("👋 {from} says hi!"), t);
                self.react(Anim::Happy, 1.2, t);
                self.hop();
                let a = self.anchor();
                self.burst(a, Fx::Heart, 5, 60.0);
            }
            Msg::Gift { from, item } => {
                if let Some(p) = &mut self.save.pet {
                    p.give(item, 1);
                    self.dirty = true;
                    self.say_me(format!("🎁 {from} sent me a {}!", item.info().name), t);
                    self.react(Anim::Happy, 1.5, t);
                    let a = self.anchor();
                    self.confetti(a, 24);
                }
            }
            Msg::Chat { id, name, room, text } => {
                if id == self.save.id {
                    return;
                }
                self.chat.push(ChatLine { room: Room::ALL[room as usize], name, text: text.clone(), mine: false });
                if self.chat.len() > 100 {
                    self.chat.remove(0);
                }
                self.say(id, text, t, 6.0);
            }
        }
    }

    fn start_fight(&mut self, cards: [Card; 2], me: usize, seed: u64, t: f64) {
        let b = game::battle(&cards[0].fighter, &cards[1].fighter, seed);
        let foe_level = cards[1 - me].level;
        let reward = match &mut self.save.pet {
            Some(p) => {
                let (xp, gold) = p.pvp_reward(b.winner == me, foe_level, &mut self.queued);
                self.dirty = true;
                if gold > 0 { format!("+{xp} XP   +{gold} gold") } else { format!("+{xp} XP") }
            }
            None => String::new(),
        };
        let [a, c] = cards;
        self.fight = Some(Fight {
            hp: [a.fighter.hp, c.fighter.hp],
            shown: [a.fighter.hp as f32, c.fighter.hp as f32],
            sp: [a.species, c.species],
            hats: [a.hat, c.hat],
            f: [a.fighter, c.fighter],
            me,
            hits: b.hits,
            winner: b.winner,
            step: 0,
            next: t + 1.2,
            lunge: [0.0; 2],
            anim: [(Anim::Idle, 0.0); 2],
            flash: [0.0; 2],
            heads: [Pos2::ZERO; 2],
            reward,
            over: false,
        });
        self.incoming = None;
        self.pending = None;
        self.want_panel = Some(true);
        self.last_hello = -10.0;
    }

    fn simulate(&mut self, t: f64) {
        let mut ev = std::mem::take(&mut self.queued);
        if let Some(p) = &mut self.save.pet {
            p.tick(now(), &mut ev);
        }
        for e in ev {
            let a = self.anchor();
            self.dirty = true;
            match e {
                Event::LevelUp(l) => {
                    self.float(a + vec2(0.0, -10.0), format!("LEVEL {l}!"), GOLD, 22.0);
                    self.burst(a, Fx::Star, 14, 150.0);
                    self.react(Anim::Happy, 1.6, t);
                    self.say_me("I feel stronger!", t);
                    self.m.flash = 1.0;
                    self.hop();
                }
                Event::Done(job) => {
                    let gain = match job {
                        Job::Study => "+3 MANA",
                        Job::Lift => "+3 STR",
                        Job::Run => "+8 HP",
                        _ => "Rested!",
                    };
                    self.float(a, gain, GREEN, 16.0);
                    self.burst(a, Fx::Spark(GOLD), 8, 90.0);
                    self.react(Anim::Happy, 1.2, t);
                    self.say_me(if job == Job::Sleep { "Good morning!" } else { "Done! That was fun." }, t);
                }
                Event::Back(rep) => {
                    self.say_me(if rep.fled { "Ouch... I ran away." } else { "I'm back! Check my loot!" }, t);
                    self.react(if rep.fled { Anim::Sad } else { Anim::Happy }, 2.0, t);
                    self.m.x = self.art.spot(Room::Portal, false).x + 30.0;
                    self.m.flip = true;
                    self.report = Some(rep);
                    self.confetti(a, 20);
                }
            }
        }
    }

    fn housekeeping(&mut self, ctx: &egui::Context, t: f64) {
        if let Some(on) = self.want_panel.take() {
            self.set_panel(ctx, on);
        }
        if t - self.last_hello > 2.0 {
            self.last_hello = t;
            if let (Some(n), Some(card)) = (&self.net, self.card()) {
                n.broadcast(&Msg::Hello { card });
            }
        }
        self.peers.retain(|_, p| t - p.seen < 8.0);
        if self.pending.as_ref().is_some_and(|p| t > p.until) {
            self.pending = None;
            self.say_me("No answer...", t);
        }
        if self.incoming.as_ref().is_some_and(|i| t > i.until) {
            self.incoming = None;
        }
        self.said.retain(|_, (_, until)| t < *until);
        if self.selected.is_some_and(|id| !self.peers.contains_key(&id)) {
            self.selected = None;
        }
        if (self.dirty && t - self.last_save > 2.0) || t - self.last_save > 20.0 {
            self.last_save = t;
            if self.save.pet.is_some() {
                self.persist();
            }
        }
    }

    fn animate(&mut self, dt: f32, t: f64) {
        // hatching
        if let Some(start) = self.hatch_at {
            self.shake = self.shake.max(((t - start) * 2.0) as f32);
            if t - start > 1.5 {
                self.hatch_at = None;
                let name = match clean(&self.hatch_name, 16) {
                    n if n.is_empty() => game::random_name(&mut self.rng),
                    n => n,
                };
                self.save.pet = Some(game::Pet::new(name.clone(), Species::ALL[self.hatch_species], now()));
                self.m.x = 100.0;
                self.m.target_x = 100.0;
                let a = self.anchor();
                self.confetti(a, 40);
                self.burst(a, Fx::Star, 12, 160.0);
                self.react(Anim::Happy, 2.5, t);
                self.say_me(format!("Hi! I'm {name}!"), t);
                self.persist();
            }
        }

        // our pet's body
        let job = self.save.pet.as_ref().and_then(|p| p.task).map(|t| t.job);
        if let Some(room) = self.pet_room().filter(|_| self.fight.is_none()) {
            let slot = self.layout(room).0;
            let target = match (slot, job) {
                (Some(s), _) => s,
                (None, Some(Job::Run)) => self.art.spot(Room::Gym, true),
                (None, Some(j)) => self.art.spot(job_room(j), false),
                (None, None) => {
                    if t > self.m.wander_at {
                        self.m.wander_at = t + 3.0 + self.rng.f32() as f64 * 6.0;
                        if self.rng.chance(60) {
                            self.m.target_x = 24.0 + self.rng.f32() * 152.0;
                        }
                    }
                    pos2(self.m.target_x, FLOOR_Y)
                }
            };
            let dx = target.x - self.m.x;
            self.m.moving = dx.abs() > 0.5 && t >= self.m.react_until;
            if self.m.moving {
                self.m.x += dx.signum() * (30.0 * dt).min(dx.abs());
                self.m.flip = dx < 0.0;
            }
            if (target.y - self.m.y).abs() > 0.1 && dx.abs() < 3.0 {
                if target.y < self.m.y - 2.0 {
                    self.hop();
                }
                self.m.y = target.y;
            } else if dx.abs() >= 3.0 {
                self.m.y = FLOOR_Y;
            }
            let happy = self.save.pet.as_ref().is_some_and(|p| p.mood > 60.0);
            if job.is_none() && !self.m.moving && happy && t > self.m.hop_at {
                self.m.hop_at = t + 5.0 + self.rng.f32() as f64 * 8.0;
                self.hop();
            }
        }
        if self.m.hop < 0.0 || self.m.vy < 0.0 {
            self.m.vy += 420.0 * dt;
            self.m.hop += self.m.vy * dt;
            if self.m.hop >= 0.0 {
                self.m.hop = 0.0;
                self.m.vy = 0.0;
                self.m.sv += 6.0;
            }
        }
        self.m.sv += (-140.0 * self.m.squash - 9.0 * self.m.sv) * dt;
        self.m.squash += self.m.sv * dt;
        self.m.flash = (self.m.flash - dt * 3.0).max(0.0);
        if t > self.m.blink_at {
            self.m.blink_until = t + 0.13;
            self.m.blink_at = t + 2.0 + self.rng.f32() as f64 * 3.5;
        }
        if t > self.m.chatter_at {
            self.m.chatter_at = t + 35.0 + self.rng.f32() as f64 * 50.0;
            if let Some(p) = &self.save.pet {
                let line = if p.hunger < 25.0 {
                    *self.rng.pick(&HUNGRY_LINES)
                } else if p.energy < 20.0 {
                    *self.rng.pick(&TIRED_LINES)
                } else if let Some((_, l)) = JOB_LINES.iter().find(|(j, _)| Some(*j) == job) {
                    *self.rng.pick(l)
                } else {
                    *self.rng.pick(&IDLE_LINES)
                };
                self.say_me(line, t);
            }
        }

        // ambient particles for everyone on screen
        let heads = std::mem::take(&mut self.heads);
        for &(_, head, feet, anim) in &heads {
            let r = self.rng.f32();
            let side = if self.rng.chance(50) { 1.0 } else { -1.0 };
            let (kind, pos, vel, rate) = match anim {
                Anim::Sleep => (Fx::Zzz, head + vec2(10.0, 0.0), vec2(8.0, -16.0), 0.7),
                Anim::Study => (Fx::Spark(Color32::from_rgb(0xe6, 0xf0, 0xa0)), head + vec2(side * 14.0, 4.0), vec2(0.0, -20.0), 0.8),
                Anim::Train => (Fx::Drop, head + vec2(side * 14.0, 6.0), vec2(side * 40.0, -50.0), 1.2),
                Anim::Run => (Fx::Dust, feet + vec2(-side * 10.0, -2.0), vec2(-side * 20.0, -8.0), 5.0),
                Anim::Happy => (Fx::Note, head + vec2(side * 16.0, 0.0), vec2(side * 10.0, -24.0), 0.8),
                _ => continue,
            };
            if r < dt * rate {
                let grav = if matches!(kind, Fx::Drop) { 300.0 } else { 0.0 };
                self.fx.push(Particle { pos, vel, age: 0.0, life: 1.4, kind, size: 12.0, grav });
            }
        }
        self.heads = heads;

        for p in &mut self.fx {
            p.age += dt;
            p.vel.y += p.grav * dt;
            p.pos += p.vel * dt;
        }
        self.fx.retain(|p| p.age < p.life);
        for f in &mut self.floaters {
            f.age += dt;
            f.pos.y -= 26.0 * dt;
        }
        self.floaters.retain(|f| f.age < 1.4);
        self.shake *= (-dt * 9.0).exp();

        if let Some(p) = &self.save.pet {
            let target = [p.hp, p.energy, p.hunger, p.mood, p.xp];
            let k = 1.0 - (-dt * 6.0).exp();
            for (d, v) in self.disp.iter_mut().zip(target) {
                *d += (v - *d) * k;
            }
        }
        self.fight_step(dt, t);
    }

    fn fight_step(&mut self, dt: f32, t: f64) {
        let Some(f) = &mut self.fight else { return };
        for i in 0..2 {
            f.lunge[i] *= (-dt * 7.0).exp();
            f.flash[i] = (f.flash[i] - dt * 4.0).max(0.0);
            f.shown[i] += (f.hp[i] as f32 - f.shown[i]) * (1.0 - (-dt * 8.0).exp());
        }
        if f.over || t < f.next {
            return;
        }
        let Some(&h) = f.hits.get(f.step) else {
            f.over = true;
            let won = f.winner == f.me;
            let at = f.heads[f.winner];
            self.float(at + vec2(0.0, -16.0), if won { "VICTORY!" } else { "DEFEAT" }, if won { GOLD } else { RED }, 26.0);
            if won {
                self.confetti(at, 50);
            }
            return;
        };
        f.step += 1;
        f.next = t + 0.8;
        let (a, d) = (h.by, 1 - h.by);
        f.lunge[a] = 1.0;
        f.anim[a] = (Anim::Attack, t + 0.4);
        let at = f.heads[d];
        let special = f.sp[a].special();
        match h.kind {
            HitKind::Miss => {
                f.anim[d] = (Anim::Happy, t + 0.4);
                self.float(at, "MISS", DIM, 16.0);
            }
            kind => {
                f.hp[d] = h.hp;
                f.flash[d] = 1.0;
                f.anim[d] = (Anim::Hurt, t + 0.45);
                let crit = kind == HitKind::Crit;
                self.shake = if crit { 9.0 } else { 4.0 };
                if kind == HitKind::Special {
                    let id = if a == f.me { self.save.id } else { u64::MAX - a as u64 };
                    self.say(id, format!("{special}!"), t, 1.0);
                    self.burst(at, Fx::Spark(ACCENT), 10, 120.0);
                }
                self.float(at, if crit { format!("CRIT -{}", h.dmg) } else { format!("-{}", h.dmg) }, if crit { GOLD } else { RED }, if crit { 22.0 } else { 17.0 });
                self.burst(at, Fx::Star, if crit { 8 } else { 4 }, 110.0);
            }
        }
    }

    // -------------------------------------------------------------------------------------- actions

    fn apply(&mut self, ctx: &egui::Context, act: Act, t: f64) {
        let a = self.anchor();
        match act {
            Act::Panel(on) => self.set_panel(ctx, on),
            Act::Quit => {
                self.persist();
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Act::Go(r) => {
                if r != self.room {
                    let followed = self.pet_room() == Some(self.room);
                    self.room = r;
                    self.selected = None;
                    if followed && self.pet_room() == Some(r) {
                        self.m.x = -14.0;
                        self.m.y = FLOOR_Y;
                        self.m.target_x = 50.0 + self.rng.f32() * 90.0;
                        self.m.wander_at = t + 3.0;
                    }
                    self.last_hello = -10.0;
                }
            }
            Act::Select(id) => self.selected = id,
            Act::PetIt => {
                self.burst(a, Fx::Heart, 4, 70.0);
                self.react(Anim::Happy, 0.9, t);
                self.hop();
                if t - self.last_pet > 1.5 {
                    self.last_pet = t;
                    if let Some(p) = &mut self.save.pet {
                        p.pet();
                        self.dirty = true;
                    }
                    if self.rng.chance(30) {
                        let line = *self.rng.pick(&["Hehe!", "More pets!", "♥♥♥", "That tickles!"]);
                        self.say_me(line, t);
                    }
                }
            }
            Act::Start(job) => {
                let seed = self.rng.next();
                match self.save.pet.as_mut().map(|p| p.start(job, now(), seed)) {
                    Some(Ok(())) => {
                        self.dirty = true;
                        self.last_hello = -10.0;
                        let line = match job {
                            Job::Study => "Time to hit the books!",
                            Job::Lift => "Let's get those gains!",
                            Job::Run => "Zoom zoom!",
                            Job::Sleep => "Goodnight...",
                            Job::Explore(_) => "Adventure time!",
                        };
                        if let Job::Explore(z) = job {
                            self.burst(a, Fx::Spark(ACCENT), 16, 120.0);
                            self.float(a, format!("Off to {}!", ZONES[z as usize].name), ACCENT, 16.0);
                        } else {
                            self.say_me(line, t);
                            self.burst(a, Fx::Spark(GOLD), 6, 80.0);
                        }
                    }
                    Some(Err(e)) => {
                        self.say_me(e, t);
                        self.react(Anim::Sad, 1.0, t);
                        self.m.sv += 4.0;
                    }
                    None => {}
                }
            }
            Act::Stop => {
                if let Some(p) = &mut self.save.pet {
                    p.stop();
                    self.dirty = true;
                    self.last_hello = -10.0;
                }
                self.say_me("Break time!", t);
            }
            Act::Use(it) => match self.save.pet.as_mut().map(|p| p.use_item(it)) {
                Some(Ok(())) => {
                    self.dirty = true;
                    let info = it.info();
                    if matches!(info.slot, Slot::Food | Slot::Drink) {
                        self.react(Anim::Eat, 1.4, t);
                        self.say_me(if info.slot == Slot::Food { "Nom nom!" } else { "Glug glug!" }, t);
                        for (v, label, c) in [(info.fx.hunger, "Food", GREEN), (info.fx.hp.min(999.0), "HP", RED), (info.fx.energy, "Energy", GOLD), (info.fx.mood, "Mood", PINK)] {
                            if v > 0.0 {
                                self.float(a, format!("+{v:.0} {label}"), c, 15.0);
                            }
                        }
                        self.burst(a + vec2(0.0, 30.0), Fx::Confetti(Color32::from_rgb(0xc8, 0x90, 0x4c)), 6, 60.0);
                    } else {
                        self.say_me(format!("{} equipped!", info.name), t);
                        self.burst(a, Fx::Spark(GOLD), 10, 100.0);
                        self.react(Anim::Happy, 1.0, t);
                        self.m.flash = 0.8;
                    }
                }
                Some(Err(e)) => self.say_me(e, t),
                None => {}
            },
            Act::Buy(it) => match self.save.pet.as_mut().map(|p| p.buy(it)) {
                Some(Ok(())) => {
                    self.dirty = true;
                    self.float(a, format!("-{} gold", it.info().price), GOLD, 15.0);
                    self.burst(a, Fx::Coin, 8, 110.0);
                    self.say_me(format!("Ooh, {}!", it.info().name), t);
                    self.react(Anim::Happy, 0.8, t);
                }
                Some(Err(e)) => {
                    self.say_me(e, t);
                    self.react(Anim::Sad, 0.8, t);
                }
                None => {}
            },
            Act::Sell(it) => {
                if let Some(Ok(g)) = self.save.pet.as_mut().map(|p| p.sell(it)) {
                    self.dirty = true;
                    self.float(a, format!("+{g} gold"), GOLD, 15.0);
                    self.burst(a, Fx::Coin, 6, 100.0);
                }
            }
            Act::Unequip(slot) => {
                if let Some(p) = &mut self.save.pet {
                    p.unequip(slot);
                    self.dirty = true;
                }
            }
            Act::Challenge(id) => {
                let (Some(peer), Some(card)) = (self.peers.get(&id), self.card()) else { return };
                if self.pending.is_some() || self.fight.is_some() {
                    return;
                }
                let battle = self.rng.next();
                let (addr, name) = (peer.addr, peer.card.name.clone());
                self.send(addr, &Msg::Challenge { battle, card: card.clone() });
                self.pending = Some(Pending { battle, peer: id, card, until: t + 20.0 });
                self.say_me(format!("Hey {name}, fight me!"), t);
                self.react(Anim::Attack, 0.6, t);
            }
            Act::Wave(id) => {
                if t - self.last_wave < 2.0 {
                    return;
                }
                self.last_wave = t;
                let (Some(peer), Some(pet)) = (self.peers.get(&id), &self.save.pet) else { return };
                let (addr, name) = (peer.addr, peer.card.name.clone());
                self.send(addr, &Msg::Wave { from: pet.name.clone() });
                self.say_me(format!("👋 Hi {name}!"), t);
                self.react(Anim::Happy, 1.0, t);
                self.hop();
            }
            Act::Gift(id, it) => {
                let Some(addr) = self.peers.get(&id).map(|p| p.addr) else { return };
                let Some(pet) = &mut self.save.pet else { return };
                if pet.take(it) {
                    let from = pet.name.clone();
                    self.send(addr, &Msg::Gift { from, item: it });
                    self.dirty = true;
                    self.say_me(format!("Sent a {}! 🎁", it.info().name), t);
                    self.burst(a, Fx::Heart, 5, 70.0);
                }
            }
            Act::Accept => {
                let (Some(inc), Some(card)) = (self.incoming.take(), self.card()) else { return };
                self.send(inc.addr, &Msg::Accept { battle: inc.battle, card: card.clone() });
                self.start_fight([inc.card, card], 1, inc.battle, t);
            }
            Act::Decline => {
                if let Some(inc) = self.incoming.take() {
                    self.send(inc.addr, &Msg::Decline { battle: inc.battle, why: "said not right now".into() });
                }
            }
            Act::CloseReport => self.report = None,
            Act::CloseFight => {
                self.fight = None;
                self.last_hello = -10.0;
            }
            Act::Hatch => {
                if self.hatch_at.is_none() {
                    self.hatch_at = Some(t);
                }
            }
            Act::Say(text) => {
                let text = clean(&text, 120);
                let Some(pet) = &self.save.pet else { return };
                if text.is_empty() || t - self.last_chat < 0.5 {
                    return;
                }
                self.last_chat = t;
                let room = self.view_room();
                let name = pet.name.clone();
                if let Some(n) = &self.net {
                    n.broadcast(&Msg::Chat { id: self.save.id, name: name.clone(), room: room as u8, text: text.clone() });
                }
                self.chat.push(ChatLine { room, name, text: text.clone(), mine: true });
                if self.chat.len() > 100 {
                    self.chat.remove(0);
                }
                let id = self.save.id;
                self.say(id, text, t, 6.0);
            }
        }
    }

    // -------------------------------------------------------------------------------------- drawing

    /// Draws a room with everyone in it. `compact` crops around our pet (desktop widget).
    fn scene(&mut self, ui: &mut Ui, rect: Rect, room: Room, compact: bool, t: f64, acts: &mut Vec<Act>) {
        self.scene_rect = rect;
        let rs = self.art.room_size();
        let mut bodies = self.bodies(room, t);
        let view = rect.size() / PX;
        let src = if compact {
            let focus = bodies.iter().find(|b| b.me).map_or_else(|| self.art.spot(room, false), |b| pos2(b.feet.x, b.feet.y - self.m.hop));
            pos2((focus.x - view.x / 2.0).clamp(0.0, rs.x - view.x).round(), (focus.y - view.y + 10.0).clamp(0.0, rs.y - view.y).round())
        } else {
            Pos2::ZERO
        };
        let shake = if self.shake > 0.3 { vec2(self.rng.f32() - 0.5, self.rng.f32() - 0.5) * self.shake } else { Vec2::ZERO };
        let origin = rect.min + shake;
        let to = |p: Pos2| origin + (p - src) * PX;
        let painter = ui.painter_at(rect);
        self.art.room(&painter, room, Rect::from_min_size(origin, rect.size()), Rect::from_min_size(src, view), Color32::WHITE);

        bodies.sort_by(|a, b| a.feet.y.total_cmp(&b.feet.y));
        let mut hits = Vec::new();
        self.heads.clear();
        self.me_head = None;
        for b in &bodies {
            let feet = to(b.feet);
            let shadow = Rect::from_center_size(to(pos2(b.feet.x, b.feet.y.max(b.feet.y - self.m.hop * 0.0))) + vec2(0.0, -1.0), vec2(20.0 * PX * b.squash.x, 4.0 * PX));
            painter.rect_filled(shadow, 8.0, Color32::from_black_alpha(70));
            let head = self.art.draw_pet(
                &painter,
                &PetDraw { species: b.species, hat: b.hat, anim: b.anim, frame: b.frame, feet, scale: PX, squash: b.squash, flip: b.flip, flash: b.flash },
            );
            if let Some(name) = &b.name {
                let g = painter.layout_no_wrap(name.clone(), FontId::proportional(10.0), TEXT);
                let r = Rect::from_center_size(feet + vec2(0.0, 7.0), g.size() + vec2(8.0, 2.0));
                painter.rect_filled(r, 5.0, Color32::from_black_alpha(150));
                painter.galley(r.min + vec2(4.0, 1.0), g, TEXT);
            }
            if b.me {
                self.me_head = Some(head);
            }
            if let Some(f) = &mut self.fight {
                if room == Room::Arena {
                    let i = if b.me { f.me } else { 1 - f.me };
                    f.heads[i] = head;
                }
            }
            self.heads.push((b.id, head, feet, b.anim));
            hits.push((b.id, b.me, Rect::from_min_max(pos2(feet.x - 13.0 * PX, feet.y - 27.0 * PX), pos2(feet.x + 13.0 * PX, feet.y))));
        }
        let fresh = std::mem::take(&mut self.fresh);
        for id in fresh {
            if let Some(&(_, _, feet, _)) = self.heads.iter().find(|h| h.0 == id) {
                self.burst(feet + vec2(0.0, -20.0), Fx::Spark(TEXT), 10, 90.0);
            }
        }

        // everyone's speech bubbles
        let bounds = if compact { ui.max_rect() } else { rect };
        let heads: Vec<(u64, Pos2)> = self.heads.iter().map(|h| (h.0, h.1)).collect();
        for (id, head) in heads {
            if let Some((text, until)) = self.said.get(&id) {
                let pop = ((until - t) as f32).min(0.25) / 0.25;
                bubble(ui.painter(), head, text, bounds, pop);
            }
        }

        let resp = ui.interact(rect, ui.id().with(("scene", compact)), Sense::click_and_drag());
        let over_me = resp.hover_pos().is_some_and(|p| hits.iter().any(|h| h.1 && h.2.contains(p)));
        if over_me && !self.m.hovered {
            self.m.sv -= 3.0;
        }
        self.m.hovered = over_me;
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                match hits.iter().rev().find(|h| h.2.contains(p)) {
                    Some(&(_, true, _)) => acts.push(Act::PetIt),
                    Some(&(id, false, _)) if self.fight.is_none() && self.peers.contains_key(&id) => {
                        acts.push(Act::Select(Some(id)));
                        if compact {
                            acts.push(Act::Panel(true));
                        }
                    }
                    _ if !compact => acts.push(Act::Select(None)),
                    _ => {}
                }
            }
        }
        if resp.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if compact {
            if resp.double_clicked() {
                acts.push(Act::Panel(true));
            }
            resp.context_menu(|ui| {
                if ui.button("Open LanPet").clicked() {
                    acts.push(Act::Panel(true));
                }
                if ui.button("Quit").clicked() {
                    acts.push(Act::Quit);
                }
            });
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
                Fx::Note => text("♪", TEXT, q.size),
                Fx::Zzz => text("z", Color32::from_rgb(0xc8, 0xd0, 0xff), 10.0 + k * 10.0),
                Fx::Coin => {
                    p.circle_filled(q.pos, q.size, GOLD.gamma_multiply(a));
                    p.circle_stroke(q.pos, q.size, Stroke::new(1.5, Color32::from_rgb(0xb0, 0x7a, 0x10).gamma_multiply(a)));
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
            let font = FontId::proportional(size);
            p.text(f.pos + vec2(1.5, 1.5), Align2::CENTER_CENTER, &f.text, font.clone(), INK.gamma_multiply(a));
            p.text(f.pos, Align2::CENTER_CENTER, &f.text, font, f.color.gamma_multiply(a));
        }
    }

    // -------------------------------------------------------------------------------------- compact widget

    fn compact_ui(&mut self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        let win = ui.max_rect();
        let card = Rect::from_min_size(pos2(win.min.x + 10.0, win.max.y - 104.0), vec2(200.0, 100.0));
        let room = self.view_room();
        ui.painter().rect_filled(card.expand(3.0), 9.0, Color32::from_black_alpha(160));
        self.scene(ui, card, room, true, t, acts);
        ui.painter().rect_stroke(card.expand(1.0), 4.0, Stroke::new(2.0, CARD2), StrokeKind::Outside);

        let Some(pet) = &self.save.pet else { return };
        if let Some(task) = pet.task {
            let left = task.end.saturating_sub(now());
            let frac = 1.0 - left as f32 / task.job.secs() as f32;
            let label = match task.job {
                Job::Explore(z) => format!("🌀 {} · {}", ZONES[z as usize].name, mmss(left)),
                j => format!("{} · {}", j.label(), mmss(left)),
            };
            let g = ui.painter().layout_no_wrap(label, FontId::proportional(10.5), TEXT);
            let chip = Rect::from_min_size(card.min + vec2(5.0, 5.0), g.size() + vec2(10.0, 4.0));
            ui.painter().rect_filled(chip, 6.0, Color32::from_black_alpha(170));
            ui.painter().galley(chip.min + vec2(5.0, 2.0), g, TEXT);
            let bar = Rect::from_min_size(pos2(card.min.x, card.max.y - 3.0), vec2(card.width() * frac, 3.0));
            ui.painter().rect_filled(bar, 0.0, ACCENT);
        }
        let hovered = ui.rect_contains_pointer(win);
        if hovered {
            let r = Rect::from_min_size(pos2(card.max.x - 30.0, card.min.y + 4.0), vec2(26.0, 22.0));
            if ui.put(r, Button::new("⛶").fill(Color32::from_black_alpha(170))).on_hover_text("Open").clicked() {
                acts.push(Act::Panel(true));
            }
            let bars = [(self.disp[0] / pet.total_max_hp(), RED), (self.disp[1] / 100.0, GOLD), (self.disp[2] / 100.0, GREEN), (self.disp[3] / 100.0, PINK)];
            for (i, (v, c)) in bars.into_iter().enumerate() {
                let r = Rect::from_min_size(pos2(card.min.x + 6.0, card.max.y - 26.0 + i as f32 * 5.0), vec2(40.0, 3.0));
                ui.painter().rect_filled(r, 1.0, Color32::from_black_alpha(170));
                ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * v.clamp(0.0, 1.0), 3.0)), 1.0, c);
            }
        }
        if let Some(inc) = &self.incoming {
            let secs = (inc.until - t).max(0.0) as u32;
            let row = Rect::from_min_size(pos2(card.min.x + 30.0, card.min.y - 30.0), vec2(140.0, 24.0));
            let (a, b) = row.split_left_right_at_fraction(0.62);
            if ui.put(a.shrink(1.0), Button::new(RichText::new(format!("⚔ Fight! {secs}")).strong()).fill(ACCENT)).clicked() {
                acts.push(Act::Accept);
            }
            if ui.put(b.shrink(1.0), Button::new("Nah")).clicked() {
                acts.push(Act::Decline);
            }
        }
        self.paint_fx(&ui.painter_at(win));
    }

    // -------------------------------------------------------------------------------------- panel

    fn panel_ui(&mut self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        let win = ui.max_rect();
        ui.painter().rect_filled(win, 14.0, BG);
        ui.painter().rect_stroke(win, 14.0, Stroke::new(1.0, CARD2), StrokeKind::Inside);

        let header = Rect::from_min_size(win.min, vec2(win.width(), 42.0));
        if ui.interact(header, ui.id().with("header"), Sense::click_and_drag()).drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        ui.scope_builder(UiBuilder::new().max_rect(header.shrink2(vec2(14.0, 8.0))).layout(Layout::left_to_right(Align::Center)), |ui| {
            match &self.save.pet {
                Some(p) => {
                    ui.label(RichText::new(&p.name).size(17.0).strong());
                    ui.label(RichText::new(format!("Lv {} {}", p.level, p.species.name())).color(DIM));
                }
                None => {
                    ui.label(RichText::new("LanPet").size(17.0).strong());
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("🗙").on_hover_text("Quit (your pet keeps living)").clicked() {
                    acts.push(Act::Quit);
                }
                if let Some(p) = &self.save.pet {
                    if ui.button("🗕").on_hover_text("Shrink to desktop pet").clicked() {
                        acts.push(Act::Panel(false));
                    }
                    ui.label(RichText::new(format!("💰 {}", p.gold)).color(GOLD).strong());
                }
            });
        });

        let scene = Rect::from_min_size(pos2(win.min.x + 1.0, header.max.y), vec2(win.width() - 2.0, 168.0));
        let room = self.view_room();
        self.scene(ui, scene, room, false, t, acts);
        if self.fight.is_some() {
            self.fight_overlay(ui.painter(), scene);
        }
        self.paint_fx(&ui.painter_at(scene));

        if self.save.pet.is_none() {
            let content = Rect::from_min_max(pos2(win.min.x + 14.0, scene.max.y + 12.0), win.max - vec2(14.0, 12.0));
            ui.scope_builder(UiBuilder::new().max_rect(content), |ui| self.hatch_ui(ui, acts));
            return;
        }

        let nav = Rect::from_min_size(pos2(win.min.x + 8.0, scene.max.y + 8.0), vec2(win.width() - 16.0, 42.0));
        self.nav(ui, nav, acts);
        let bars = Rect::from_min_size(pos2(win.min.x + 12.0, nav.max.y + 8.0), vec2(win.width() - 24.0, 44.0));
        self.bars(ui.painter(), bars);
        let chat = Rect::from_min_max(pos2(win.min.x + 8.0, win.max.y - 122.0), win.max - vec2(8.0, 8.0));
        let content = Rect::from_min_max(pos2(win.min.x + 8.0, bars.max.y + 8.0), pos2(win.max.x - 8.0, chat.min.y - 6.0));
        ui.scope_builder(UiBuilder::new().max_rect(content), |ui| {
            egui::ScrollArea::vertical().id_salt("content").auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                self.content(ui, t, acts);
            });
        });
        self.chat_ui(ui, chat, acts);
    }

    fn fight_overlay(&self, p: &Painter, scene: Rect) {
        let Some(f) = &self.fight else { return };
        for (side, i) in [(0.0, f.me), (1.0, 1 - f.me)] {
            let r = Rect::from_min_size(pos2(scene.min.x + 12.0 + side * (scene.width() / 2.0), scene.min.y + 10.0), vec2(scene.width() / 2.0 - 24.0, 26.0));
            p.rect_filled(r, 6.0, Color32::from_black_alpha(170));
            p.text(r.min + vec2(6.0, 3.0), Align2::LEFT_TOP, format!("{}  Lv{}", f.f[i].name, f.f[i].level), FontId::proportional(11.0), TEXT);
            let bar = Rect::from_min_size(r.min + vec2(6.0, 17.0), vec2(r.width() - 12.0, 5.0));
            p.rect_filled(bar, 2.0, WELL);
            let frac = (f.shown[i] / f.f[i].max_hp.max(1) as f32).clamp(0.0, 1.0);
            p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, 5.0)), 2.0, if frac < 0.3 { RED } else { GREEN });
        }
        p.text(pos2(scene.center().x, scene.min.y + 23.0), Align2::CENTER_CENTER, "VS", FontId::proportional(14.0), GOLD);
    }

    fn nav(&self, ui: &mut Ui, rect: Rect, acts: &mut Vec<Act>) {
        let gap = 4.0;
        let w = (rect.width() - gap * 7.0) / 8.0;
        let here = self.pet_room();
        for (i, &r) in Room::ALL.iter().enumerate() {
            let b = Rect::from_min_size(pos2(rect.min.x + i as f32 * (w + gap), rect.min.y), vec2(w, rect.height()));
            let resp = ui.interact(b, ui.id().with(("nav", i)), Sense::click()).on_hover_text(r.name());
            let sel = r == self.room && self.fight.is_none();
            let painter = ui.painter_at(b);
            let spot = self.art.spot(r, false);
            let src = Rect::from_min_size(pos2((spot.x - w / 2.0).clamp(0.0, 200.0 - w), 76.0 - rect.height()), b.size());
            let tint = if sel || resp.hovered() { Color32::WHITE } else { Color32::from_gray(170) };
            self.art.room(&painter, r, b, src, tint);
            painter.rect_filled(Rect::from_min_size(pos2(b.min.x, b.max.y - 13.0), vec2(w, 13.0)), 0.0, Color32::from_black_alpha(170));
            painter.text(pos2(b.center().x, b.max.y - 6.5), Align2::CENTER_CENTER, r.name(), FontId::proportional(9.5), if sel { TEXT } else { DIM });
            painter.rect_stroke(b, 6.0, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { ACCENT } else { CARD2 }), StrokeKind::Inside);
            if here == Some(r) {
                painter.circle_filled(b.min + vec2(7.0, 7.0), 3.5, GOLD);
            }
            let n = self.peers.values().filter(|p| p.card.room == Some(r as u8)).count();
            if n > 0 {
                let c = pos2(b.max.x - 8.0, b.min.y + 8.0);
                painter.circle_filled(c, 6.5, GREEN);
                painter.text(c, Align2::CENTER_CENTER, n.to_string(), FontId::proportional(9.5), INK);
            }
            if resp.clicked() && self.fight.is_none() {
                acts.push(Act::Go(r));
            }
        }
    }

    fn bars(&self, p: &Painter, rect: Rect) {
        let Some(pet) = &self.save.pet else { return };
        let max = pet.total_max_hp();
        let w = (rect.width() - 8.0) / 2.0;
        let rows = [
            ("♥ HP", self.disp[0], pet.hp, max, RED),
            ("⚡ Energy", self.disp[1], pet.energy, 100.0, GOLD),
            ("🍖 Food", self.disp[2], pet.hunger, 100.0, GREEN),
            ("☺ Mood", self.disp[3], pet.mood, 100.0, PINK),
        ];
        for (i, (label, shown, real, max, c)) in rows.into_iter().enumerate() {
            let r = Rect::from_min_size(rect.min + vec2((i % 2) as f32 * (w + 8.0), (i / 2) as f32 * 17.0), vec2(w, 14.0));
            bar(p, r, label, shown, real, max, c);
        }
        let r = Rect::from_min_size(rect.min + vec2(0.0, 36.0), vec2(rect.width(), 8.0));
        let need = xp_needed(pet.level);
        p.rect_filled(r, 4.0, WELL);
        p.rect_filled(Rect::from_min_size(r.min, vec2(r.width() * (self.disp[4] / need).clamp(0.0, 1.0), 8.0)), 4.0, BLUE);
        p.text(r.right_center() - vec2(4.0, 0.0), Align2::RIGHT_CENTER, format!("XP {:.0}/{need:.0}", pet.xp), FontId::proportional(8.5), TEXT);
    }

    fn chat_ui(&mut self, ui: &mut Ui, rect: Rect, acts: &mut Vec<Act>) {
        ui.painter().rect_filled(rect, 10.0, CARD);
        let room = self.view_room();
        let here = self.peers.values().filter(|p| p.card.room == Some(room as u8)).count();
        ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(8.0)), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("💬 {} chat", room.name())).strong().size(12.0));
                let who = match here {
                    0 => "nobody else here".to_string(),
                    1 => "1 other pet here".to_string(),
                    n => format!("{n} other pets here"),
                };
                ui.label(RichText::new(who).color(DIM).size(11.0));
            });
            egui::ScrollArea::vertical().id_salt("chat").max_height(54.0).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                let mut any = false;
                for l in self.chat.iter().filter(|l| l.room == room) {
                    any = true;
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(RichText::new(&l.name).color(if l.mine { ACCENT } else { GOLD }).strong().size(12.0));
                        ui.label(RichText::new(&l.text).size(12.0));
                    });
                }
                if !any {
                    ui.label(RichText::new("Pets in this room see what you say here.").color(DIM).size(11.0));
                }
            });
            ui.horizontal(|ui| {
                let r = ui.add(TextEdit::singleline(&mut self.chat_input).hint_text("Say something…").desired_width(rect.width() - 84.0).char_limit(120));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if (ui.button("Send").clicked() || enter) && !self.chat_input.trim().is_empty() {
                    acts.push(Act::Say(std::mem::take(&mut self.chat_input)));
                    r.request_focus();
                }
            });
        });
    }

    fn hatch_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Act>) {
        ui.label(RichText::new("Pick your buddy").size(18.0).strong());
        ui.label(RichText::new("They live on your desktop, train while you work, and hang out with coworkers' pets on the LAN.").color(DIM));
        ui.add_space(6.0);
        let size = vec2((ui.available_width() - 4.0 * 6.0) / 5.0, 96.0);
        ui.horizontal(|ui| {
            for (i, s) in Species::ALL.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(size, Sense::click());
                let sel = self.hatch_species == i;
                ui.painter().rect_filled(r, 10.0, if sel { ACCENT_DIM } else if resp.hovered() { CARD2 } else { CARD });
                let t = ui.input(|i| i.time);
                let anim = if sel { Anim::Happy } else { Anim::Idle };
                let bob = if sel { -((t * 6.0).sin().abs() as f32) * 4.0 } else { 0.0 };
                self.art.draw_pet(
                    ui.painter(),
                    &PetDraw { species: *s, hat: None, anim, frame: self.art.frame(anim, t), feet: pos2(r.center().x, r.max.y - 20.0 + bob), scale: 2.0, squash: Vec2::splat(1.0), flip: false, flash: 0.0 },
                );
                ui.painter().text(pos2(r.center().x, r.max.y - 9.0), Align2::CENTER_CENTER, s.name(), FontId::proportional(12.0), TEXT);
                if resp.clicked() && self.hatch_at.is_none() {
                    self.hatch_species = i;
                }
            }
        });
        let s = Species::ALL[self.hatch_species];
        ui.label(RichText::new(format!("{} — {}  Special: {}", s.name(), s.blurb(), s.special())).color(DIM));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(TextEdit::singleline(&mut self.hatch_name).char_limit(16).desired_width(180.0));
            if ui.button("🎲").on_hover_text("Random name").clicked() {
                self.hatch_name = game::random_name(&mut self.rng);
            }
        });
        ui.add_space(8.0);
        let label = if self.hatch_at.is_some() { "Hatching..." } else { "✨  Hatch!" };
        if ui.add_sized([ui.available_width(), 44.0], Button::new(RichText::new(label).size(18.0).strong()).fill(ACCENT)).clicked() {
            acts.push(Act::Hatch);
        }
    }

    // -------------------------------------------------------------------------------------- panel content

    fn content(&self, ui: &mut Ui, t: f64, acts: &mut Vec<Act>) {
        let Some(pet) = &self.save.pet else { return };
        if let Some(inc) = &self.incoming {
            section(ui, |ui| {
                ui.label(RichText::new(format!("⚔ {} (Lv {} {}) challenges you!", inc.card.name, inc.card.level, inc.card.species.name())).strong());
                ui.horizontal(|ui| {
                    if ui.add(Button::new(RichText::new("Fight!").strong()).fill(ACCENT)).clicked() {
                        acts.push(Act::Accept);
                    }
                    if ui.button("Not now").clicked() {
                        acts.push(Act::Decline);
                    }
                    ui.label(RichText::new(format!("{}s", (inc.until - t).max(0.0) as u32)).color(DIM));
                });
            });
        }
        if let Some(p) = &self.pending {
            let name = self.peers.get(&p.peer).map_or("them", |x| x.card.name.as_str());
            section(ui, |ui| {
                ui.label(RichText::new(format!("Waiting for {name} to accept… {}s", (p.until - t).max(0.0) as u32)).color(DIM));
            });
        }
        if let Some(f) = &self.fight {
            section(ui, |ui| self.fight_card(ui, f, acts));
            return;
        }
        if let Some(rep) = &self.report {
            section(ui, |ui| self.report_card(ui, rep, acts));
        }
        if let Some(peer) = self.selected.and_then(|id| self.peers.get(&id)) {
            section(ui, |ui| {
                ui.horizontal(|ui| {
                    self.mini(ui, &peer.card);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&peer.card.name).strong().size(15.0));
                        ui.label(RichText::new(format!("Lv {} {} · {}", peer.card.level, peer.card.species.name(), peer.card.status)).color(DIM));
                        ui.label(RichText::new(format!("{}W / {}L", peer.card.wins, peer.card.losses)).color(DIM));
                    });
                });
                self.peer_actions(ui, peer, acts);
            });
        }
        if let Some(task) = pet.task {
            section(ui, |ui| {
                let left = task.end.saturating_sub(now());
                let frac = 1.0 - left as f32 / task.job.secs() as f32;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(task.job.label()).strong());
                    ui.label(RichText::new(format!("{} left", mmss(left))).color(DIM));
                    if !matches!(task.job, Job::Explore(_)) {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Stop").clicked() {
                                acts.push(Act::Stop);
                            }
                        });
                    }
                });
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
                ui.painter().rect_filled(r, 4.0, WELL);
                ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * frac, 8.0)), 4.0, ACCENT);
                if job_room(task.job) != self.room && !matches!(task.job, Job::Explore(_)) {
                    ui.label(RichText::new(format!("{} is in the {}.", pet.name, job_room(task.job).name())).color(DIM).size(11.0));
                }
            });
        }
        let busy = pet.task.is_some();
        match self.room {
            Room::Home => self.home(ui, pet, acts),
            Room::Bedroom => {
                if pet.task.map(|t| t.job) != Some(Job::Sleep) {
                    activity(ui, busy, "💤  Sleep", "20 min · restores Energy, heals 40% HP", Job::Sleep, acts);
                }
            }
            Room::Kitchen => self.kitchen(ui, pet, acts),
            Room::Library => activity(ui, busy, "📖  Study", "10 min · +XP, +3 Mana · 15 Energy", Job::Study, acts),
            Room::Gym => {
                activity(ui, busy, "🏋  Lift weights", "10 min · +XP, +3 STR · 20 Energy", Job::Lift, acts);
                activity(ui, busy, "🏃  Treadmill", "10 min · +XP, +8 HP, +SPD · 20 Energy", Job::Run, acts);
            }
            Room::Portal => self.portal(ui, pet, busy, acts),
            Room::Arena => self.arena(ui, pet, acts),
            Room::Shop => self.shop(ui, pet, acts),
        }
    }

    fn fight_card(&self, ui: &mut Ui, f: &Fight, acts: &mut Vec<Act>) {
        if f.over {
            let won = f.winner == f.me;
            ui.label(RichText::new(if won { "🏆 Victory!" } else { "Defeat... good fight!" }).size(18.0).strong().color(if won { GOLD } else { TEXT }));
            ui.label(RichText::new(&f.reward).color(GREEN));
            if ui.add_sized([ui.available_width(), 32.0], Button::new("Back")).clicked() {
                acts.push(Act::CloseFight);
            }
        } else {
            ui.label(RichText::new(format!("⚔ {} vs {}", f.f[f.me].name, f.f[1 - f.me].name)).strong());
        }
        for h in f.hits[..f.step].iter().rev().take(4) {
            let who = &f.f[h.by].name;
            let line = match h.kind {
                HitKind::Miss => format!("{who} missed!"),
                HitKind::Crit => format!("{who} lands a CRIT for {}!", h.dmg),
                HitKind::Special => format!("{who} uses {} for {}!", f.sp[h.by].special(), h.dmg),
                HitKind::Hit => format!("{who} hits for {}.", h.dmg),
            };
            ui.label(RichText::new(line).color(if h.by == f.me { TEXT } else { DIM }).size(12.0));
        }
    }

    fn report_card(&self, ui: &mut Ui, rep: &Report, acts: &mut Vec<Act>) {
        let zone = ZONES[rep.zone as usize].name;
        ui.label(RichText::new(if rep.fled { format!("😵 Fled from {zone}") } else { format!("🌀 Back from {zone}!") }).size(16.0).strong());
        ui.horizontal_wrapped(|ui| {
            for (foe, lvl, won) in &rep.fights {
                ui.label(RichText::new(format!("{} {foe} {lvl}", if *won { "✔" } else { "✖" })).color(if *won { GREEN } else { RED }).size(12.0));
            }
        });
        ui.label(RichText::new(format!("+{} XP   +{} gold{}", rep.xp, rep.gold, if rep.potions > 0 { format!("   ({} potion{} used)", rep.potions, if rep.potions > 1 { "s" } else { "" }) } else { String::new() })).color(GOLD));
        if !rep.loot.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for &it in &rep.loot {
                    self.icon(ui, it, 36.0).on_hover_text(it.info().name);
                }
            });
        }
        if ui.add_sized([ui.available_width(), 30.0], Button::new(RichText::new("Nice!").strong()).fill(ACCENT_DIM)).clicked() {
            acts.push(Act::CloseReport);
        }
    }

    fn home(&self, ui: &mut Ui, pet: &game::Pet, acts: &mut Vec<Act>) {
        let f = pet.fighter(true);
        section(ui, |ui| {
            ui.label(RichText::new(pet.species.blurb()).color(DIM));
            ui.horizontal(|ui| {
                for (k, v, c) in [("STR", f.str, RED), ("MANA", f.mag, BLUE), ("DEF", f.def, TEXT), ("SPD", f.spd, GREEN)] {
                    ui.label(RichText::new(k).color(DIM).size(11.0));
                    ui.label(RichText::new(v.to_string()).color(c).strong());
                    ui.add_space(4.0);
                }
                ui.label(RichText::new(format!("{}W/{}L", pet.wins, pet.losses)).color(GOLD));
            });
            ui.horizontal(|ui| {
                for (slot, it) in [(Slot::Weapon, pet.weapon), (Slot::Armor, pet.armor), (Slot::Charm, pet.charm), (Slot::Hat, pet.hat)] {
                    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
                    ui.painter().rect_filled(r, 8.0, it.map_or(WELL, |i| RARITY[i.info().rarity as usize]));
                    match it {
                        Some(i) => {
                            self.art.item(ui.painter(), i, r.shrink(4.0));
                            if resp.on_hover_text(format!("{} — click to unequip", i.info().name)).clicked() {
                                acts.push(Act::Unequip(slot));
                            }
                        }
                        None => {
                            ui.painter().text(r.center(), Align2::CENTER_CENTER, format!("{slot:?}"), FontId::proportional(9.0), DIM);
                        }
                    }
                }
            });
        });
        section(ui, |ui| {
            ui.label(RichText::new("🎒 Bag").strong());
            if pet.bag.is_empty() {
                ui.label(RichText::new("Empty. Explore or visit the Shop!").color(DIM));
            }
            for (&it, &n) in &pet.bag {
                let info = it.info();
                ui.horizontal(|ui| {
                    self.icon(ui, it, 30.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("{} ×{n}", info.name)).strong().size(13.0));
                        ui.label(RichText::new(info.desc).color(DIM).size(11.0));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(format!("Sell {}", it.sell_price())).clicked() {
                            acts.push(Act::Sell(it));
                        }
                        let verb = match info.slot {
                            Slot::Food => "Eat",
                            Slot::Drink => "Drink",
                            Slot::Hat => "Wear",
                            _ => "Equip",
                        };
                        if ui.add(Button::new(verb).fill(ACCENT_DIM)).clicked() {
                            acts.push(Act::Use(it));
                        }
                    });
                });
            }
        });
    }

    fn kitchen(&self, ui: &mut Ui, pet: &game::Pet, acts: &mut Vec<Act>) {
        section(ui, |ui| {
            ui.label(RichText::new("🍳 Fridge").strong());
            let food: Vec<_> = pet.bag.iter().filter(|(i, _)| matches!(i.info().slot, Slot::Food | Slot::Drink)).collect();
            if food.is_empty() {
                ui.label(RichText::new("Empty! Grab something quick:").color(DIM));
                if ui.add(Button::new(format!("Buy & eat an Apple · {} gold", Item::Apple.info().price)).fill(ACCENT_DIM)).clicked() {
                    acts.push(Act::Buy(Item::Apple));
                    acts.push(Act::Use(Item::Apple));
                }
            }
            ui.horizontal_wrapped(|ui| {
                for (&it, &n) in food {
                    let resp = self.icon(ui, it, 52.0);
                    ui.painter().text(resp.rect.right_bottom() - vec2(4.0, 3.0), Align2::RIGHT_BOTTOM, format!("×{n}"), FontId::proportional(11.0), TEXT);
                    if resp.on_hover_text(format!("{} — {}", it.info().name, it.info().desc)).clicked() {
                        acts.push(Act::Use(it));
                    }
                }
            });
            ui.label(RichText::new("Click food to feed. Hunger drains slowly, even while you're away.").color(DIM).size(11.0));
        });
    }

    fn portal(&self, ui: &mut Ui, pet: &game::Pet, busy: bool, acts: &mut Vec<Act>) {
        for (i, z) in ZONES.iter().enumerate() {
            section(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(z.name).strong());
                        ui.label(RichText::new(format!("{} min · foes Lv {}-{} · {}", z.mins, z.foe_level.0, z.foe_level.1, z.foes.join(", "))).color(DIM).size(11.0));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if pet.level < z.min_level {
                            ui.label(RichText::new(format!("🔒 Lv {}", z.min_level)).color(DIM));
                        } else if ui.add_enabled(!busy, Button::new(RichText::new("Go!").strong()).fill(ACCENT_DIM)).clicked() {
                            acts.push(Act::Start(Job::Explore(i as u8)));
                        }
                    });
                });
            });
        }
    }

    fn arena(&self, ui: &mut Ui, pet: &game::Pet, acts: &mut Vec<Act>) {
        section(ui, |ui| {
            ui.label(RichText::new(format!("⚔ LAN Arena · your record {}W / {}L", pet.wins, pet.losses)).strong());
            if let Some(e) = &self.net_err {
                ui.label(RichText::new(format!("LAN offline: {e}")).color(RED));
            } else if self.peers.is_empty() {
                let dots = ".".repeat(1 + (ui.input(|i| i.time) as usize % 3));
                ui.label(RichText::new(format!("Looking for pets on your network{dots}")).color(DIM));
                ui.label(RichText::new("Run LanPet on a coworker's PC on the same Wi-Fi/LAN.").color(DIM).size(11.0));
            }
        });
        for peer in self.peers.values() {
            section(ui, |ui| {
                ui.horizontal(|ui| {
                    self.mini(ui, &peer.card);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("{}  Lv {}", peer.card.name, peer.card.level)).strong());
                        let room = peer.card.room.map_or("away", |r| Room::ALL[r as usize].name());
                        ui.label(RichText::new(format!("{} · {} · {}W/{}L", peer.card.status, room, peer.card.wins, peer.card.losses)).color(DIM).size(11.0));
                    });
                });
                self.peer_actions(ui, peer, acts);
            });
        }
    }

    fn peer_actions(&self, ui: &mut Ui, peer: &Peer, acts: &mut Vec<Act>) {
        let id = peer.card.id;
        ui.horizontal(|ui| {
            let can_fight = self.pending.is_none() && self.pet_room().is_some();
            if ui.add_enabled(can_fight, Button::new(RichText::new("⚔ Battle").strong()).fill(ACCENT_DIM)).clicked() {
                acts.push(Act::Challenge(id));
            }
            if ui.button("👋 Wave").clicked() {
                acts.push(Act::Wave(id));
            }
            if let Some(pet) = &self.save.pet {
                ui.menu_button("🎁 Gift", |ui| {
                    if pet.bag.is_empty() {
                        ui.label("Your bag is empty");
                    }
                    for (&it, &n) in &pet.bag {
                        if ui.button(format!("{} ×{n}", it.info().name)).clicked() {
                            acts.push(Act::Gift(id, it));
                            ui.close();
                        }
                    }
                });
            }
        });
    }

    fn shop(&self, ui: &mut Ui, pet: &game::Pet, acts: &mut Vec<Act>) {
        let groups: [(&str, &[Slot]); 3] = [("Food & drinks", &[Slot::Food, Slot::Drink]), ("Gear", &[Slot::Weapon, Slot::Armor, Slot::Charm]), ("Hats", &[Slot::Hat])];
        for (title, slots) in groups {
            section(ui, |ui| {
                ui.label(RichText::new(title).strong());
                for it in Item::ALL.into_iter().filter(|i| i.info().price > 0 && slots.contains(&i.info().slot)) {
                    let info = it.info();
                    ui.horizontal(|ui| {
                        self.icon(ui, it, 30.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(info.name).strong().size(13.0));
                            ui.label(RichText::new(info.desc).color(DIM).size(11.0));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let afford = pet.gold >= info.price;
                            let b = Button::new(RichText::new(format!("💰 {}", info.price)).color(if afford { GOLD } else { DIM }));
                            if ui.add(b).clicked() {
                                acts.push(Act::Buy(it));
                            }
                        });
                    });
                }
            });
        }
    }

    // -------------------------------------------------------------------------------------- small widgets

    fn icon(&self, ui: &mut Ui, it: Item, size: f32) -> egui::Response {
        let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
        let bg = RARITY[it.info().rarity as usize];
        ui.painter().rect_filled(r, 8.0, if resp.hovered() { bg.gamma_multiply(1.4) } else { bg });
        self.art.item(ui.painter(), it, r.shrink(size * 0.12));
        resp
    }

    fn mini(&self, ui: &mut Ui, card: &Card) {
        let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
        ui.painter().rect_filled(r, 8.0, WELL);
        let t = ui.input(|i| i.time);
        self.art.draw_pet(
            ui.painter(),
            &PetDraw { species: card.species, hat: card.hat, anim: Anim::Idle, frame: self.art.frame(Anim::Idle, t), feet: r.center_bottom() - vec2(0.0, 2.0), scale: 1.25, squash: Vec2::splat(1.0), flip: false, flash: 0.0 },
        );
    }
}

impl eframe::App for App {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let t = ctx.input(|i| i.time);
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.place(&ctx);
        self.poll_net(t);
        self.simulate(t);
        self.housekeeping(&ctx, t);
        self.animate(dt, t);
        let mut acts = Vec::new();
        if self.panel || self.save.pet.is_none() {
            self.panel_ui(ui, t, &mut acts);
        } else {
            self.compact_ui(ui, t, &mut acts);
        }
        for a in acts {
            self.apply(&ctx, a, t);
        }
        ctx.request_repaint_after(Duration::from_millis(if self.panel { 33 } else { 50 }));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.save.pet.is_some() {
            self.persist();
        }
    }
}

// ------------------------------------------------------------------------------------------ helpers

fn style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.spacing.item_spacing = vec2(6.0, 6.0);
        s.spacing.button_padding = vec2(10.0, 5.0);
        let v = &mut s.visuals;
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = WELL;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = ACCENT_DIM;
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(8);
        }
        v.widgets.inactive.weak_bg_fill = CARD2;
        v.widgets.inactive.bg_fill = CARD2;
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3d, 0x38, 0x4c);
        v.widgets.active.weak_bg_fill = ACCENT;
    });
}

fn section(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new().fill(CARD).corner_radius(10).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

fn activity(ui: &mut Ui, busy: bool, label: &str, sub: &str, job: Job, acts: &mut Vec<Act>) {
    section(ui, |ui| {
        let b = Button::new(RichText::new(label).size(16.0).strong()).fill(ACCENT_DIM);
        if ui.add_enabled(!busy, b).clicked() {
            acts.push(Act::Start(job));
        }
        ui.label(RichText::new(sub).color(DIM).size(11.0));
    });
}

fn bar(p: &Painter, r: Rect, label: &str, shown: f32, real: f32, max: f32, c: Color32) {
    p.rect_filled(r, 4.0, WELL);
    let w = |v: f32| r.width() * (v / max).clamp(0.0, 1.0);
    if real > shown + 0.5 {
        p.rect_filled(Rect::from_min_size(r.min, vec2(w(real), r.height())), 4.0, Color32::WHITE.gamma_multiply(0.5));
    }
    p.rect_filled(Rect::from_min_size(r.min, vec2(w(shown), r.height())), 4.0, c.gamma_multiply(0.85));
    p.rect_filled(Rect::from_min_size(r.min + vec2(2.0, 2.0), vec2((w(shown) - 4.0).max(0.0), 2.0)), 1.0, Color32::WHITE.gamma_multiply(0.25));
    let font = FontId::proportional(10.0);
    p.text(r.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, label, font.clone(), TEXT);
    p.text(r.right_center() - vec2(6.0, 0.0), Align2::RIGHT_CENTER, format!("{:.0}/{:.0}", real, max), font, TEXT);
}

/// Speech bubble with a little tail, kept inside `bounds`.
fn bubble(p: &Painter, head: Pos2, text: &str, bounds: Rect, pop: f32) {
    let g = p.layout(text.to_owned(), FontId::proportional(12.0), INK, 150.0);
    let size = g.size() + vec2(14.0, 8.0);
    let lift = 10.0 + (1.0 - pop) * 6.0;
    let mut r = Rect::from_min_size(pos2(head.x - size.x / 2.0, head.y - size.y - lift), size);
    r = r.translate(vec2((bounds.min.x + 2.0 - r.min.x).max(0.0) + (bounds.max.x - 2.0 - r.max.x).min(0.0), (bounds.min.y + 2.0 - r.min.y).max(0.0)));
    let tip_x = head.x.clamp(r.min.x + 8.0, r.max.x - 8.0);
    let bg = Color32::from_rgb(0xf4, 0xf0, 0xfa);
    p.rect_filled(r.translate(vec2(0.0, 2.0)), 7.0, Color32::from_black_alpha(90));
    p.rect_filled(r, 7.0, bg);
    p.add(Shape::convex_polygon(vec![pos2(tip_x - 5.0, r.max.y - 1.0), pos2(tip_x + 5.0, r.max.y - 1.0), pos2(tip_x, r.max.y + 6.0)], bg, Stroke::NONE));
    p.galley(r.min + vec2(7.0, 4.0), g, INK);
}
