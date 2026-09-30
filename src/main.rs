#![cfg_attr(windows, windows_subsystem = "windows")]
//! LanPet: a tiny pixel pet that lives in your system tray and hangs out with coworkers' pets over the LAN.
//!
//! This file holds the app's state and its game loop (network, simulation, animation, actions).
//! Drawing lives in `view`, its look in `look`, the per-room choices in `actions`, and the
//! tray/popover/expanded window handling in `window`.

mod actions;
mod art;
mod game;
mod look;
mod net;
mod tray;
mod update;
mod view;
mod window;

// Portable Linux binary: newer glibc re-versioned a few libm functions egui uses, which would make
// the build require the builder's glibc. Bind them to their original versions instead. Release is
// fat-LTO with one codegen unit, so this directive covers every call site; result runs on glibc 2.34+.
#[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
std::arch::global_asm!(
    ".symver atan2f, atan2f@GLIBC_2.2.5",
    ".symver acosf, acosf@GLIBC_2.2.5",
    ".symver hypotf, hypotf@GLIBC_2.2.5",
    ".symver hypot, hypot@GLIBC_2.2.5",
);

use art::{Anim, Art, Room, assign_slots};
use eframe::egui::{self, Color32, Pos2, Rect, Ui, Vec2, pos2, vec2};
use game::{BEAT, DAY, Event, Fighter, Hit, HitKind, Item, Job, Need, Report, Rng, Save, Slot, Species, Stage, ZONES, clean, now};
use look::{AQUA, BLUE, DIM, GOLD, LEAF, PINK, RED, VIOLET};
use net::{Card, Msg, Net};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tray::{Badge, Tray};

/// Screen pixels per room/sprite pixel.
const PX: f32 = 2.0;
const FLOOR_Y: f32 = 74.0;

fn main() -> eframe::Result {
    let path = Save::path();
    let save = Save::load(&path);
    let size = if save.pet.is_some() { window::POPOVER } else { window::HATCH };
    let vp = egui::ViewportBuilder::default()
        .with_title("LanPet")
        .with_app_id("lanpet")
        .with_inner_size(size)
        .with_decorations(false)
        .with_transparent(true)
        .with_has_shadow(false) // macOS would shadow the transparent area
        .with_resizable(false)
        .with_always_on_top()
        // X11: not a "normal" window, so shell extensions that frame, round or shadow app windows leave the pet see-through
        .with_window_type(egui::X11WindowType::Utility)
        .with_taskbar(false);
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

/// A line in the chat log: something said in a room, or our pet's own news (`room` and `name` None).
struct ChatLine {
    room: Option<Room>,
    name: Option<String>,
    text: String,
    mine: bool,
    /// When it arrived, so the desktop pet can pop it up for a moment.
    at: f64,
}

struct Fight {
    f: [Fighter; 2],
    sp: [Species; 2],
    st: [Stage; 2],
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

/// The desktop pet: the window shrinks to just the pet and walks along the screen (points).
struct Roam {
    pos: Pos2,
    target_x: f32,
    wander_at: f64,
    /// Picked up: the OS drags the window until it has sat still for a moment.
    held: bool,
    still_at: f64,
    /// The window has grown upward to show the chat log above the pet.
    log: bool,
    /// Pointer on screen while it's over the window. Screen, not window, coordinates: the window
    /// moves and grows under a still pointer, and egui only hears where it is when it moves.
    pointer: Option<Pos2>,
    /// When the pointer came to rest on the pet or its log, and until when the log stays open for it.
    hover_since: Option<f64>,
    hover_until: f64,
}

struct Body {
    id: u64,
    species: Species,
    stage: Stage,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    /// Hidden; only the tray icon shows.
    Tray,
    /// Just the pet, out on the desktop.
    Roam,
    Popover,
    Expanded,
}

/// The expanded view's side panel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Here,
    Bag,
    Chat,
}

#[derive(Clone)]
enum Act {
    Start(Job),
    Stop,
    Use(Item),
    /// Water from the kitchen's cooler.
    Drink,
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
    View(View),
    /// Close the window: the pet goes out on the desktop (true) or hides in the tray.
    Out(bool),
    Tab(Tab),
    Quit,
    CloseReport,
    CloseFight,
    Hatch,
    PetIt,
    Say(String),
    /// Install the downloaded new version and restart as it.
    Update,
}

struct App {
    art: Art,
    save: Save,
    path: PathBuf,
    rng: Rng,
    net: Option<Net>,
    net_err: Option<String>,
    tray: Option<Tray>,
    update: update::Updater,
    clock: Instant,
    peers: BTreeMap<u64, Peer>,
    fresh: Vec<u64>,
    view: View,
    tab: Tab,
    want_view: Option<View>,
    room: Room,
    selected: Option<u64>,
    m: Motion,
    roam: Roam,
    fx: Vec<Particle>,
    floaters: Vec<Floater>,
    said: HashMap<u64, (String, f64)>,
    shake: f32,
    /// Room-change beat: the room cuts to black for a moment and the title hops.
    cut: f32,
    kick: f32,
    /// Smoothed bars: HP, energy, food, water, mood, XP.
    disp: [f32; 6],
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
    last_tip: f64,
    /// Something happened while hidden: the tray icon wears a badge until the window opens.
    attention: bool,
    /// What the pet last asked for, so each need is announced once.
    last_need: Option<Need>,
    /// After the window closes, a needy pet waits this long before coming out of the tray by itself.
    snooze: f64,
    screen: Vec2,
    ppp: f32,
    win_rect: Rect,
    placed: bool,
    shown_at: f64,
    hidden_at: f64,
    had_focus: bool,
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

/// Where a need gets fixed.
fn need_room(n: Need) -> Room {
    match n {
        Need::Sick => Room::Shop,
        Need::Thirsty | Need::Hungry => Room::Kitchen,
        Need::Tired => Room::Bedroom,
        Need::Sad => Room::Home,
    }
}

fn need_lines(n: Need) -> &'static [&'static str; 3] {
    match n {
        Need::Sick => &["I don't feel so good...", "*cough cough*", "Medicine, please..."],
        Need::Thirsty => &["So thirsty...", "Water, please!", "*dry gulp*"],
        Need::Hungry => &["I'm hungry...", "Food? Food!", "*tummy rumbles*"],
        Need::Tired => &["*yawn*", "So sleepy...", "Nap time?"],
        Need::Sad => &["Play with me?", "I'm lonely...", "*sigh*"],
    }
}

/// Desktop notification, sent off the UI thread (some backends block until it's on screen).
fn notify(title: String, body: String) {
    std::thread::spawn(move || {
        if let Err(e) = notify_rust::Notification::new().appname("LanPet").summary(&title).body(&body).show() {
            eprintln!("lanpet: notification failed: {e}");
        }
    });
}

const IDLE_LINES: [&str; 8] = ["♪ la la la", "What are we working on?", "Pet me!", "*stretches*", "You got this!", "Snack break soon?", "Boop!", "Stay hydrated!"];
const JOB_LINES: [(Job, [&str; 3]); 4] = [
    (Job::Study, ["Hmm, fascinating...", "Taking notes!", "Big brain time."]),
    (Job::Lift, ["One more rep!", "Feel the burn!", "GAINS!"]),
    (Job::Run, ["Zoom zoom!", "Cardio!", "Can't catch me!"]),
    (Job::Sleep, ["zzz...", "*snore*", "mmm... snacks..."]),
];

impl App {
    fn new(cc: &eframe::CreationContext<'_>, save: Save, path: PathBuf) -> Self {
        let ctx = cc.egui_ctx.clone();
        look::style(&ctx);
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
            tray: Tray::start(&ctx),
            update: update::Updater::start(),
            clock: Instant::now(),
            save,
            path,
            rng,
            net,
            net_err,
            peers: BTreeMap::new(),
            fresh: Vec::new(),
            view: View::Popover,
            tab: Tab::Here,
            want_view: None,
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
            // first time out it starts at the right edge (the position is clamped to the screen)
            roam: Roam { pos: pos2(f32::MAX, 0.0), target_x: f32::MAX, wander_at: 0.0, held: false, still_at: 0.0, log: false, pointer: None, hover_since: None, hover_until: 0.0 },
            fx: Vec::new(),
            floaters: Vec::new(),
            said: HashMap::new(),
            shake: 0.0,
            cut: 0.0,
            kick: 0.0,
            disp: [0.0; 6],
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
            last_tip: -10.0,
            attention: false,
            last_need: None,
            snooze: 0.0,
            screen: Vec2::new(1920.0, 1080.0),
            ppp: 1.0,
            win_rect: Rect::NOTHING,
            placed: false,
            shown_at: 0.0,
            hidden_at: -10.0,
            had_focus: false,
            dirty: false,
        }
    }

    /// Seconds since start. egui's own clock stops while the window is hidden; this one doesn't.
    fn now_t(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }

    /// The window is open (popover or expanded), as opposed to closed into the tray or the desktop pet.
    fn open(&self) -> bool {
        matches!(self.view, View::Popover | View::Expanded)
    }

    /// Closing the window leaves the pet out on the desktop: by choice, or for want of a tray to hide in.
    fn lives_out(&self) -> bool {
        self.save.pet.is_some() && (self.save.out || self.tray.is_none())
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
        if self.fight.is_some() { Room::Arena } else { self.room }
    }

    fn peers_in(&self, room: Room) -> impl Iterator<Item = &Peer> + '_ {
        self.peers.values().filter(move |p| p.card.room == Some(room as u8))
    }

    /// Shared-room layout: our slot (only when others are here) and every visible peer's slot.
    fn layout(&self, room: Room) -> (Option<Pos2>, Vec<(u64, Pos2)>) {
        let mut list: Vec<(u64, Option<usize>)> = self.peers_in(room).map(|p| (p.card.id, p.card.job.filter(|&j| job_room(j) == room).and_then(job_slot))).collect();
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
        if self.view == View::Roam && self.roam.held {
            return Anim::Hurt; // dangling from the cursor
        }
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
        if p.sick {
            Anim::Sick
        } else if p.need().is_some() || p.hp < p.total_max_hp() * 0.2 {
            Anim::Sad
        } else if t < self.m.blink_until {
            Anim::Blink
        } else {
            Anim::Idle
        }
    }

    /// Our pet, standing at `feet`.
    fn me_body(&self, feet: Pos2, t: f64) -> Option<Body> {
        let pet = self.save.pet.as_ref()?;
        let anim = self.my_anim(t);
        Some(Body {
            id: self.save.id,
            species: pet.species,
            stage: pet.stage(),
            hat: pet.hat,
            anim,
            frame: self.art.frame(anim, t),
            feet,
            squash: vec2(1.0 + self.m.squash * 0.25, 1.0 - self.m.squash * 0.25),
            flash: self.m.flash,
            flip: self.m.flip,
            name: None,
            me: true,
        })
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
                    stage: f.st[i],
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
        if self.save.pet.is_none() {
            if room == Room::Home {
                let (frame, shake) = match self.hatch_at {
                    Some(s) => (1 + (((t - s) / 0.5) as usize).min(2), ((t - s) * 3.0) as f32),
                    None => ((t * 1.2) as usize % 2, 0.0),
                };
                let wob = (t as f32 * 40.0).sin() * shake;
                let egg = Body {
                    id: 0,
                    species: Species::ALL[self.hatch_species],
                    stage: Stage::Baby,
                    hat: None,
                    anim: Anim::Egg,
                    frame,
                    feet: pos2(100.0 + wob, FLOOR_Y),
                    squash: Vec2::splat(1.0),
                    flash: 0.0,
                    flip: false,
                    name: None,
                    me: false,
                };
                // the pet that died of old age keeps watch until the next egg hatches
                if let Some(old) = &self.save.late {
                    let bob = (t * 1.6).sin() as f32 * 2.0;
                    v.push(Body { id: 1, species: old.species, anim: Anim::Ghost, frame: self.art.frame(Anim::Ghost, t), feet: pos2(148.0, FLOOR_Y - 10.0 + bob), flip: true, name: None, ..egg });
                }
                v.push(egg);
            }
            return v;
        }
        if self.pet_room() == Some(room) {
            v.extend(self.me_body(pos2(self.m.x, self.m.y + self.m.hop), t));
        }
        for (id, feet) in self.layout(room).1 {
            let Some(p) = self.peers.get(&id) else { continue };
            let phase = (id % 997) as f64 * 0.37;
            let anim = p.card.job.map_or(if (t + phase) % 4.0 < 0.13 { Anim::Blink } else { Anim::Idle }, job_anim);
            v.push(Body {
                id,
                species: p.card.species,
                stage: p.card.stage,
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
            stage: p.stage(),
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
            let c = [GOLD, PINK, BLUE, LEAF, VIOLET, RED][i % 6];
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

    fn log(&mut self, line: ChatLine) {
        self.chat.push(line);
        if self.chat.len() > 100 {
            self.chat.remove(0);
        }
    }

    /// Our pet's own news (visits, gifts, meals, needs...) for the chat log and the desktop pet.
    fn news(&mut self, text: impl Into<String>, t: f64) {
        self.log(ChatLine { room: None, name: None, text: text.into(), mine: true, at: t });
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

    fn persist(&mut self) {
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

    /// News from the updater: a new version to offer at Home, or time to restart as it.
    fn poll_update(&mut self, ctx: &egui::Context, t: f64) {
        let msgs: Vec<_> = self.update.rx.try_iter().collect();
        for m in msgs {
            match m {
                update::Msg::Ready(v, file) => {
                    self.say_me(format!("LanPet {v} is out! Update me at Home."), t);
                    self.update.ready = Some((v, file));
                }
                update::Msg::Installed => {
                    self.quit(ctx);
                    update::relaunch();
                }
                update::Msg::Failed(e) => {
                    self.update.installing = false;
                    self.say_me(format!("Update failed: {e}"), t);
                    self.react(Anim::Sad, 1.2, t);
                }
            }
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
                self.say_me(format!("{} wants to battle!", card.name), t);
                self.news(format!("{} challenged you to a battle", card.name), t);
                self.react(Anim::Happy, 1.0, t);
                self.hop();
                self.attention |= !self.open();
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
                    self.news(format!("{name} {why}."), t);
                    self.react(Anim::Sad, 1.2, t);
                }
            }
            Msg::Wave { from } => {
                self.say_me(format!("{from} says hi!"), t);
                self.news(format!("{from} waved at you"), t);
                self.react(Anim::Happy, 1.2, t);
                self.hop();
                let a = self.anchor();
                self.burst(a, Fx::Heart, 5, 60.0);
            }
            Msg::Gift { from, item } => {
                if let Some(p) = &mut self.save.pet {
                    p.give(item, 1);
                    self.dirty = true;
                    self.attention |= !self.open();
                    self.say_me(format!("{from} sent me a {}!", item.info().name), t);
                    self.news(format!("{from} gave you a {}", item.info().name), t);
                    self.react(Anim::Happy, 1.5, t);
                    let a = self.anchor();
                    self.confetti(a, 24);
                }
            }
            Msg::Chat { id, name, room, text } => {
                if id == self.save.id {
                    return;
                }
                let room = Room::ALL[room as usize];
                // hidden in the tray, talk in our pet's room is a notification (the desktop pet shows it itself)
                if self.view == View::Tray && self.pet_room() == Some(room) {
                    self.attention = true;
                    notify(format!("{name} in the {}", room.name()), text.clone());
                }
                self.log(ChatLine { room: Some(room), name: Some(name), text: text.clone(), mine: false, at: t });
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
                if gold > 0 { format!("+{xp} XP · +{gold} gold") } else { format!("+{xp} XP") }
            }
            None => String::new(),
        };
        let [a, c] = cards;
        self.fight = Some(Fight {
            hp: [a.fighter.hp, c.fighter.hp],
            shown: [a.fighter.hp as f32, c.fighter.hp as f32],
            sp: [a.species, c.species],
            st: [a.stage, c.stage],
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
        self.selected = None;
        if !self.open() {
            self.want_view = Some(View::Popover);
        }
        self.last_hello = -10.0;
    }

    fn simulate(&mut self, t: f64) {
        let mut ev = std::mem::take(&mut self.queued);
        if let Some(p) = &mut self.save.pet {
            p.tick(now(), &mut ev);
        }
        // minutes of work paid out this frame (several at once after time away); shown together below
        let mut paid: Option<(Job, u64)> = None;
        for e in ev {
            let a = self.anchor();
            self.dirty = true;
            match e {
                Event::Paid(job) => paid = Some((job, paid.map_or(0, |p| p.1) + BEAT)),
                Event::Stopped { job, secs, why } => {
                    self.float_earned(job, secs, 15.0);
                    match why {
                        Some(why) => {
                            self.attention |= !self.open();
                            self.say_me(why, t);
                            self.news(format!("Stopped {}: {why}", job.label().to_lowercase()), t);
                            self.react(Anim::Sad, 1.2, t);
                        }
                        None => self.say_me("Break time! I keep what I earned.", t),
                    }
                }
                Event::LevelUp(l) => {
                    self.float(a + vec2(0.0, -10.0), format!("LEVEL {l}!"), GOLD, 22.0);
                    self.burst(a, Fx::Star, 14, 150.0);
                    self.react(Anim::Happy, 1.6, t);
                    self.say_me("I feel stronger!", t);
                    self.news(format!("Reached level {l}"), t);
                    self.m.flash = 1.0;
                    self.hop();
                }
                Event::Done(job) => {
                    self.attention |= !self.open();
                    self.float_earned(job, job.secs(), 16.0);
                    self.burst(a, Fx::Spark(GOLD), 8, 90.0);
                    self.react(Anim::Happy, 1.2, t);
                    self.say_me(if job == Job::Sleep { "Good morning!" } else { "Done! That was fun." }, t);
                    self.news(format!("Finished {}", job.label().to_lowercase()), t);
                }
                Event::Back(rep) => {
                    self.attention |= !self.open();
                    self.say_me(if rep.fled { "Ouch... I ran away." } else { "I'm back! Check my loot!" }, t);
                    let zone = ZONES[rep.zone as usize].name;
                    self.news(if rep.fled { format!("Fled from {zone}") } else { format!("Back from {zone} with {} loot", rep.loot.len()) }, t);
                    self.react(if rep.fled { Anim::Sad } else { Anim::Happy }, 2.0, t);
                    self.m.x = self.art.spot(Room::Portal, false).x + 30.0;
                    self.m.flip = true;
                    self.report = Some(rep);
                    self.confetti(a, 20);
                }
                Event::Grew(stage) => {
                    self.float(a + vec2(0.0, -10.0), format!("{}!", stage.name().to_uppercase()), GOLD, 22.0);
                    self.confetti(a, 30);
                    self.burst(a, Fx::Star, 12, 150.0);
                    self.react(Anim::Happy, 2.2, t);
                    self.m.flash = 1.0;
                    self.hop();
                    self.say_me(if stage == Stage::Elder { "I feel wise... and a bit creaky." } else { "Look how big I am!" }, t);
                    self.news(format!("Grew into {}", stage.name().to_lowercase()), t);
                    if let (Some(p), false) = (&self.save.pet, self.open()) {
                        self.attention = true;
                        let an = if stage == Stage::Teen { "a" } else { "an" };
                        notify(format!("{} grew up!", p.name), format!("{} is {an} {} now.", p.name, stage.name().to_lowercase()));
                    }
                }
                Event::Sick => {
                    self.say_me("I don't feel so good...", t);
                    self.news("Got sick", t);
                    self.react(Anim::Sad, 1.5, t);
                }
                Event::Healed => {
                    self.say_me("I feel better!", t);
                    self.news("Feeling better", t);
                    self.react(Anim::Happy, 1.5, t);
                    self.burst(a, Fx::Heart, 6, 70.0);
                }
                Event::Died => self.lay_to_rest(),
            }
        }
        if let Some((job, secs)) = paid {
            self.float_earned(job, secs, 13.0);
            let a = self.anchor();
            self.burst(a, Fx::Spark(GOLD), 3, 50.0);
        }
    }

    /// Old age: the pet becomes a memory, its things an inheritance, and a new egg waits to be hatched.
    fn lay_to_rest(&mut self) {
        let Some(old) = self.save.pet.take() else { return };
        if !self.open() {
            self.attention = true;
            notify(format!("{} passed away", old.name), format!("{} good days. A new egg is waiting, with everything {} owned.", old.age / DAY, old.name));
        }
        self.save.late = Some(old);
        (self.fight, self.report, self.incoming, self.pending, self.selected) = (None, None, None, None, None);
        self.room = Room::Home;
        self.hatch_name = game::random_name(&mut self.rng);
        // no pet left to walk the desktop: the hatch screen takes over an open window, else it waits in the tray
        self.want_view = Some(if self.open() || self.tray.is_none() { View::Popover } else { View::Tray });
        self.persist();
    }

    /// Floats what `secs` of a job earned over the pet's head: XP in blue, the stat in green.
    fn float_earned(&mut self, job: Job, secs: u64, size: f32) {
        let Some((xp, gain)) = self.save.pet.as_ref().map(|p| actions::earned(p, job, secs)) else { return };
        let a = self.anchor();
        self.float(a, gain, LEAF, size);
        if !xp.is_empty() {
            self.float(a + vec2(0.0, size + 2.0), xp, BLUE, size);
        }
    }

    fn housekeeping(&mut self, ctx: &egui::Context, t: f64) {
        if let Some(v) = self.want_view.take() {
            self.set_view(ctx, v, None);
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
        if t - self.last_tip > 1.0 {
            self.last_tip = t;
            self.watch_needs(ctx, t);
            let need = !self.open() && self.last_need.is_some();
            let badge = if need { Badge::Need } else if self.attention || (!self.open() && self.incoming.is_some()) { Badge::Done } else { Badge::None };
            let tip = self.tray_tip();
            if let Some(tr) = &mut self.tray {
                tr.badge(badge);
                tr.tooltip(tip);
            }
        }
    }

    /// The pet asking for care while the window is closed: a notification for each new need, and
    /// out of the tray it comes to ask in person (back in once it's happy, unless it lives out there).
    fn watch_needs(&mut self, ctx: &egui::Context, t: f64) {
        let Some(pet) = &self.save.pet else { return };
        let need = pet.need();
        if need != self.last_need {
            self.last_need = need;
            if let Some(n) = need {
                let (title, body) = (format!("{} is {}", pet.name, n.label().to_lowercase()), format!("{}.", n.fix()));
                self.news(format!("{title}. {body}"), t);
                if !self.open() {
                    notify(title, body);
                }
            }
        }
        match (self.view, need) {
            (View::Tray, Some(n)) if t > self.snooze && self.pet_room().is_some() => {
                self.set_view(ctx, View::Roam, None);
                let line = *self.rng.pick(need_lines(n));
                self.say_me(line, t);
            }
            (View::Roam, None) if !self.lives_out() => self.set_view(ctx, View::Tray, None),
            _ => {}
        }
    }

    fn animate(&mut self, ctx: &egui::Context, dt: f32, t: f64) {
        // hatching
        if let Some(start) = self.hatch_at {
            self.shake = self.shake.max(((t - start) * 2.0) as f32);
            if t - start > 1.5 {
                self.hatch_at = None;
                let name = match clean(&self.hatch_name, 16) {
                    n if n.is_empty() => game::random_name(&mut self.rng),
                    n => n,
                };
                let mut pet = game::Pet::new(name.clone(), Species::ALL[self.hatch_species], now());
                if let Some(old) = self.save.late.take() {
                    pet.inherit(old);
                }
                self.save.pet = Some(pet);
                self.m.x = 100.0;
                self.m.target_x = 100.0;
                let a = self.anchor();
                self.confetti(a, 40);
                self.burst(a, Fx::Star, 12, 160.0);
                self.react(Anim::Happy, 2.5, t);
                self.say_me(format!("Hi! I'm {name}!"), t);
                self.persist();
                self.want_view = Some(View::Popover);
            }
        }

        // our pet's body
        let job = self.save.pet.as_ref().and_then(|p| p.task).map(|t| t.job);
        if self.view == View::Roam {
            self.roam_step(ctx, job, dt, t);
        } else if let Some(room) = self.pet_room().filter(|_| self.fight.is_none()) {
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
        }
        let happy = self.save.pet.as_ref().is_some_and(|p| p.mood > 60.0 && p.need().is_none());
        if job.is_none() && !self.m.moving && happy && t > self.m.hop_at {
            self.m.hop_at = t + 5.0 + self.rng.f32() as f64 * 8.0;
            self.hop();
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
            // a pet that needs something says so, and more often
            let need = self.save.pet.as_ref().and_then(|p| p.need());
            self.m.chatter_at = t + if need.is_some() { 12.0 + self.rng.f32() as f64 * 14.0 } else { 35.0 + self.rng.f32() as f64 * 50.0 };
            if self.save.pet.is_some() {
                let line = if let Some(n) = need {
                    *self.rng.pick(need_lines(n))
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
        self.cut = (self.cut - dt * 4.0).max(0.0);
        self.kick *= (-dt * 10.0).exp();

        if let Some(p) = &self.save.pet {
            let target = [p.hp, p.energy, p.hunger, p.thirst, p.mood, p.xp];
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
            let line = format!("{} {} · {}", if won { "Beat" } else { "Lost to" }, f.f[1 - f.me].name, f.reward);
            self.news(line, t);
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
                    self.burst(at, Fx::Spark(VIOLET), 10, 120.0);
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
            Act::View(v) => self.set_view(ctx, v, None),
            Act::Out(out) => {
                self.save.out = out;
                self.dirty = true;
                self.set_view(ctx, View::Tray, None);
            }
            Act::Tab(tab) => self.tab = tab,
            Act::Quit => self.quit(ctx),
            Act::Go(r) => {
                if r != self.room && self.fight.is_none() {
                    let forward = (r as usize + 8 - self.room as usize) % 8 <= 4;
                    let followed = self.pet_room() == Some(self.room);
                    self.room = r;
                    self.selected = None;
                    self.cut = 1.0;
                    self.kick = 1.0;
                    if followed && self.pet_room() == Some(r) {
                        // walk in from the side we came from
                        self.m.x = if forward { -14.0 } else { 214.0 };
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
                            self.burst(a, Fx::Spark(VIOLET), 16, 120.0);
                            self.float(a, format!("Off to {}!", ZONES[z as usize].name), VIOLET, 16.0);
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
                    p.stop(now(), &mut self.queued);
                    self.dirty = true;
                    self.last_hello = -10.0;
                }
            }
            Act::Use(it) => match self.save.pet.as_mut().map(|p| p.use_item(it)) {
                Some(Ok(())) => {
                    self.dirty = true;
                    let info = it.info();
                    if matches!(info.slot, Slot::Food | Slot::Drink) {
                        self.react(Anim::Eat, 1.4, t);
                        self.say_me(if it == Item::Medicine { "Yuck! ...but better." } else if info.slot == Slot::Food { "Nom nom!" } else { "Glug glug!" }, t);
                        let fx = info.fx;
                        let mut line = format!("{} {}", if info.slot == Slot::Food { "Ate" } else { "Drank" }, info.name);
                        for (v, label, c) in [(fx.hunger, "Food", LEAF), (fx.thirst, "Water", AQUA), (fx.hp.min(999.0), "HP", RED), (fx.energy, "Energy", GOLD), (fx.mood, "Mood", PINK)] {
                            if v > 0.0 {
                                self.float(a, format!("+{v:.0} {label}"), c, 15.0);
                                line += &format!(" · +{v:.0} {label}");
                            }
                        }
                        self.news(line, t);
                        self.burst(a + vec2(0.0, 30.0), Fx::Confetti(Color32::from_rgb(0xc8, 0x90, 0x4c)), 6, 60.0);
                    } else {
                        self.say_me(format!("{} equipped!", info.name), t);
                        self.news(format!("Equipped {}", info.name), t);
                        self.burst(a, Fx::Spark(GOLD), 10, 100.0);
                        self.react(Anim::Happy, 1.0, t);
                        self.m.flash = 0.8;
                    }
                }
                Some(Err(e)) => self.say_me(e, t),
                None => {}
            },
            Act::Drink => {
                if let Some(p) = &mut self.save.pet {
                    p.drink();
                    self.dirty = true;
                    self.react(Anim::Eat, 1.2, t);
                    self.say_me("Glug glug!", t);
                    self.float(a, "+40 Water", AQUA, 15.0);
                    self.news("Drank from the cooler · +40 Water", t);
                }
            }
            Act::Buy(it) => match self.save.pet.as_mut().map(|p| p.buy(it)) {
                Some(Ok(())) => {
                    self.dirty = true;
                    self.float(a, format!("-{} gold", it.info().price), GOLD, 15.0);
                    self.burst(a, Fx::Coin, 8, 110.0);
                    self.say_me(format!("Ooh, {}!", it.info().name), t);
                    self.news(format!("Bought {} for {} gold", it.info().name, it.info().price), t);
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
                    self.news(format!("Sold {} for {g} gold", it.info().name), t);
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
                self.news(format!("You challenged {name}"), t);
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
                self.say_me(format!("Hi {name}!"), t);
                self.news(format!("You waved at {name}"), t);
                self.react(Anim::Happy, 1.0, t);
                self.hop();
            }
            Act::Gift(id, it) => {
                let Some((addr, name)) = self.peers.get(&id).map(|p| (p.addr, p.card.name.clone())) else { return };
                let Some(pet) = &mut self.save.pet else { return };
                if pet.take(it) {
                    let from = pet.name.clone();
                    self.send(addr, &Msg::Gift { from, item: it });
                    self.dirty = true;
                    self.say_me(format!("Sent a {}!", it.info().name), t);
                    self.news(format!("You gave {name} a {}", it.info().name), t);
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
            Act::Update => {
                self.update.install();
                self.say_me("Updating... see you in a sec!", t);
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
                self.log(ChatLine { room: Some(room), name: Some(name), text: text.clone(), mine: true, at: t });
                let id = self.save.id;
                self.say(id, text, t, 6.0);
            }
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    /// Runs even while the window is hidden in the tray, so the pet stays on the LAN.
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let t = self.now_t();
        self.handle_tray(ctx, t);
        self.poll_net(t);
        self.poll_update(ctx, t);
        self.simulate(t);
        self.housekeeping(ctx, t);
        if self.view == View::Tray {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let t = self.now_t();
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.track_window(&ctx, t);
        self.animate(&ctx, dt, t);
        let mut acts = Vec::new();
        self.keys(&ctx, &mut acts);
        if self.save.pet.is_none() {
            self.hatch_view(ui, t, &mut acts);
        } else if self.view == View::Roam {
            self.roam_view(ui, t, &mut acts);
        } else if self.view == View::Expanded {
            self.expanded_view(ui, t, &mut acts);
        } else {
            self.popover_view(ui, t, &mut acts);
        }
        for a in acts {
            self.apply(&ctx, a, t);
        }
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.save.pet.is_some() {
            self.persist();
        }
    }
}
