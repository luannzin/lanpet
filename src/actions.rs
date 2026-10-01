//! What you can do right now: the popover's buttons, the expanded list, clickable furniture,
//! and the one-line status under the place's name.

use crate::game::{DAY, Furni, Item, Job, Need, Pet, Slot, Stage, ZONES, now};
use crate::world::{Door, Loc};
use crate::{Act, App, Tab, View};

/// One thing you can do: a button in the popover, a row in the expanded list.
pub(crate) struct Btn {
    pub acts: Vec<Act>,
    pub label: String,
    pub sub: String,
    /// Short verb for the row's action chip.
    pub go: String,
    pub item: Option<Item>,
    pub on: bool,
    /// Task end (unix secs): the sub line counts down to it.
    pub timer: Option<u64>,
    pub primary: bool,
    more: bool,
    /// Stop / decline: never the highlighted button.
    quiet: bool,
}

impl Btn {
    fn new(label: impl Into<String>, sub: impl Into<String>, go: impl Into<String>, acts: Vec<Act>) -> Btn {
        Btn { acts, label: label.into(), sub: sub.into(), go: go.into(), item: None, on: true, timer: None, primary: false, more: false, quiet: false }
    }
    fn item(mut self, it: Item) -> Btn {
        self.item = Some(it);
        self
    }
    fn off_if(mut self, off: bool) -> Btn {
        self.on &= !off;
        self
    }
    fn timer(mut self, end: u64) -> Btn {
        self.timer = Some(end);
        self
    }
    fn quiet(mut self) -> Btn {
        self.quiet = true;
        self
    }
    pub fn sub_now(&self) -> String {
        match self.timer {
            Some(end) => format!("{} · {}", self.sub, mmss(end.saturating_sub(now()))),
            None => self.sub.clone(),
        }
    }
}

/// The popover shows at most three buttons (two and a "More" when there are more), and the first
/// one worth pressing is green.
fn finish(mut v: Vec<Btn>, full: bool) -> Vec<Btn> {
    if !full && v.len() > 3 {
        let n = v.len() - 2;
        v.truncate(2);
        let mut more = Btn::new("More", format!("{n} more here"), "Open", vec![Act::View(View::Expanded), Act::Tab(Tab::Here)]);
        more.more = true;
        v.push(more);
    }
    if let Some(b) = v.iter_mut().find(|b| b.on && !b.more && !b.quiet) {
        b.primary = true;
    }
    v
}

/// What `secs` of a job earns, as ("+18 XP", "+1.2 Mana"); the XP part is empty for sleep.
pub(crate) fn earned(pet: &Pet, job: Job, secs: u64) -> (String, String) {
    let (xp, gain, what) = pet.job_gain(job);
    let k = secs.min(job.secs()) as f32 / job.secs() as f32;
    let (xp, gain) = (xp * k, gain * k);
    let gain = if gain >= 10.0 { format!("+{gain:.0} {what}") } else { format!("+{gain:.1} {what}") };
    (if xp > 0.0 { format!("+{xp:.0} XP") } else { String::new() }, gain)
}

fn mmss(s: u64) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

/// Roughly how long `secs` is: "3d", "5h", "20 min".
fn span(secs: u64) -> String {
    if secs >= DAY {
        format!("{}d", secs / DAY)
    } else if secs >= 3600 {
        format!("{}h", secs / 3600)
    } else {
        format!("{} min", secs.div_ceil(60))
    }
}

/// Where the pet is in its life and how well it's being kept, e.g. "Grows into a teen in 5h · Care 4/5".
pub(crate) fn life_line(pet: &Pet) -> String {
    let stage = pet.stage();
    let left = span(stage.ends().saturating_sub(pet.age));
    let next = match stage {
        Stage::Baby => format!("Grows into a teen in {left}"),
        Stage::Teen => format!("Grown up in {left}"),
        Stage::Adult => format!("Grows old in {left}"),
        Stage::Elder => format!("{left} left to enjoy together"),
    };
    format!("{next} · Care {:.0}/5", pet.care * 5.0)
}

fn job_verb(j: Job) -> &'static str {
    match j {
        Job::Study => "Study",
        Job::Lift => "Lift",
        Job::Run => "Run",
        Job::Sleep => "Sleep",
        Job::Explore(_) => "Explore",
    }
}

/// What eating/drinking an item does, in two short parts.
fn fx_text(it: Item) -> String {
    let f = it.info().fx;
    if f.hp >= 999.0 {
        return "Full heal".into();
    }
    let parts: Vec<String> = [(f.hunger, "Food"), (f.thirst, "Water"), (f.energy, "Energy"), (f.hp, "HP"), (f.mood, "Mood")]
        .into_iter()
        .filter(|(v, _)| *v > 0.0)
        .take(2)
        .map(|(v, l)| format!("+{v:.0} {l}"))
        .collect();
    if parts.is_empty() { it.info().desc.into() } else { parts.join(" · ") }
}

impl App {
    /// One line under the room name: what's going on here.
    pub(crate) fn status_line(&self) -> String {
        let Some(pet) = &self.save.pet else { return String::new() };
        if let Some(f) = &self.fight {
            return format!("Battle: {} vs {}", f.f[f.me].name, f.f[1 - f.me].name);
        }
        if let Some(n) = pet.need() {
            return format!("{} is {} · {}", pet.name, n.label().to_lowercase(), n.fix());
        }
        if let Some(task) = pet.task {
            let left = mmss(task.end.saturating_sub(now()));
            return match task.job {
                Job::Explore(z) => format!("{} is exploring {} · {left}", pet.name, ZONES[z as usize].name),
                j => format!("{} is {} · {left}", pet.name, j.label().to_lowercase()),
            };
        }
        if let Some(goal) = self.m.goal {
            let to = match goal {
                Loc::Home(o) if o == self.save.id => "home".to_string(),
                l if l.in_block() => self.place_name(l),
                l => format!("the {}", l.name()),
            };
            return format!("{} is on the way to {to}", pet.name);
        }
        let names: Vec<&str> = self.peers_in(self.view_loc()).map(|p| p.card.name.as_str()).collect();
        match names.as_slice() {
            [] => format!("{} is hanging out", pet.name),
            [a] => format!("{a} is here"),
            [a, b] => format!("{a} & {b} are here"),
            more => format!("{} pets are here", more.len()),
        }
    }

    pub(crate) fn tray_tip(&self) -> String {
        let Some(pet) = &self.save.pet else { return "LanPet".into() };
        if let Some(inc) = &self.incoming {
            return format!("LanPet · {} wants to battle!", inc.card.name);
        }
        if let Some(n) = pet.need() {
            return format!("LanPet · {} is {}", pet.name, n.label().to_lowercase());
        }
        match pet.task {
            Some(task) => {
                let mins = task.end.saturating_sub(now()).div_ceil(60);
                format!("LanPet · {} · {} ({mins} min left)", pet.name, task.job.label())
            }
            None => format!("LanPet · {} is {}", pet.name, self.loc.at()),
        }
    }

    /// Everything you can do right now, most urgent first. `full` = the expanded list.
    pub(crate) fn actions(&self, full: bool, t: f64) -> Vec<Btn> {
        let Some(pet) = &self.save.pet else { return Vec::new() };
        let mut v = Vec::new();
        if let Some(hand) = self.deco {
            v.push(Btn::new("Done", "Finish decorating", "Done", vec![Act::Decorate(false)]));
            if let Some(f) = hand {
                v.push(Btn::new(format!("Put away the {}", f.info().0.to_lowercase()), "Back into storage", "Store", vec![Act::PutAway]));
            }
            for (&f, &n) in &self.save.home.stored {
                v.push(Btn::new(format!("{} ×{n}", f.info().0), "Pick it up, then click the floor", "Hold", vec![Act::Hold(f)]));
            }
            if hand.is_none() && self.save.home.stored.is_empty() {
                v.push(Btn::new("Nothing put away", "Click furniture to move it · the Shop sells more", "…", vec![]).off_if(true));
            }
            return finish(v, full);
        }
        if let Some(f) = &self.fight {
            if f.over {
                v.push(Btn::new("Back", f.reward.clone(), "Back", vec![Act::CloseFight]));
            } else {
                v.push(Btn::new("Battle on", format!("vs {}", f.f[1 - f.me].name), "…", vec![]).off_if(true));
            }
            return finish(v, full);
        }
        if let Some(inc) = &self.incoming {
            let secs = (inc.until - t).max(0.0) as u32;
            v.push(Btn::new("Fight!", format!("{} · Lv {} {} · {secs}s", inc.card.name, inc.card.level, inc.card.species.name()), "Fight", vec![Act::Accept]));
            v.push(Btn::new("Not now", "Decline the battle", "No", vec![Act::Decline]).quiet());
            return finish(v, full);
        }
        if let Some(rep) = &self.report {
            let zone = ZONES[rep.zone as usize].name;
            let sub = if rep.fled { format!("Fled from {zone} · +{} XP", rep.xp) } else { format!("+{} XP · +{} gold · {} loot", rep.xp, rep.gold, rep.loot.len()) };
            v.push(Btn::new("Nice!", sub, "OK", vec![Act::CloseReport]));
            if !full {
                return finish(v, full);
            }
        }
        if let Some(p) = &self.pending {
            let name = self.peers.get(&p.peer).map_or("them", |x| x.card.name.as_str());
            v.push(Btn::new(format!("Waiting for {name}"), format!("{}s to accept", (p.until - t).max(0.0) as u32), "…", vec![]).off_if(true));
        }
        let can_fight = self.pending.is_none() && self.pet_loc().is_some();
        if let Some(peer) = self.selected.and_then(|id| self.peers.get(&id)) {
            let (id, c) = (peer.card.id, &peer.card);
            v.push(Btn::new(format!("Battle {}", c.name), format!("Lv {} {} · {}W / {}L", c.level, c.species.name(), c.wins, c.losses), "Battle", vec![Act::Challenge(id)]).off_if(!can_fight));
            v.push(Btn::new("Wave", format!("Say hi to {}", c.name), "Wave", vec![Act::Wave(id)]));
            if full {
                for (&it, &n) in &pet.bag {
                    v.push(Btn::new(format!("Gift {}", it.info().name), format!("×{n} in your bag"), "Gift", vec![Act::Gift(id, it)]).item(it));
                }
            } else {
                v.push(Btn::new("Gift", "Send something from your bag", "Gift", vec![Act::View(View::Expanded), Act::Tab(Tab::Here)]).off_if(pet.bag.is_empty()));
            }
            return finish(v, full);
        }
        // what the pet needs comes first: the cure if it's at hand, else the way to the room that has it
        match pet.need() {
            Some(Need::Sick) if pet.bag.contains_key(&Item::Medicine) => {
                v.push(Btn::new("Give medicine", "Cures the sickness", "Give", vec![Act::Use(Item::Medicine)]).item(Item::Medicine));
            }
            Some(n) if self.need_loc(n) != self.view_loc() => {
                let to = match self.need_loc(n) {
                    Loc::Home(_) => "Head home".to_string(),
                    l => format!("To the {}", l.name()),
                };
                v.push(Btn::new(format!("{}!", n.label()), to, "Go", vec![Act::Go(self.need_loc(n))]));
            }
            _ => {}
        }
        let busy = pet.task.is_some();
        let job_here = pet.task.filter(|t| !matches!(t.job, Job::Explore(_)) && self.job_loc(t.job) == self.view_loc());
        if let Some(task) = job_here {
            let (xp, gain) = earned(pet, task.job, now().saturating_sub(task.start));
            let so_far = if xp.is_empty() { format!("{gain} so far") } else { format!("{gain} · {xp} so far") };
            v.push(Btn::new("Stop", so_far, "Stop", vec![Act::Stop]).timer(task.end).quiet());
        }
        let job = |j: Job, sub: &str| {
            let sub = if busy { format!("{} is busy", pet.name) } else { sub.to_string() };
            Btn::new(job_verb(j), sub, "Start", vec![Act::Start(j)]).off_if(busy)
        };
        match self.view_loc() {
            Loc::Town => {
                for loc in Loc::buildings(self.save.id) {
                    let sub = match loc {
                        Loc::Home(_) => "Sleep, eat and drink",
                        Loc::Library => "Study · +Mana",
                        Loc::Gym => "Lift and run · +Str, +HP",
                        Loc::Shop => "Food, gear and hats",
                        Loc::Portal => "Expeditions and loot",
                        _ => "Battle coworkers' pets",
                    };
                    v.push(Btn::new(loc.name(), sub, "Go", vec![Act::Go(loc)]));
                }
            }
            Loc::Home(o) if o != self.save.id => {
                v.push(Btn::new("Go home", "Back to your own place", "Go", vec![Act::Go(Loc::Home(self.save.id))]));
                v.push(Btn::new("Water", "+40 Water · free", "Drink", vec![Act::Drink]));
                v.push(Btn::new("Pet", format!("{} loves it · +Mood", pet.name), "Pet", vec![Act::PetIt]));
            }
            Loc::Lobby | Loc::Floor(_) => {
                let (here, me) = (self.view_loc(), self.save.id);
                if let Some((floor, _)) = self.apartment(me) {
                    v.push(Btn::new("Go home", format!("Floor {floor} · your place"), "Go", vec![Act::Go(Loc::Home(me))]));
                }
                let floors = self.residents().len().div_ceil(4) as u8;
                for n in (1..=floors).filter(|&n| here != Loc::Floor(n)) {
                    let names: Vec<String> = (0..4).filter_map(|i| self.resident(n, i)).map(|o| if o == me { "you".into() } else { self.pet_name(o).unwrap_or_default() }).collect();
                    v.push(Btn::new(format!("Floor {n}"), names.join(", "), "Ride", vec![Act::Go(Loc::Floor(n))]));
                }
                if here == Loc::Lobby {
                    v.push(Btn::new("Town", "Back outside", "Go", vec![Act::Go(Loc::Town)]));
                } else {
                    v.push(Btn::new("Lobby", "Ground floor · the way out", "Ride", vec![Act::Go(Loc::Lobby)]));
                }
            }
            Loc::Home(_) => {
                if let Some((ver, _)) = &self.update.ready {
                    let sub = if self.update.installing { "Installing…".to_string() } else { format!("{ver} is ready · LanPet restarts") };
                    v.push(Btn::new("Update LanPet", sub, "Update", vec![Act::Update]).off_if(self.update.installing));
                }
                let first = v.len();
                if job_here.is_none() {
                    v.push(job(Job::Sleep, "20 min · +Energy, heals HP"));
                }
                v.push(Btn::new("Water", "+40 Water · free", "Drink", vec![Act::Drink]));
                let food: Vec<(Item, u32)> = pet.bag.iter().filter(|(i, _)| matches!(i.info().slot, Slot::Food | Slot::Drink)).map(|(&i, &n)| (i, n)).collect();
                for &(it, n) in &food {
                    let verb = if it.info().slot == Slot::Drink { "Drink" } else { "Eat" };
                    v.push(Btn::new(format!("{} ×{n}", it.info().name), fx_text(it), verb, vec![Act::Use(it)]).item(it));
                }
                if food.is_empty() || full {
                    let price = Item::Apple.info().price;
                    v.push(Btn::new("Quick apple", format!("Buy & eat · {price} gold"), format!("{price} g"), vec![Act::Buy(Item::Apple), Act::Use(Item::Apple)]).item(Item::Apple).off_if(pet.gold < price));
                }
                let n: u32 = pet.bag.values().sum();
                v.push(Btn::new("Pet", format!("{} loves it · +Mood", pet.name), "Pet", vec![Act::PetIt]));
                v.push(Btn::new("Decorate", "Move furniture, put it away or out", "Decorate", vec![Act::Decorate(true)]));
                v.push(Btn::new("Bag", format!("{n} item{}", if n == 1 { "" } else { "s" }), "Open", vec![Act::View(View::Expanded), Act::Tab(Tab::Bag)]));
                // what the pet needs most goes first
                let urgent = |b: &Btn| {
                    b.acts.iter().any(|a| match pet.need() {
                        Some(Need::Tired) => matches!(a, Act::Start(Job::Sleep)),
                        Some(Need::Thirsty) => matches!(a, Act::Drink),
                        Some(Need::Hungry) => matches!(a, Act::Use(i) if i.info().slot == Slot::Food),
                        Some(Need::Sad) => matches!(a, Act::PetIt),
                        _ => false,
                    })
                };
                if let Some(i) = v[first..].iter().position(urgent) {
                    let b = v.remove(first + i);
                    v.insert(first, b);
                }
            }
            Loc::Library if job_here.is_none() => v.push(job(Job::Study, "10 min · +3 Mana")),
            Loc::Gym if job_here.is_none() => {
                v.push(job(Job::Lift, "10 min · +3 Str"));
                v.push(job(Job::Run, "10 min · +8 HP, +Spd"));
            }
            Loc::Portal => match pet.task {
                Some(task) if matches!(task.job, Job::Explore(_)) => {
                    v.push(Btn::new(task.job.label(), "Out adventuring", "Away", vec![]).timer(task.end).off_if(true));
                }
                _ => {
                    let baby = pet.stage() == Stage::Baby;
                    for (i, z) in ZONES.iter().enumerate() {
                        let locked = pet.level < z.min_level;
                        let sub = if baby {
                            "Too little to explore. Babies stay home".into()
                        } else if locked {
                            format!("Unlocks at Lv {}", z.min_level)
                        } else {
                            format!("{} min · foes Lv {}–{}", z.mins, z.foe_level.0, z.foe_level.1)
                        };
                        let go = if baby { "Teen".into() } else if locked { format!("Lv {}", z.min_level) } else { "Go".into() };
                        v.push(Btn::new(z.name, sub, go, vec![Act::Start(Job::Explore(i as u8))]).off_if(baby || locked || busy));
                    }
                }
            },
            Loc::Arena => {
                if let Some(e) = &self.net_err {
                    v.push(Btn::new("LAN offline", e.clone(), "…", vec![]).off_if(true));
                } else if self.peers.is_empty() {
                    v.push(Btn::new("Looking for pets", "Run LanPet on a coworker's PC on the same network", "…", vec![]).off_if(true));
                }
                for peer in self.peers.values() {
                    let c = &peer.card;
                    let at = c.loc.map_or("away".to_string(), |l| l.at());
                    v.push(Btn::new(format!("Battle {}", c.name), format!("Lv {} {} · {at}", c.level, c.species.name()), "Battle", vec![Act::Challenge(c.id)]).off_if(!can_fight));
                    if full {
                        v.push(Btn::new(format!("Wave at {}", c.name), "Say hi across the LAN", "Wave", vec![Act::Wave(c.id)]));
                    }
                }
            }
            Loc::Shop => {
                let featured = if pet.sick { [Item::Medicine, Item::Coffee, Item::Cake] } else { [Item::Coffee, Item::Cake, Item::Potion] };
                let items: Vec<Item> = if full { Item::ALL.into_iter().filter(|i| i.info().price > 0).collect() } else { featured.to_vec() };
                for it in items {
                    let price = it.info().price;
                    let sub = if matches!(it.info().slot, Slot::Food | Slot::Drink) { fx_text(it) } else { it.info().desc.into() };
                    v.push(Btn::new(it.info().name, sub, format!("{price} g"), vec![Act::Buy(it)]).item(it).off_if(pet.gold < price));
                }
                if full {
                    for f in Furni::ALL {
                        let (name, price) = f.info();
                        v.push(Btn::new(name, "Furniture · goes into storage at home", format!("{price} g"), vec![Act::BuyFurni(f)]).off_if(pet.gold < price));
                    }
                } else {
                    v.push(Btn::new("Furniture", "Beds, sofas, plants, an arcade…", "Browse", vec![Act::View(View::Expanded), Act::Tab(Tab::Here)]));
                }
            }
            _ => {}
        }
        finish(v, full)
    }

    /// What going through a door does, for its hover label.
    pub(crate) fn door_label(&self, d: &Door) -> String {
        match self.through(d) {
            Some((Loc::Town, _)) => "Out to town".into(),
            Some((Loc::Home(o), _)) if o == self.save.id => "Go home".into(),
            Some((Loc::Home(o), _)) => format!("Visit {}", self.pet_name(o).unwrap_or_default()),
            Some((Loc::Floor(n), _)) => format!("Out to floor {n}"),
            Some((Loc::Lobby, _)) if self.loc != Loc::Town => "Out to the lobby".into(),
            Some((l, _)) => format!("Enter the {}", l.name()),
            None => "Nobody lives here yet".into(),
        }
    }

    /// Props with an `act` (see `put` in gen_assets.py): the hover label and what clicking does.
    pub(crate) fn hot_act(&self, act: &str) -> (String, Vec<Act>) {
        let pet = self.save.pet.as_ref();
        let visiting = matches!(self.view_loc(), Loc::Home(o) if o != self.save.id);
        match act {
            "sleep" if visiting => ("Not your bed".into(), vec![]),
            "sleep" => ("Sleep · 20 min".into(), vec![Act::Start(Job::Sleep)]),
            "elevator" => ("Elevator · pick a floor".into(), vec![Act::View(View::Expanded), Act::Tab(Tab::Here)]),
            "study" => ("Study · 10 min".into(), vec![Act::Start(Job::Study)]),
            "lift" => ("Lift weights · 10 min".into(), vec![Act::Start(Job::Lift)]),
            "run" => ("Run · 10 min".into(), vec![Act::Start(Job::Run)]),
            "feed" => match pet.and_then(|p| p.bag.keys().copied().find(|i| matches!(i.info().slot, Slot::Food | Slot::Drink))) {
                Some(it) => (format!("Have the {}", it.info().name), vec![Act::Use(it)]),
                None => (format!("Buy & eat an apple · {} gold", Item::Apple.info().price), vec![Act::Buy(Item::Apple), Act::Use(Item::Apple)]),
            },
            "drink" => ("Drink water".into(), vec![Act::Drink]),
            "explore" => (format!("Explore {} · {} min", ZONES[0].name, ZONES[0].mins), vec![Act::Start(Job::Explore(0))]),
            "challenge" => match self.peers.values().next() {
                Some(p) => (format!("Battle {}", p.card.name), vec![Act::Challenge(p.card.id)]),
                None => ("Nobody to battle yet".into(), vec![]),
            },
            "browse" => ("See everything for sale".into(), vec![Act::View(View::Expanded), Act::Tab(Tab::Here)]),
            // a building: walking up to its door goes in
            a => match a.strip_prefix("go:").and_then(|k| Loc::from_key(k, self.save.id)) {
                Some(Loc::Town) => ("Out to town".into(), vec![]),
                Some(Loc::Home(_)) => ("Go home".into(), vec![]),
                Some(l) => (format!("Enter the {}", l.name()), vec![]),
                None => (String::new(), vec![]),
            },
        }
    }
}
