//! Pet simulation: stats, jobs, expeditions, items, battles and the save file.
//! Everything is driven by wall-clock seconds so the pet keeps living while the app is closed.

use crate::world::Loc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// splitmix64: tiny and deterministic, so both LAN peers replay the exact same battle.
#[derive(Clone)]
pub struct Rng(pub u64);

impl Rng {
    pub fn seeded() -> Self {
        let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        Rng(t ^ ((std::process::id() as u64) << 32))
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
    pub fn chance(&mut self, pct: i64) -> bool {
        (self.below(100) as i64) < pct
    }
    pub fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

// ------------------------------------------------------------------------------------------ species

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Species {
    Dino,
    Lizard,
    Monkey,
    Frog,
    Wolf,
}

impl Species {
    pub const ALL: [Species; 5] = [Species::Dino, Species::Lizard, Species::Monkey, Species::Frog, Species::Wolf];

    pub fn name(self) -> &'static str {
        match self {
            Species::Dino => "Dino",
            Species::Lizard => "Lizard",
            Species::Monkey => "Monkey",
            Species::Frog => "Frog",
            Species::Wolf => "Wolf",
        }
    }
    /// Asset / sprite-sheet key.
    pub fn key(self) -> &'static str {
        match self {
            Species::Dino => "dino",
            Species::Lizard => "lizard",
            Species::Monkey => "monkey",
            Species::Frog => "frog",
            Species::Wolf => "wolf",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Species::Dino => "Tanky chomper. Big HP.",
            Species::Lizard => "Quick and slippery.",
            Species::Monkey => "Scrappy all-rounder.",
            Species::Frog => "Bubbly spellcaster.",
            Species::Wolf => "Fierce striker.",
        }
    }
    pub fn special(self) -> &'static str {
        match self {
            Species::Dino => "Mega Chomp",
            Species::Lizard => "Frill Flash",
            Species::Monkey => "Banana Barrage",
            Species::Frog => "Bubble Blast",
            Species::Wolf => "Moon Howl",
        }
    }
    /// Starting (max_hp, str, mana, spd).
    fn base(self) -> (f32, f32, f32, f32) {
        match self {
            Species::Dino => (44.0, 7.0, 3.0, 4.0),
            Species::Lizard => (34.0, 5.0, 5.0, 8.0),
            Species::Monkey => (36.0, 6.0, 4.0, 7.0),
            Species::Frog => (32.0, 3.0, 8.0, 5.0),
            Species::Wolf => (36.0, 8.0, 3.0, 6.0),
        }
    }
}

// ------------------------------------------------------------------------------------------ life

pub const DAY: u64 = 86_400;
/// A whole life, in seconds lived. The clock runs while the app is closed too.
pub const LIFE: u64 = 30 * DAY;

/// Life stages, in `STAGES` order from the asset generator (each has its own sprites).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Stage {
    Baby,
    Teen,
    #[default]
    Adult,
    Elder,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Baby, Stage::Teen, Stage::Adult, Stage::Elder];

    pub fn name(self) -> &'static str {
        ["Baby", "Teen", "Adult", "Elder"][self as usize]
    }
    /// Age at which the stage is over; an elder's end is the end of its life.
    pub fn ends(self) -> u64 {
        [DAY, 4 * DAY, 24 * DAY, LIFE][self as usize]
    }
    fn at(age: u64) -> Stage {
        Stage::ALL.into_iter().find(|s| age < s.ends()).unwrap_or(Stage::Elder)
    }
    /// How demanding the pet is: needs run down (and neglect builds up) this much faster.
    fn appetite(self) -> f32 {
        [1.8, 1.3, 1.0, 1.3][self as usize]
    }
    /// Growth spurt on reaching the stage, for a perfectly cared-for pet: (max HP, STR, MANA, SPD).
    fn spurt(self) -> (f32, f32, f32, f32) {
        [(0.0, 0.0, 0.0, 0.0), (6.0, 2.0, 2.0, 1.0), (10.0, 3.0, 3.0, 1.5), (0.0, 0.0, 4.0, 0.0)][self as usize]
    }
    /// Size on screen next to an adult.
    pub fn size(self) -> f32 {
        [0.68, 0.86, 1.0, 1.0][self as usize]
    }
}

/// What the pet is asking for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Need {
    Sick,
    Thirsty,
    Hungry,
    Tired,
    Sad,
}

impl Need {
    pub fn label(self) -> &'static str {
        match self {
            Need::Sick => "Sick",
            Need::Thirsty => "Thirsty",
            Need::Hungry => "Hungry",
            Need::Tired => "Sleepy",
            Need::Sad => "Sad",
        }
    }
    /// How to help, for the status line and notifications.
    pub fn fix(self) -> &'static str {
        match self {
            Need::Sick => "Medicine or a good sleep helps",
            Need::Thirsty => "There's water at home",
            Need::Hungry => "There's food at home",
            Need::Tired => "The bed's at home",
            Need::Sad => "Pets and treats help",
        }
    }
}

// ------------------------------------------------------------------------------------------ items

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Item {
    Apple,
    Burger,
    Cake,
    Sushi,
    GoldenFish,
    Coffee,
    EnergyDrink,
    Potion,
    MegaPotion,
    Medicine,
    WoodSword,
    IronSword,
    DragonBlade,
    MagicWand,
    ArcaneOrb,
    LeatherVest,
    KnightArmor,
    LuckyClover,
    SpeedBoots,
    PartyHat,
    Flower,
    Headphones,
    WizardHat,
    CowboyHat,
    Crown,
    Halo,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Food,
    Drink,
    Weapon,
    Armor,
    Charm,
    Hat,
}

/// What an item does. Consumables use hunger/thirst/hp/energy/mood; gear uses hp (max) and the combat stats.
#[derive(Clone, Copy, Default)]
pub struct Fx {
    pub hunger: f32,
    pub thirst: f32,
    pub hp: f32,
    pub energy: f32,
    pub mood: f32,
    pub str: f32,
    pub mana: f32,
    pub def: f32,
    pub spd: f32,
}

pub struct Info {
    pub name: &'static str,
    pub slot: Slot,
    /// Shop price; 0 = loot only.
    pub price: u32,
    /// 0 common, 1 rare, 2 epic, 3 legendary.
    pub rarity: u8,
    pub fx: Fx,
    pub desc: &'static str,
}

impl Item {
    pub const ALL: [Item; 26] = [
        Item::Apple, Item::Burger, Item::Cake, Item::Sushi, Item::GoldenFish, Item::Coffee, Item::EnergyDrink,
        Item::Potion, Item::MegaPotion, Item::Medicine, Item::WoodSword, Item::IronSword, Item::DragonBlade, Item::MagicWand,
        Item::ArcaneOrb, Item::LeatherVest, Item::KnightArmor, Item::LuckyClover, Item::SpeedBoots, Item::PartyHat,
        Item::Flower, Item::Headphones, Item::WizardHat, Item::CowboyHat, Item::Crown, Item::Halo,
    ];

    pub fn info(self) -> Info {
        use Slot::*;
        let z = Fx::default();
        let (name, slot, price, rarity, fx, desc) = match self {
            Item::Apple => ("Apple", Food, 8, 0, Fx { hunger: 20.0, hp: 5.0, ..z }, "Crunchy classic."),
            Item::Burger => ("Burger", Food, 20, 0, Fx { hunger: 45.0, hp: 10.0, mood: 5.0, ..z }, "Big bite energy."),
            Item::Cake => ("Cake", Food, 35, 0, Fx { hunger: 30.0, mood: 25.0, ..z }, "Every day is a birthday."),
            Item::Sushi => ("Sushi", Food, 30, 1, Fx { hunger: 35.0, hp: 25.0, mood: 10.0, ..z }, "Fancy lunch."),
            Item::GoldenFish => ("Golden Fish", Food, 0, 2, Fx { hunger: 100.0, hp: 999.0, mood: 30.0, ..z }, "Full heal. Shiny."),
            Item::Coffee => ("Coffee", Drink, 12, 0, Fx { thirst: 15.0, energy: 25.0, mood: 5.0, ..z }, "Office fuel."),
            Item::EnergyDrink => ("Energy Drink", Drink, 30, 0, Fx { thirst: 20.0, energy: 60.0, mood: -5.0, ..z }, "Zoom. Crash later."),
            Item::Potion => ("Potion", Drink, 25, 0, Fx { hp: 50.0, ..z }, "Heals 50. Auto-used on expeditions."),
            Item::MegaPotion => ("Mega Potion", Drink, 70, 1, Fx { hp: 150.0, energy: 20.0, ..z }, "Heals 150."),
            Item::Medicine => ("Medicine", Drink, 40, 0, z, "Cures sickness. Yuck."),
            Item::WoodSword => ("Wood Sword", Weapon, 40, 0, Fx { str: 3.0, ..z }, "+3 STR"),
            Item::IronSword => ("Iron Sword", Weapon, 160, 1, Fx { str: 8.0, ..z }, "+8 STR"),
            Item::DragonBlade => ("Dragon Blade", Weapon, 0, 3, Fx { str: 18.0, spd: 2.0, ..z }, "+18 STR +2 SPD"),
            Item::MagicWand => ("Magic Wand", Weapon, 150, 1, Fx { mana: 8.0, ..z }, "+8 MANA"),
            Item::ArcaneOrb => ("Arcane Orb", Weapon, 0, 3, Fx { mana: 18.0, hp: 20.0, ..z }, "+18 MANA +20 HP"),
            Item::LeatherVest => ("Leather Vest", Armor, 60, 0, Fx { def: 3.0, hp: 15.0, ..z }, "+3 DEF +15 HP"),
            Item::KnightArmor => ("Knight Armor", Armor, 320, 2, Fx { def: 8.0, hp: 40.0, ..z }, "+8 DEF +40 HP"),
            Item::LuckyClover => ("Lucky Clover", Charm, 90, 1, Fx { spd: 2.0, ..z }, "+2 SPD, more loot"),
            Item::SpeedBoots => ("Speed Boots", Charm, 140, 1, Fx { spd: 6.0, ..z }, "+6 SPD"),
            Item::PartyHat => ("Party Hat", Hat, 30, 0, z, "It's a party."),
            Item::Flower => ("Flower", Hat, 20, 0, z, "Smells nice."),
            Item::Headphones => ("Headphones", Hat, 80, 0, z, "Lo-fi beats to train to."),
            Item::WizardHat => ("Wizard Hat", Hat, 120, 1, z, "Pointy and wise."),
            Item::CowboyHat => ("Cowboy Hat", Hat, 100, 1, z, "Yeehaw."),
            Item::Crown => ("Crown", Hat, 600, 2, z, "Office royalty."),
            Item::Halo => ("Halo", Hat, 0, 3, z, "Legendary drop."),
        };
        Info { name, slot, price, rarity, fx, desc }
    }

    pub fn sell_price(self) -> u32 {
        let i = self.info();
        if i.price > 0 { i.price / 2 } else { [10, 40, 150, 500][i.rarity as usize] }
    }
}

// ------------------------------------------------------------------------------------------ jobs & zones

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Job {
    Study,
    Lift,
    Run,
    Sleep,
    Explore(u8),
}

impl Job {
    pub fn secs(self) -> u64 {
        match self {
            Job::Study | Job::Lift | Job::Run => 600,
            Job::Sleep => 1200,
            Job::Explore(z) => ZONES[z as usize].mins * 60,
        }
    }
    pub fn energy_cost(self) -> f32 {
        match self {
            Job::Study => 15.0,
            Job::Lift | Job::Run => 20.0,
            Job::Sleep => 0.0,
            Job::Explore(_) => 25.0,
        }
    }
    fn hunger_cost(self) -> f32 {
        match self {
            Job::Study => 8.0,
            Job::Lift | Job::Run => 12.0,
            Job::Sleep => 4.0,
            Job::Explore(_) => 20.0,
        }
    }
    fn thirst_cost(self) -> f32 {
        match self {
            Job::Study => 5.0,
            Job::Lift | Job::Run => 15.0,
            Job::Sleep => 2.0,
            Job::Explore(_) => 20.0,
        }
    }
    pub fn label(self) -> String {
        match self {
            Job::Study => "Studying".into(),
            Job::Lift => "Lifting".into(),
            Job::Run => "Running".into(),
            Job::Sleep => "Sleeping".into(),
            Job::Explore(z) => format!("Exploring {}", ZONES[z as usize].name),
        }
    }
}

pub struct Zone {
    pub name: &'static str,
    pub min_level: u32,
    pub mins: u64,
    pub foe_level: (u32, u32),
    pub foes: [&'static str; 3],
    pub loot: &'static [(Item, u32)],
}

pub const ZONES: [Zone; 5] = [
    Zone {
        name: "Sunny Meadow",
        min_level: 1,
        mins: 5,
        foe_level: (1, 2),
        foes: ["Slime", "Bunbun", "Beetle"],
        loot: &[(Item::Apple, 40), (Item::Coffee, 25), (Item::Flower, 10), (Item::Potion, 15), (Item::WoodSword, 8), (Item::PartyHat, 5)],
    },
    Zone {
        name: "Whisper Woods",
        min_level: 3,
        mins: 15,
        foe_level: (3, 6),
        foes: ["Goblin", "Shroomy", "Bat"],
        loot: &[(Item::Burger, 30), (Item::Potion, 25), (Item::LeatherVest, 10), (Item::MagicWand, 6), (Item::Headphones, 6), (Item::LuckyClover, 5)],
    },
    Zone {
        name: "Crystal Caves",
        min_level: 7,
        mins: 30,
        foe_level: (7, 11),
        foes: ["Golem", "Cave Spider", "Crystal Crab"],
        loot: &[(Item::Sushi, 25), (Item::MegaPotion, 20), (Item::IronSword, 10), (Item::SpeedBoots, 8), (Item::WizardHat, 8), (Item::CowboyHat, 6)],
    },
    Zone {
        name: "Magma Peak",
        min_level: 12,
        mins: 60,
        foe_level: (12, 17),
        foes: ["Salamander", "Imp", "Lava Slug"],
        loot: &[(Item::EnergyDrink, 25), (Item::MegaPotion, 25), (Item::KnightArmor, 10), (Item::DragonBlade, 4), (Item::ArcaneOrb, 4), (Item::Crown, 3)],
    },
    Zone {
        name: "Sky Ruins",
        min_level: 20,
        mins: 120,
        foe_level: (20, 28),
        foes: ["Wyvern", "Ghost Knight", "Storm Owl"],
        loot: &[(Item::GoldenFish, 20), (Item::MegaPotion, 25), (Item::DragonBlade, 8), (Item::ArcaneOrb, 8), (Item::Crown, 6), (Item::Halo, 3)],
    },
];

// ------------------------------------------------------------------------------------------ battle

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fighter {
    pub name: String,
    pub level: u32,
    pub hp: i32,
    pub max_hp: i32,
    pub str: i32,
    pub mag: i32,
    pub def: i32,
    pub spd: i32,
}

impl Fighter {
    fn monster(name: &str, lvl: u32) -> Fighter {
        let l = lvl as i32;
        Fighter {
            name: name.into(),
            level: lvl,
            hp: 10 + 7 * l,
            max_hp: 10 + 7 * l,
            str: 1 + 2 * l,
            mag: l,
            def: l / 3,
            spd: 2 + l * 4 / 5,
        }
    }
    /// Clamp stats from untrusted LAN peers so battle math can't overflow.
    pub fn sanitize(&mut self) {
        let c = |v: i32| v.clamp(0, 100_000);
        self.max_hp = self.max_hp.clamp(1, 100_000);
        self.hp = self.hp.clamp(1, self.max_hp);
        (self.str, self.mag, self.def, self.spd) = (c(self.str), c(self.mag), c(self.def), c(self.spd));
        self.level = self.level.clamp(1, 999);
        self.name = clean(&self.name, 16);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitKind {
    Hit,
    Crit,
    Special,
    Miss,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub by: usize,
    pub kind: HitKind,
    pub dmg: i32,
    /// Target HP after the hit.
    pub hp: i32,
}

pub struct Battle {
    pub hits: Vec<Hit>,
    pub winner: usize,
    pub hp: [i32; 2],
}

/// Deterministic auto-battle. Same fighters + seed => same result on every machine.
pub fn battle(a: &Fighter, b: &Fighter, seed: u64) -> Battle {
    let f = [a, b];
    let mut hp = [a.hp, b.hp];
    let mut rng = Rng(seed);
    let mut hits = Vec::new();
    'fight: for _round in 0..30 {
        let first = if a.spd == b.spd { rng.below(2) as usize } else if a.spd > b.spd { 0 } else { 1 };
        for by in [first, 1 - first] {
            let (att, def) = (f[by], f[1 - by]);
            let miss = (8 + (def.spd - att.spd).min(20)).clamp(3, 25) as i64;
            let (kind, dmg) = if rng.chance(miss) {
                (HitKind::Miss, 0)
            } else {
                // casters lean on their special more often
                let special = att.mag > 0 && rng.chance(25 + ((att.mag - att.str) * 3).clamp(0, 25) as i64);
                let base = if special { att.mag * 3 / 2 + att.str / 4 } else { att.str };
                let dmg = (base + rng.below((base / 3 + 2) as u64) as i32 - def.def / 2).max(1);
                let crit = rng.chance((10 + (att.spd - def.spd).clamp(0, 10)) as i64);
                match (special, crit) {
                    (_, true) => (HitKind::Crit, dmg * 2),
                    (true, false) => (HitKind::Special, dmg),
                    _ => (HitKind::Hit, dmg),
                }
            };
            hp[1 - by] = (hp[1 - by] - dmg).max(0);
            hits.push(Hit { by, kind, dmg, hp: hp[1 - by] });
            if hp[1 - by] == 0 {
                break 'fight;
            }
        }
    }
    let ratio = |i: usize| hp[i] as f32 / f[i].max_hp.max(1) as f32;
    let winner = if hp[1] == 0 || (hp[0] > 0 && ratio(0) >= ratio(1)) { 0 } else { 1 };
    Battle { hits, winner, hp }
}

// ------------------------------------------------------------------------------------------ pet

/// Jobs pay out every minute, so stopping early keeps what was earned. Expeditions pay at the end.
pub const BEAT: u64 = 60;
/// Enough energy to start a job; it ends on its own when energy runs out.
const MIN_ENERGY: f32 = 5.0;
/// Below this a need counts as unmet: the pet asks for it.
const LOW: f32 = 25.0;
/// Idle time is simulated in steps this long, so needs bottoming out mid-gap are noticed.
const STEP: u64 = 600;
/// A gap longer than this means the app was closed. The pet looked after itself meanwhile:
/// needs run down at `AWAY_RATE` and it can't fall sick, so a weekend away isn't a death sentence.
const AWAY: u64 = 300;
const AWAY_RATE: f32 = 0.25;
/// Hours of an unmet need (for an adult) before the pet falls sick.
const SICK_AT: f32 = 1.5;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Task {
    pub job: Job,
    pub start: u64,
    pub end: u64,
    pub seed: u64,
}

impl Task {
    /// When this task next pays out.
    fn next_beat(&self, after: u64) -> u64 {
        match self.job {
            Job::Explore(_) => self.end,
            _ => (self.start + (after.saturating_sub(self.start) / BEAT + 1) * BEAT).min(self.end),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub zone: u8,
    /// (foe, level, won)
    pub fights: Vec<(String, u32, bool)>,
    pub xp: u32,
    pub gold: u32,
    pub loot: Vec<Item>,
    pub potions: u32,
    pub fled: bool,
}

pub enum Event {
    LevelUp(u32),
    /// A minute of work paid out (`BEAT` seconds of `Pet::job_gain`).
    Paid(Job),
    Done(Job),
    /// Ended early after `secs` of work: by you (`why` None) or because the pet ran out of steam.
    Stopped { job: Job, secs: u64, why: Option<&'static str> },
    Back(Report),
    /// Reached a new life stage.
    Grew(Stage),
    Sick,
    Healed,
    /// Old age. The pet stops ticking; the app lays it to rest.
    Died,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Pet {
    pub name: String,
    pub species: Species,
    pub level: u32,
    pub xp: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub str: f32,
    pub mana: f32,
    pub spd: f32,
    pub energy: f32,
    /// Fullness: 100 = stuffed, 0 = starving.
    pub hunger: f32,
    /// Hydration: 100 = quenched, 0 = parched.
    #[serde(default = "old_thirst")]
    pub thirst: f32,
    pub mood: f32,
    /// Seconds lived; decides the life stage.
    #[serde(default = "old_age")]
    pub age: u64,
    #[serde(default)]
    pub sick: bool,
    /// Neglect built up, in hours of an unmet need; at `SICK_AT` the pet falls sick, at 0 it's well again.
    #[serde(default)]
    pub strain: f32,
    /// How well it's been looked after lately, 0..1. Sets how big the next growth spurt is.
    #[serde(default = "old_care")]
    pub care: f32,
    pub gold: u32,
    pub bag: BTreeMap<Item, u32>,
    pub weapon: Option<Item>,
    pub armor: Option<Item>,
    pub charm: Option<Item>,
    pub hat: Option<Item>,
    pub task: Option<Task>,
    pub last: u64,
    pub wins: u32,
    pub losses: u32,
}

// Pets from saves that predate life stages: a young adult, watered and well cared for.
fn old_age() -> u64 {
    Stage::Teen.ends()
}
fn old_thirst() -> f32 {
    70.0
}
fn old_care() -> f32 {
    1.0
}

/// 30, 70, 120, 180… first level comes after one session, later ones take a while.
pub fn xp_needed(level: u32) -> f32 {
    let l = level as f32;
    25.0 * l + 5.0 * l * l
}

impl Pet {
    pub fn new(name: String, species: Species, now: u64) -> Pet {
        let (max_hp, str, mana, spd) = species.base();
        Pet {
            name,
            species,
            level: 1,
            xp: 0.0,
            hp: max_hp,
            max_hp,
            str,
            mana,
            spd,
            energy: 80.0,
            hunger: 70.0,
            thirst: 70.0,
            mood: 70.0,
            age: 0,
            sick: false,
            strain: 0.0,
            care: 1.0,
            gold: 60,
            bag: BTreeMap::from([(Item::Apple, 3), (Item::Coffee, 1)]),
            weapon: None,
            armor: None,
            charm: None,
            hat: None,
            task: None,
            last: now,
            wins: 0,
            losses: 0,
        }
    }

    pub fn stage(&self) -> Stage {
        Stage::at(self.age)
    }
    pub fn alive(&self) -> bool {
        self.age < LIFE
    }
    fn exploring(&self) -> bool {
        matches!(self.task, Some(Task { job: Job::Explore(_), .. }))
    }
    fn sleeping(&self) -> bool {
        matches!(self.task, Some(Task { job: Job::Sleep, .. }))
    }

    /// The most pressing thing the pet wants from you right now. Nothing while it's away
    /// exploring (out of reach), and a sleeping pet only wakes you for food and water.
    pub fn need(&self) -> Option<Need> {
        if self.exploring() {
            None
        } else if self.sick && !self.sleeping() {
            Some(Need::Sick)
        } else if self.thirst < LOW && self.thirst <= self.hunger {
            Some(Need::Thirsty)
        } else if self.hunger < LOW {
            Some(Need::Hungry)
        } else if self.energy < LOW && self.task.is_none() {
            Some(Need::Tired)
        } else if self.mood < LOW && !self.sleeping() {
            Some(Need::Sad)
        } else {
            None
        }
    }

    fn gear(&self) -> impl Iterator<Item = Fx> + '_ {
        [self.weapon, self.armor, self.charm].into_iter().flatten().map(|i| i.info().fx)
    }
    pub fn total_max_hp(&self) -> f32 {
        self.max_hp + self.gear().map(|f| f.hp).sum::<f32>()
    }
    pub fn fighter(&self, full_hp: bool) -> Fighter {
        let g = |f: fn(&Fx) -> f32| self.gear().map(|x| f(&x)).sum::<f32>();
        let max_hp = self.total_max_hp().round() as i32;
        Fighter {
            name: self.name.clone(),
            level: self.level,
            hp: if full_hp { max_hp } else { (self.hp.round() as i32).clamp(1, max_hp) },
            max_hp,
            str: (self.str + g(|f| f.str)).round() as i32,
            mag: (self.mana + g(|f| f.mana)).round() as i32,
            def: (self.level as f32 / 3.0 + g(|f| f.def)).round() as i32,
            spd: (self.spd + g(|f| f.spd)).round() as i32,
        }
    }

    /// Mood makes training pay off: 0.5x when miserable, 1.5x when happy.
    fn xp_mult(&self) -> f32 {
        0.5 + self.mood / 100.0
    }

    /// What a whole job gives: (XP, gain, what the gain is). XP grows with level and mood;
    /// it's earned bit by bit, so half a job gives half of this.
    pub fn job_gain(&self, job: Job) -> (f32, f32, &'static str) {
        let xp = (25.0 + 5.0 * self.level as f32) * self.xp_mult();
        match job {
            Job::Study => (xp, 3.0, "Mana"),
            Job::Lift => (xp, 3.0, "Str"),
            Job::Run => (xp, 8.0, "HP"),
            Job::Sleep => (0.0, 100.0, "Energy"),
            Job::Explore(_) => (0.0, 0.0, ""),
        }
    }

    fn gain_xp(&mut self, xp: f32, ev: &mut Vec<Event>) {
        self.xp += xp;
        while self.xp >= xp_needed(self.level) {
            self.xp -= xp_needed(self.level);
            self.level += 1;
            self.max_hp += 5.0;
            self.str += 1.0;
            self.mana += 1.0;
            self.spd += 0.5;
            self.hp = self.total_max_hp();
            ev.push(Event::LevelUp(self.level));
        }
    }

    fn clamp(&mut self) {
        self.energy = self.energy.clamp(0.0, 100.0);
        self.hunger = self.hunger.clamp(0.0, 100.0);
        self.thirst = self.thirst.clamp(0.0, 100.0);
        self.mood = self.mood.clamp(0.0, 100.0);
        self.hp = self.hp.clamp(0.0, self.total_max_hp());
    }

    /// Advance the simulation to `now`, catching up any time spent offline.
    pub fn tick(&mut self, now: u64, ev: &mut Vec<Event>) {
        let away = now.saturating_sub(self.last) > AWAY;
        while self.last < now && self.alive() {
            let stage = self.stage();
            let beat = self.task.map(|t| t.next_beat(self.last));
            // stop at the next payout, the next birthday into a new stage, or after a short step
            let end = beat.unwrap_or(now).min(self.last + STEP).min(self.last + stage.ends() - self.age).min(now).max(self.last);
            self.integrate((end - self.last) as f32, away, ev);
            self.age += end - self.last;
            self.last = end;
            if !self.alive() {
                self.task = None;
                ev.push(Event::Died);
                break;
            }
            if self.stage() != stage {
                self.grow(ev);
            }
            let Some(t) = self.task else { continue };
            if end >= t.end {
                self.task = None;
                self.finish(t, ev);
            } else if let Some(why) = self.out_of_steam(t.job) {
                self.task = None;
                ev.push(Event::Stopped { job: t.job, secs: end - t.start, why: Some(why) });
            } else if Some(end) == beat && !matches!(t.job, Job::Explore(_)) {
                ev.push(Event::Paid(t.job));
            }
        }
    }

    /// A new life stage: a growth spurt as big as the care it got, and a full heal.
    fn grow(&mut self, ev: &mut Vec<Event>) {
        let stage = self.stage();
        let (hp, str, mana, spd) = stage.spurt();
        self.max_hp += hp * self.care;
        self.str += str * self.care;
        self.mana += mana * self.care;
        self.spd += spd * self.care;
        self.hp = self.total_max_hp();
        ev.push(Event::Grew(stage));
    }

    /// Why a job can't go on (expeditions always finish).
    fn out_of_steam(&self, job: Job) -> Option<&'static str> {
        match job {
            Job::Explore(_) => None,
            _ if self.hunger <= 5.0 => Some("Too hungry to keep going!"),
            _ if self.thirst <= 5.0 => Some("Too thirsty to keep going!"),
            Job::Sleep => None,
            _ if self.sick => Some("I don't feel well..."),
            _ if self.energy <= 0.0 => Some("Too tired to keep going!"),
            _ => None,
        }
    }

    /// Neglect builds up into sickness and good care mends it; how well the pet is kept is
    /// remembered for its next growth spurt. Only counts while you're around to help (or it sleeps).
    fn tend(&mut self, h: f32, ev: &mut Vec<Event>) {
        let unmet = [self.hunger <= 5.0, self.thirst <= 5.0, self.energy <= 2.0, self.mood <= 5.0].into_iter().filter(|&b| b).count() as f32;
        // one full sleep, fed and watered, sleeps any sickness off
        let mend = if self.sleeping() { 6.0 } else { 0.5 };
        let change = if unmet > 0.0 { unmet * self.stage().appetite() } else { -mend };
        self.strain = (self.strain + change * h).clamp(0.0, SICK_AT + 0.5);
        if self.strain >= SICK_AT && !self.sick {
            self.sick = true;
            ev.push(Event::Sick);
        } else if self.strain <= 0.0 && self.sick {
            self.sick = false;
            ev.push(Event::Healed);
        }
        let well = if self.sick { 0.0 } else { (self.hunger.min(self.thirst).min(self.mood) / 50.0).min(1.0) };
        self.care = (self.care + (well - self.care) * (1.0 - (-h / 4.0).exp())).clamp(0.0, 1.0);
    }

    fn integrate(&mut self, dt: f32, away: bool, ev: &mut Vec<Event>) {
        if dt <= 0.0 {
            return;
        }
        let h = dt / 3600.0;
        let max = self.total_max_hp();
        // what time alone wears down: slower while the app was closed, faster for the young and old
        let wear = h * if away { AWAY_RATE } else { 1.0 };
        let appetite = self.stage().appetite();
        self.hunger -= 4.0 * appetite * wear;
        self.thirst -= 6.0 * appetite * wear;
        self.mood -= 2.0 * appetite * wear;
        if self.hunger <= 0.0 || self.thirst <= 0.0 || self.sick {
            self.mood -= 6.0 * wear;
            self.hp -= 0.05 * max * wear;
        } else if self.hunger > 30.0 && self.thirst > 30.0 {
            self.hp += 0.15 * max * h;
        }
        if self.energy < 10.0 {
            self.mood -= 3.0 * wear;
        }
        match self.task {
            None => self.energy += 4.0 * h,
            Some(t) => {
                let frac = dt / t.job.secs() as f32;
                self.energy -= t.job.energy_cost() * frac;
                self.hunger -= t.job.hunger_cost() * frac;
                self.thirst -= t.job.thirst_cost() * frac;
                let (xp, gain, _) = self.job_gain(t.job);
                match t.job {
                    Job::Study => self.mana += gain * frac,
                    Job::Lift => self.str += gain * frac,
                    Job::Run => {
                        self.max_hp += gain * frac;
                        self.hp += gain * frac;
                        self.spd += 0.3 * frac;
                    }
                    Job::Sleep => {
                        self.energy += gain * frac;
                        self.hp += 0.4 * max * frac;
                    }
                    Job::Explore(_) => {}
                }
                if xp > 0.0 {
                    self.gain_xp(xp * frac, ev);
                }
            }
        }
        self.clamp();
        if (!away || self.sleeping()) && !self.exploring() {
            self.tend(h, ev);
        }
    }

    fn finish(&mut self, t: Task, ev: &mut Vec<Event>) {
        match t.job {
            Job::Explore(z) => {
                let rep = self.explore(z, t.seed, ev);
                ev.push(Event::Back(rep));
            }
            job => ev.push(Event::Done(job)),
        }
    }

    pub fn start(&mut self, job: Job, now: u64, seed: u64) -> Result<(), &'static str> {
        if self.task.is_some() {
            return Err("I'm busy!");
        }
        if self.sick && job != Job::Sleep {
            return Err("I feel too sick...");
        }
        if let Job::Explore(z) = job {
            if self.stage() == Stage::Baby {
                return Err("I'm too little to explore!");
            }
            if self.level < ZONES[z as usize].min_level {
                return Err("Too scary for my level...");
            }
            if self.hp < self.total_max_hp() * 0.3 {
                return Err("Too hurt to explore. Heal me!");
            }
        }
        if job != Job::Sleep {
            // an expedition can't be cut short, so it needs all its energy up front
            let need = if matches!(job, Job::Explore(_)) { job.energy_cost() } else { MIN_ENERGY };
            if self.energy < need {
                return Err("Too sleepy... zzz");
            }
            if self.hunger < 10.0 {
                return Err("Too hungry!");
            }
            if self.thirst < 10.0 {
                return Err("Too thirsty!");
            }
        }
        self.task = Some(Task { job, start: now, end: now + job.secs(), seed });
        Ok(())
    }

    /// Stop early (not for expeditions). Everything earned up to `now` stays.
    pub fn stop(&mut self, now: u64, ev: &mut Vec<Event>) {
        self.tick(now, ev);
        if let Some(t) = self.task.filter(|t| !matches!(t.job, Job::Explore(_))) {
            self.task = None;
            ev.push(Event::Stopped { job: t.job, secs: now.saturating_sub(t.start), why: None });
        }
    }

    pub fn give(&mut self, it: Item, n: u32) {
        *self.bag.entry(it).or_default() += n;
    }
    pub fn take(&mut self, it: Item) -> bool {
        match self.bag.get_mut(&it) {
            Some(n) if *n > 0 => {
                *n -= 1;
                if *n == 0 {
                    self.bag.remove(&it);
                }
                true
            }
            _ => false,
        }
    }

    pub fn slot_mut(&mut self, slot: Slot) -> Option<&mut Option<Item>> {
        match slot {
            Slot::Weapon => Some(&mut self.weapon),
            Slot::Armor => Some(&mut self.armor),
            Slot::Charm => Some(&mut self.charm),
            Slot::Hat => Some(&mut self.hat),
            Slot::Food | Slot::Drink => None,
        }
    }

    /// Eat/drink a consumable or equip gear (the old piece goes back in the bag).
    pub fn use_item(&mut self, it: Item) -> Result<(), &'static str> {
        if !self.take(it) {
            return Err("None left!");
        }
        let info = it.info();
        match self.slot_mut(info.slot) {
            Some(slot) => {
                if let Some(old) = slot.replace(it) {
                    self.give(old, 1);
                }
            }
            None => {
                let f = info.fx;
                self.hunger += f.hunger;
                self.thirst += f.thirst;
                self.hp += f.hp;
                self.energy += f.energy;
                self.mood += f.mood;
                if it == Item::Medicine {
                    self.sick = false;
                    self.strain = 0.0;
                }
            }
        }
        self.clamp();
        Ok(())
    }

    /// A cup from the kitchen's water cooler.
    pub fn drink(&mut self) {
        self.thirst = (self.thirst + 40.0).min(100.0);
    }

    /// A new egg takes over what the last pet left behind: its gold and everything it owned.
    pub fn inherit(&mut self, old: Pet) {
        self.gold += old.gold;
        let worn = [old.weapon, old.armor, old.charm, old.hat].into_iter().flatten().map(|it| (it, 1));
        for (it, n) in old.bag.into_iter().chain(worn) {
            self.give(it, n);
        }
    }

    pub fn unequip(&mut self, slot: Slot) {
        if let Some(old) = self.slot_mut(slot).and_then(|s| s.take()) {
            self.give(old, 1);
        }
        self.clamp();
    }

    pub fn buy(&mut self, it: Item) -> Result<(), &'static str> {
        let price = it.info().price;
        if price == 0 {
            return Err("Not for sale");
        }
        if self.gold < price {
            return Err("Not enough gold!");
        }
        self.gold -= price;
        self.give(it, 1);
        Ok(())
    }

    pub fn sell(&mut self, it: Item) -> Result<u32, &'static str> {
        if !self.take(it) {
            return Err("None left!");
        }
        let g = it.sell_price();
        self.gold += g;
        Ok(g)
    }

    pub fn pet(&mut self) {
        self.mood = (self.mood + 3.0).min(100.0);
    }

    /// Friendly LAN spar: no risk, small reward, W/L record.
    pub fn pvp_reward(&mut self, won: bool, foe_level: u32, ev: &mut Vec<Event>) -> (u32, u32) {
        let (xp, gold) = if won { (15 + 4 * foe_level, 10 + 2 * foe_level) } else { (8, 0) };
        if won { self.wins += 1 } else { self.losses += 1 }
        self.gold += gold;
        self.gain_xp(xp as f32, ev);
        (xp, gold)
    }

    fn explore(&mut self, z: u8, seed: u64, ev: &mut Vec<Event>) -> Report {
        let zone = &ZONES[z as usize];
        let mut rng = Rng(seed);
        let mut rep = Report { zone: z, ..Default::default() };
        let mut me = self.fighter(false);
        let loot_bonus = if self.charm == Some(Item::LuckyClover) { 12 } else { 0 };
        let fights = 2 + zone.mins / 10;
        for _ in 0..fights {
            me.hp = (me.hp + me.max_hp / 10).min(me.max_hp); // catch a breath between fights
            if me.hp < me.max_hp * 2 / 5 {
                for pot in [Item::Potion, Item::MegaPotion] {
                    if self.take(pot) {
                        me.hp = (me.hp + pot.info().fx.hp as i32).min(me.max_hp);
                        rep.potions += 1;
                        break;
                    }
                }
            }
            let lvl = zone.foe_level.0 + rng.below((zone.foe_level.1 - zone.foe_level.0 + 1) as u64) as u32;
            let name = *rng.pick(&zone.foes);
            let b = battle(&me, &Fighter::monster(name, lvl), rng.next());
            me.hp = b.hp[0];
            rep.fights.push((name.to_string(), lvl, b.winner == 0));
            if b.winner != 0 {
                rep.fled = true;
                break;
            }
            rep.xp += 6 + 3 * lvl;
            rep.gold += 2 + lvl + rng.below(lvl as u64 + 1) as u32;
            if rng.chance(30 + loot_bonus) {
                rep.loot.push(roll_loot(zone, &mut rng));
            }
        }
        if !rep.fled {
            rep.gold += fights as u32 * 3; // treasure chest
            rep.loot.push(roll_loot(zone, &mut rng));
        } else {
            self.mood -= 10.0;
        }
        self.hp = me.hp.max(1) as f32;
        self.gold += rep.gold;
        for &it in &rep.loot {
            self.give(it, 1);
        }
        let xp = rep.xp as f32 * self.xp_mult();
        rep.xp = xp.round() as u32;
        self.gain_xp(xp, ev);
        self.clamp();
        rep
    }

    /// Short status for the LAN list, e.g. "Lifting", "Hungry" or "Chilling".
    pub fn status(&self) -> String {
        match (self.task, self.need()) {
            (Some(t), _) => t.job.label(),
            (None, Some(n)) => n.label().into(),
            (None, None) => "Chilling".into(),
        }
    }
}

fn roll_loot(zone: &Zone, rng: &mut Rng) -> Item {
    let total: u32 = zone.loot.iter().map(|l| l.1).sum();
    let mut r = rng.below(total as u64) as u32;
    for &(it, w) in zone.loot {
        if r < w {
            return it;
        }
        r -= w;
    }
    zone.loot[0].0
}

/// Strip control chars and cap length (for names coming from the user or the network).
pub fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

pub fn random_name(rng: &mut Rng) -> String {
    const A: [&str; 16] = ["Mo", "Pi", "Bo", "Ku", "Ta", "Nu", "Zi", "Lu", "Po", "Ri", "Ga", "Fu", "Chi", "Ba", "Mi", "To"];
    const B: [&str; 16] = ["chi", "po", "bo", "ki", "mo", "ru", "zu", "pi", "ko", "nya", "bun", "dle", "tsu", "ppy", "xel", "mi"];
    format!("{}{}", rng.pick(&A), rng.pick(&B))
}

// ------------------------------------------------------------------------------------------ save

#[derive(Serialize, Deserialize)]
pub struct Save {
    pub id: u64,
    pub pet: Option<Pet>,
    /// The pet that died of old age, until the next egg inherits its things.
    #[serde(default)]
    pub late: Option<Pet>,
    /// When the window closes the pet walks the desktop instead of hiding in the tray.
    #[serde(default)]
    pub out: bool,
    /// Where on the screen (y, points) the desktop pet walks: wherever it was last dropped.
    #[serde(default)]
    pub floor: Option<f32>,
    /// Where the pet was last, and its feet there (place pixels), so it's there after a restart.
    #[serde(default)]
    pub loc: Option<Loc>,
    #[serde(default)]
    pub spot: Option<[f32; 2]>,
}

impl Save {
    /// `LANPET_SAVE` overrides the location (handy for running two pets on one machine).
    pub fn path() -> PathBuf {
        match std::env::var_os("LANPET_SAVE") {
            Some(p) => p.into(),
            None => Save::dir().join("save.json"),
        }
    }

    /// LanPet's own folder in the user's data directory.
    pub fn dir() -> PathBuf {
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        };
        base.unwrap_or_default().join("lanpet")
    }

    pub fn load(path: &Path) -> Save {
        let fresh = || Save { id: Rng::seeded().next(), pet: None, late: None, out: false, floor: None, loc: None, spot: None };
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                // Never silently overwrite a save we can't read.
                let bad = path.with_extension("json.bad");
                eprintln!("lanpet: save unreadable ({e}); moved to {}", bad.display());
                let _ = std::fs::rename(path, bad);
                fresh()
            }),
            Err(_) => fresh(),
        }
    }

    /// Write-then-rename so a crash mid-save can't corrupt the pet.
    pub fn store(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battle_is_deterministic_and_ends() {
        let a = Pet::new("A".into(), Species::Dino, 0).fighter(true);
        let b = Pet::new("B".into(), Species::Frog, 0).fighter(true);
        let (x, y) = (battle(&a, &b, 42), battle(&a, &b, 42));
        assert_eq!(x.hits, y.hits);
        assert_eq!(x.winner, y.winner);
        assert!(!x.hits.is_empty() && x.hits.len() <= 60);
    }

    #[test]
    fn offline_progress_finishes_jobs_and_levels_up() {
        let mut p = Pet::new("A".into(), Species::Wolf, 1000);
        p.mood = 100.0;
        p.start(Job::Lift, 1000, 1).unwrap();
        let mut ev = Vec::new();
        p.tick(1000 + 3 * 3600, &mut ev); // app closed for 3 hours
        assert!(p.task.is_none());
        assert!((p.str - 12.0).abs() < 0.01, "str {}", p.str); // 8 base + 3 lifting + 1 level-up
        assert!(ev.iter().any(|e| matches!(e, Event::Done(Job::Lift))));
        assert!(ev.iter().any(|e| matches!(e, Event::LevelUp(2))));
    }

    #[test]
    fn jobs_pay_as_they_go() {
        let mut p = Pet::new("A".into(), Species::Frog, 0);
        let mana = p.mana;
        p.start(Job::Study, 0, 1).unwrap();
        let mut ev = Vec::new();
        p.tick(300, &mut ev); // half a study session
        assert_eq!(ev.iter().filter(|e| matches!(e, Event::Paid(Job::Study))).count(), 5);
        assert!((p.mana - mana - 1.5).abs() < 0.01, "half the mana: {}", p.mana - mana);
        p.stop(300, &mut ev);
        assert!(p.task.is_none());
        assert!(ev.iter().any(|e| matches!(e, Event::Stopped { job: Job::Study, secs: 300, why: None })));
        assert!((p.mana - mana - 1.5).abs() < 0.01, "stopping keeps it");

        // a little energy is enough to farm a little; the job ends itself when it runs out
        let (str0, mut ev) = (p.str, Vec::new());
        p.energy = 6.0;
        p.start(Job::Lift, 300, 2).unwrap();
        p.tick(900, &mut ev);
        let Some(Event::Stopped { secs, why: Some(_), .. }) = ev.iter().find(|e| matches!(e, Event::Stopped { .. })) else { panic!("no auto-stop") };
        assert!((170..=240).contains(secs), "6 energy lifts for ~3 min, got {secs}s");
        assert!(p.str > str0 && p.str < str0 + 1.5);
        // expeditions still want all their energy up front
        assert!(p.start(Job::Explore(0), 900, 3).is_err());
    }

    #[test]
    fn fresh_pet_usually_clears_the_meadow() {
        let rates: Vec<(Species, u32)> = Species::ALL
            .iter()
            .map(|&s| {
                let wins = (0..200)
                    .filter(|&seed| {
                        let mut p = Pet::new("A".into(), s, 0);
                        p.age = Stage::Baby.ends(); // babies stay home
                        p.start(Job::Explore(0), 0, seed).unwrap();
                        let mut ev = Vec::new();
                        p.tick(10_000, &mut ev);
                        let Some(Event::Back(rep)) = ev.into_iter().find(|e| matches!(e, Event::Back(_))) else { panic!() };
                        assert!(rep.fled || (rep.gold > 0 && !rep.loot.is_empty()));
                        !rep.fled
                    })
                    .count() as u32;
                (s, wins / 2)
            })
            .collect();
        assert!(rates.iter().all(|r| (75..=99).contains(&r.1)), "meadow win % per species: {rates:?}");
    }

    #[test]
    fn pets_grow_up_need_care_and_die_of_old_age() {
        let mut p = Pet::new("A".into(), Species::Dino, 0);
        assert_eq!(p.stage(), Stage::Baby);
        assert!(p.start(Job::Explore(0), 0, 1).is_err(), "babies stay home");
        // app open, nobody looking after it: it asks for water, then falls sick
        let (mut ev, mut now) = (Vec::new(), 0);
        while !p.sick {
            now += 60;
            p.tick(now, &mut ev);
            assert!(now < DAY, "neglect never made it sick");
            assert!(p.thirst > 5.0 || p.need() == Some(Need::Thirsty) || p.sick);
        }
        assert!(ev.iter().any(|e| matches!(e, Event::Sick)));
        assert_eq!(p.need(), Some(Need::Sick));
        assert!(p.start(Job::Lift, now, 1).is_err(), "too sick to train");
        p.give(Item::Medicine, 1);
        p.use_item(Item::Medicine).unwrap();
        p.drink();
        assert!(!p.sick && p.need() != Some(Need::Thirsty));
        // it grows up while the app is closed, stronger for the care it got
        let str0 = p.str;
        p.tick(now + DAY, &mut ev);
        assert_eq!(p.stage(), Stage::Teen);
        assert!(p.str > str0 && p.str < str0 + 2.0, "a neglected baby gets a small spurt: {}", p.str - str0);
        p.tick(now + LIFE, &mut ev);
        let grew: Vec<Stage> = ev.iter().filter_map(|e| if let Event::Grew(s) = e { Some(*s) } else { None }).collect();
        assert_eq!(grew, [Stage::Teen, Stage::Adult, Stage::Elder]);
        assert!(!p.alive() && matches!(ev.last(), Some(Event::Died)));
        // a closed app never makes a pet sick, however long it's away
        let mut q = Pet::new("B".into(), Species::Frog, 0);
        q.tick(10 * DAY, &mut ev);
        assert!(!q.sick && q.stage() == Stage::Adult);
        // the next egg gets the old pet's things
        p.weapon = Some(Item::IronSword);
        let gold = p.gold + q.gold;
        q.inherit(p);
        assert_eq!((q.gold, q.bag.get(&Item::IronSword)), (gold, Some(&1)));
    }

    #[test]
    fn save_round_trips() {
        let s = Save { id: 7, pet: Some(Pet::new("Mochi".into(), Species::Monkey, 5)), late: None, out: true, floor: Some(900.0), loc: Some(Loc::Gym), spot: Some([5.0, 6.0]) };
        let back: Save = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back.pet.as_ref().unwrap().bag.get(&Item::Apple), Some(&3));
        assert!(back.out && back.floor == Some(900.0) && back.loc == Some(Loc::Gym) && back.spot == Some([5.0, 6.0]));
        // saves from before the tray still load (they carried a window position), and so do ones
        // from before the walkable world (a room number): the pet starts out in town
        let old: Save = serde_json::from_str(r#"{"id":7,"pet":null,"pos":[1.0,2.0],"room":3}"#).unwrap();
        assert_eq!((old.id, old.loc, old.spot), (7, None, None));
        // pets from before life stages load as young adults
        let mut pet = serde_json::to_value(back.pet.unwrap()).unwrap();
        for k in ["age", "thirst", "sick", "strain", "care"] {
            pet.as_object_mut().unwrap().remove(k);
        }
        let pet: Pet = serde_json::from_value(pet).unwrap();
        assert_eq!((pet.stage(), pet.thirst, pet.sick), (Stage::Adult, 70.0, false));
    }
}
