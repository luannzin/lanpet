//! The world: places on a 16 px tile grid (the town and the inside of its buildings), what stands
//! in them, and getting around (paths over the walkable tiles, doors between places).
//! Layouts come from `tools/gen_assets.py` through assets/meta.json.

use crate::game::{Furni, Placed};
use eframe::egui::{Pos2, Rect, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

pub const TILE: f32 = 16.0;
/// The most pieces a home holds (and a peer may send us).
pub const MAX_PIECES: usize = 80;

/// How a piece of furniture looks and sits (meta.json `furniture`).
#[derive(Clone, Copy, Deserialize)]
pub struct FurniArt {
    pub sprite: usize,
    /// Footprint in tiles.
    pub size: [i32; 2],
    pub low: bool,
    /// Blocks walking (a rug doesn't).
    pub solid: bool,
    /// Where a pet stands to use it, from its feet.
    pub stand: [f32; 2],
}

/// Every place, with the homes furnished from their owners' layouts.
pub struct World {
    places: Vec<Place>,
    homes: HashMap<u64, Place>,
    furni: HashMap<Furni, FurniArt>,
    /// Prop sprite sizes, for clickable areas.
    sizes: Vec<Vec2>,
}

impl World {
    pub fn new(places: Vec<Place>, furni: HashMap<Furni, FurniArt>, sprites: &[[f32; 4]]) -> World {
        assert!(Furni::ALL.iter().all(|f| furni.contains_key(f)), "regenerate assets: furniture changed");
        World { places, homes: HashMap::new(), furni, sizes: sprites.iter().map(|s| vec2(s[2], s[3])).collect() }
    }

    pub fn place(&self, loc: Loc) -> &Place {
        if let Loc::Home(owner) = loc
            && let Some(home) = self.homes.get(&owner)
        {
            return home;
        }
        self.places.iter().find(|p| p.key == loc.key()).expect("every place is checked in Art::load")
    }

    pub fn art(&self, f: Furni) -> FurniArt {
        self.furni[&f]
    }

    /// A home with nothing in it.
    fn shell(&self) -> &Place {
        self.places.iter().find(|p| p.key == "home").expect("every place is checked in Art::load")
    }

    /// The tiles a piece covers.
    fn footprint(&self, p: Placed) -> impl Iterator<Item = (i32, i32)> {
        let [w, h] = self.art(p.f).size;
        (p.y..p.y + h).flat_map(move |y| (p.x..p.x + w).map(move |x| (x, y)))
    }

    /// An empty home with `pieces` in it: their props (in the same order), the floor they cover,
    /// and a bed's spot to sleep in.
    fn build(&self, pieces: &[Placed]) -> Place {
        let mut home = self.shell().clone();
        let mut beds = Vec::new();
        for &p in pieces {
            let a = self.art(p.f);
            let feet = pos2((p.x as f32 + a.size[0] as f32 / 2.0) * TILE, (p.y + a.size[1]) as f32 * TILE);
            let size = self.sizes[a.sprite];
            let stand = feet + Vec2::from(a.stand);
            if a.solid {
                for (x, y) in self.footprint(p) {
                    home.block(x, y);
                }
            }
            if p.f == Furni::Bed {
                beds.push([stand.x, stand.y]);
            }
            home.props.push(Prop {
                sprite: a.sprite,
                feet: [feet.x, feet.y],
                low: a.low,
                hot: Some([feet.x - size.x / 2.0, feet.y - size.y, size.x, size.y]),
                act: p.f.act().map(String::from),
                stand: Some([stand.x, stand.y]),
            });
        }
        home.slots.insert("sleep".into(), beds);
        home
    }

    /// Furnishes `owner`'s home as laid out.
    pub fn furnish(&mut self, owner: u64, pieces: &[Placed]) {
        let home = self.build(pieces);
        self.homes.insert(owner, home);
    }

    /// Whether `new` can go into a home laid out as `pieces`: on the floor, clear of the doormat,
    /// not on another solid piece, and leaving every bit of floor still reachable from the door.
    pub fn fits(&self, pieces: &[Placed], new: Placed) -> bool {
        let empty = self.shell();
        let door = tile(empty.spawn());
        if pieces.len() >= MAX_PIECES || !self.footprint(new).all(|(x, y)| empty.open(x, y) && (x, y) != door) {
            return false;
        }
        if !self.art(new.f).solid {
            return true;
        }
        let taken: Vec<(i32, i32)> = pieces.iter().filter(|p| self.art(p.f).solid).flat_map(|&p| self.footprint(p)).collect();
        if self.footprint(new).any(|t| taken.contains(&t)) {
            return false;
        }
        let mut after = pieces.to_vec();
        after.push(new);
        self.build(&after).all_reachable(empty.spawn())
    }
}

/// Keeps a layout from the network sane: so many pieces at most, on the home's grid.
pub fn sane_layout(pieces: &mut Vec<Placed>) {
    pieces.truncate(MAX_PIECES);
    for p in pieces.iter_mut() {
        (p.x, p.y) = (p.x.clamp(0, 31), p.y.clamp(0, 31));
    }
}

/// Where a pet is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Loc {
    Town,
    Library,
    Gym,
    Portal,
    Arena,
    Shop,
    /// Someone's home, by the owner's pet id.
    Home(u64),
    /// The apartment block's ground floor, and its floors above (from 1), four homes each.
    Lobby,
    Floor(u8),
}

impl Loc {
    /// The town's buildings, in the order the town's "go to" list shows them (yours first).
    pub fn buildings(me: u64) -> [Loc; 6] {
        [Loc::Home(me), Loc::Library, Loc::Gym, Loc::Shop, Loc::Portal, Loc::Arena]
    }

    /// Key in meta.json.
    pub fn key(self) -> &'static str {
        match self {
            Loc::Town => "town",
            Loc::Library => "library",
            Loc::Gym => "gym",
            Loc::Portal => "portal",
            Loc::Arena => "arena",
            Loc::Shop => "shop",
            Loc::Home(_) => "home",
            Loc::Lobby => "lobby",
            Loc::Floor(_) => "floor",
        }
    }

    pub fn name(self) -> String {
        match self {
            Loc::Town => "Town".into(),
            Loc::Library => "Library".into(),
            Loc::Gym => "Gym".into(),
            Loc::Portal => "Portal".into(),
            Loc::Arena => "Arena".into(),
            Loc::Shop => "Shop".into(),
            Loc::Home(_) => "Home".into(),
            Loc::Lobby => "Apartments".into(),
            Loc::Floor(n) => format!("Floor {n}"),
        }
    }

    /// "in town", "at home", "in the Gym", "on floor 2".
    pub fn at(self) -> String {
        match self {
            Loc::Town => "in town".into(),
            Loc::Home(_) => "at home".into(),
            Loc::Floor(n) => format!("on floor {n}"),
            l => format!("in the {}", l.name()),
        }
    }

    /// The apartment block: the lobby, its floors and the homes on them.
    pub fn in_block(self) -> bool {
        matches!(self, Loc::Lobby | Loc::Floor(_) | Loc::Home(_))
    }

    /// A fixed door's destination ("home" is always your own). The block's other doors depend on
    /// who's online: see `App::through`.
    pub fn from_key(key: &str, me: u64) -> Option<Loc> {
        Some(match key {
            "town" => Loc::Town,
            "library" => Loc::Library,
            "gym" => Loc::Gym,
            "portal" => Loc::Portal,
            "arena" => Loc::Arena,
            "shop" => Loc::Shop,
            "home" => Loc::Home(me),
            "lobby" => Loc::Lobby,
            _ => return None,
        })
    }
}

#[derive(Clone, Deserialize)]
pub struct Door {
    /// The door's tile: walking onto it at the end of a path goes through.
    pub at: [i32; 2],
    pub to: String,
    /// Where you come out on the other side (place pixels).
    pub arrive: [f32; 2],
}

impl Door {
    pub fn centre(&self) -> Pos2 {
        centre((self.at[0], self.at[1]))
    }
}

/// Something standing in a place: a building, a tree, a bed.
#[derive(Clone, Deserialize)]
pub struct Prop {
    pub sprite: usize,
    /// Bottom centre (place pixels); props sort with the pets by it.
    pub feet: [f32; 2],
    /// Flat things (beds, mats) drawn under every pet.
    pub low: bool,
    /// Clickable area and what clicking it does (see `App::hot_act`).
    pub hot: Option<[f32; 4]>,
    pub act: Option<String>,
    /// Where the pet goes to use it.
    pub stand: Option<[f32; 2]>,
}

impl Prop {
    pub fn hot_rect(&self) -> Option<Rect> {
        self.hot.map(|[x, y, w, h]| Rect::from_min_size(pos2(x, y), vec2(w, h)))
    }
}

#[derive(Clone, Deserialize)]
pub struct Place {
    pub key: String,
    /// In tiles.
    pub size: [i32; 2],
    /// Its ground in places.png, and its minimap in minimaps.png (pixels).
    pub img: [f32; 4],
    pub mini: [f32; 4],
    /// One string per row: '.' walkable, '#' not.
    walk: Vec<String>,
    /// Around the place when it's smaller than the view: the sea, or the dark outside a room.
    pub bg: [u8; 3],
    pub spawn: [f32; 2],
    pub doors: Vec<Door>,
    pub props: Vec<Prop>,
    /// Spots for jobs ("study", "sleep"...), the arena's "fight" spots and the "egg".
    #[serde(default)]
    pub slots: HashMap<String, Vec<[f32; 2]>>,
}

fn tile(p: Pos2) -> (i32, i32) {
    ((p.x / TILE).floor() as i32, (p.y / TILE).floor() as i32)
}

fn centre((x, y): (i32, i32)) -> Pos2 {
    pos2((x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE)
}

impl Place {
    /// Size in pixels.
    pub fn px(&self) -> Vec2 {
        vec2(self.size[0] as f32, self.size[1] as f32) * TILE
    }

    pub fn open(&self, x: i32, y: i32) -> bool {
        y >= 0 && x >= 0 && self.walk.get(y as usize).and_then(|r| r.as_bytes().get(x as usize)) == Some(&b'.')
    }

    pub fn open_at(&self, p: Pos2) -> bool {
        let (x, y) = tile(p);
        self.open(x, y)
    }

    fn block(&mut self, x: i32, y: i32) {
        if self.open(x, y) {
            self.walk[y as usize].replace_range(x as usize..x as usize + 1, "#");
        }
    }

    /// Every open tile can be walked to from `from`.
    fn all_reachable(&self, from: Pos2) -> bool {
        let open = self.walk.iter().map(|r| r.bytes().filter(|&b| b == b'.').count()).sum::<usize>();
        let mut seen = vec![tile(from)];
        let mut i = 0;
        while i < seen.len() {
            let (x, y) = seen[i];
            i += 1;
            for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if self.open(n.0, n.1) && !seen.contains(&n) {
                    seen.push(n);
                }
            }
        }
        seen.len() - usize::from(!self.open_at(from)) == open
    }

    pub fn door_at(&self, p: Pos2) -> Option<&Door> {
        let (x, y) = tile(p);
        self.doors.iter().find(|d| d.at == [x, y])
    }

    pub fn door_to(&self, key: &str) -> Option<&Door> {
        self.doors.iter().find(|d| d.to == key)
    }

    pub fn slots(&self, name: &str) -> &[[f32; 2]] {
        self.slots.get(name).map_or(&[], |v| v.as_slice())
    }

    pub fn spawn(&self) -> Pos2 {
        Pos2::from(self.spawn)
    }

    /// Keeps a point (say, from the network) inside the place.
    pub fn clamp(&self, p: Pos2) -> Pos2 {
        let s = self.px();
        pos2(p.x.clamp(0.0, s.x - 1.0), p.y.clamp(0.0, s.y - 1.0))
    }

    /// Open ground under a pet's feet, with a little room around them.
    fn roomy(&self, p: Pos2) -> bool {
        const R: f32 = 3.0;
        [vec2(0.0, 0.0), vec2(R, 0.0), vec2(-R, 0.0), vec2(0.0, R), vec2(0.0, -R)].iter().all(|&o| self.open_at(p + o))
    }

    /// A straight walk from `a` to `b` stays on open ground.
    fn clear(&self, a: Pos2, b: Pos2) -> bool {
        let n = (a.distance(b) / 4.0).ceil().max(1.0) as usize;
        (0..=n).all(|i| self.roomy(a.lerp(b, i as f32 / n as f32)))
    }

    /// Straightens a tile-by-tile walk: from each point, straight on to the farthest of `way[..n]`
    /// in clear sight. What comes after `n` is kept as it is.
    fn smooth(&self, from: Pos2, way: Vec<Pos2>, n: usize) -> Vec<Pos2> {
        let mut out = Vec::new();
        let (mut at, mut i) = (from, 0);
        while i < n {
            let mut j = i;
            while j + 1 < n && self.clear(at, way[j + 1]) {
                j += 1;
            }
            out.push(way[j]);
            at = way[j];
            i = j + 1;
        }
        out.extend_from_slice(&way[n..]);
        out
    }

    /// Waypoints from `from` to `to` over walkable tiles: the shortest 8-way route (never cutting a
    /// corner), straightened wherever the way is clear, ending exactly on `to`. A target on a
    /// blocked tile (a bed, a bench) is stepped onto from the open tile beside it. None when
    /// there's no way there.
    pub fn path(&self, from: Pos2, to: Pos2) -> Option<Vec<Pos2>> {
        let [w, h] = self.size;
        let inside = |(x, y): (i32, i32)| (0..w).contains(&x) && (0..h).contains(&y);
        let from = self.clamp(from);
        let (s, g) = (tile(from), tile(to));
        if !inside(g) {
            return None;
        }
        let goal = |t: (i32, i32)| if self.open(g.0, g.1) { t == g } else { (t.0 - g.0).abs() <= 1 && (t.1 - g.1).abs() <= 1 };
        let idx = |(x, y): (i32, i32)| (y * w + x) as usize;
        let mut prev: Vec<Option<(i32, i32)>> = vec![None; (w * h) as usize];
        prev[idx(s)] = Some(s);
        let mut queue = VecDeque::from([s]);
        while let Some(t) = queue.pop_front() {
            if goal(t) {
                let mut way = Vec::new();
                let mut at = t;
                while at != s {
                    way.push(centre(at));
                    at = prev[idx(at)]?;
                }
                way.reverse();
                if t == g {
                    way.pop(); // the point itself stands in for its tile's centre
                }
                way.push(to);
                let n = if t == g { way.len() } else { way.len() - 1 };
                return Some(self.smooth(from, way, n));
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let n = (t.0 + dx, t.1 + dy);
                let corner_ok = dx == 0 || dy == 0 || (self.open(t.0 + dx, t.1) && self.open(t.0, t.1 + dy));
                if inside(n) && self.open(n.0, n.1) && corner_ok && prev[idx(n)].is_none() {
                    prev[idx(n)] = Some(t);
                    queue.push_back(n);
                }
            }
        }
        None
    }
}

/// Floor (from 1) and door (0..4) of `id`'s home among the block's `residents` (sorted ids, four
/// homes a floor).
pub fn apartment(residents: &[u64], id: u64) -> Option<(u8, usize)> {
    let i = residents.iter().position(|&r| r == id)?;
    Some(((i / 4 + 1) as u8, i % 4))
}

/// Whose home is behind `door` (0..4) of `floor`.
pub fn resident(residents: &[u64], floor: u8, door: usize) -> Option<u64> {
    if door >= 4 {
        return None;
    }
    residents.get((floor as usize).checked_sub(1)? * 4 + door).copied()
}

/// One leg of a walk between places.
#[derive(Debug, PartialEq)]
pub enum Leg {
    /// Through the door whose `to` is this.
    Door(String),
    /// Up or down in the apartment block's elevator.
    Ride(Loc),
}

/// The next leg from `at` towards `goal` (somewhere else): out of a building to town and into the
/// next one; in the apartment block, by elevator to the right floor and in at the right front
/// door. `home` says where someone's home is while they're online. None when there's no way.
pub fn next_leg(at: Loc, goal: Loc, home: impl Fn(u64) -> Option<(u8, usize)>) -> Option<Leg> {
    let floor_of = |l: Loc| match l {
        Loc::Floor(n) => Some(n),
        Loc::Home(o) => home(o).map(|h| h.0),
        _ => None,
    };
    Some(match at {
        Loc::Town => Leg::Door(if goal.in_block() { "lobby" } else { goal.key() }.into()),
        Loc::Lobby if !goal.in_block() => Leg::Door("town".into()),
        Loc::Lobby => Leg::Ride(Loc::Floor(floor_of(goal)?)),
        Loc::Floor(n) => match (goal, floor_of(goal)) {
            (Loc::Home(o), Some(f)) if f == n => Leg::Door(format!("apt{}", home(o)?.1)),
            (_, Some(f)) => Leg::Ride(Loc::Floor(f)),
            _ => Leg::Ride(Loc::Lobby),
        },
        Loc::Home(_) => Leg::Door("floor".into()),
        _ => Leg::Door("town".into()),
    })
}

/// Walks `pos` along `path` at `speed` px/s, dropping waypoints as they're reached. Returns how far
/// it moved sideways (to face that way), or None when there was nowhere to go.
pub fn step(pos: &mut Pos2, path: &mut Vec<Pos2>, speed: f32, dt: f32) -> Option<f32> {
    if path.is_empty() {
        return None;
    }
    let start = pos.x;
    let mut left = speed * dt;
    while let Some(&next) = path.first() {
        let d = next - *pos;
        let len = d.length();
        if len <= left {
            *pos = next;
            left -= len;
            path.remove(0);
        } else {
            *pos += d / len * left;
            break;
        }
    }
    Some(pos.x - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(rows: &[&str]) -> Place {
        Place {
            key: "test".into(),
            size: [rows[0].len() as i32, rows.len() as i32],
            img: [0.0; 4],
            mini: [0.0; 4],
            walk: rows.iter().map(|r| r.to_string()).collect(),
            bg: [0; 3],
            spawn: [8.0, 8.0],
            doors: Vec::new(),
            props: Vec::new(),
            slots: HashMap::new(),
        }
    }

    fn at(x: i32, y: i32) -> Pos2 {
        centre((x, y))
    }

    #[test]
    fn paths_go_around_walls_in_straight_lines() {
        let p = place(&["......", ".###..", "......"]);
        let from = at(0, 1);
        let way = p.path(from, at(4, 1)).expect("there's a way round");
        assert_eq!(*way.last().unwrap(), at(4, 1));
        // every leg stays on open ground, corners included
        let mut a = from;
        for &b in &way {
            assert!((0..=40).all(|i| p.open_at(a.lerp(b, i as f32 / 40.0))), "{a:?} -> {b:?} crosses a wall");
            a = b;
        }
        // open ground is crossed in one straight line
        assert_eq!(place(&["....", "....", "...."]).path(at(0, 0), at(3, 2)).unwrap(), vec![at(3, 2)]);
        assert!(p.path(at(0, 0), pos2(-5.0, 3.0)).is_none());
        assert!(place(&["..#.."]).path(at(0, 0), at(4, 0)).is_none());
    }

    #[test]
    fn a_blocked_target_is_reached_from_beside_it() {
        let p = place(&["...", ".#.", "..."]);
        let bed = at(1, 1) + vec2(2.0, 3.0);
        let way = p.path(at(0, 0), bed).unwrap();
        assert_eq!(*way.last().unwrap(), bed);
        assert!(way[..way.len() - 1].iter().all(|w| p.open_at(*w)));
    }

    #[test]
    fn walking_follows_the_path_and_stops_at_the_end() {
        let mut pos = pos2(0.0, 0.0);
        let mut path = vec![pos2(10.0, 0.0), pos2(10.0, 10.0)];
        assert_eq!(step(&mut pos, &mut path, 15.0, 1.0), Some(10.0));
        assert_eq!((pos, path.len()), (pos2(10.0, 5.0), 1));
        step(&mut pos, &mut path, 100.0, 1.0);
        assert_eq!((pos, path.is_empty()), (pos2(10.0, 10.0), true));
        assert_eq!(step(&mut pos, &mut path, 100.0, 1.0), None);
    }

    #[test]
    fn the_block_houses_everyone_online_four_a_floor() {
        let ids = [3, 8, 20, 21, 40, 77];
        assert_eq!(apartment(&ids, 3), Some((1, 0)));
        assert_eq!(apartment(&ids, 40), Some((2, 0)));
        assert_eq!(apartment(&ids, 77), Some((2, 1)));
        assert_eq!(apartment(&ids, 5), None);
        assert_eq!((resident(&ids, 2, 1), resident(&ids, 2, 2), resident(&ids, 0, 0), resident(&ids, 1, 4)), (Some(77), None, None, None));
    }

    #[test]
    fn the_way_home_goes_by_lobby_elevator_and_front_door() {
        let ids = [3, 8, 20, 21, 40, 77];
        let home = |o| apartment(&ids, o);
        let door = |k: &str| Some(Leg::Door(k.into()));
        // from the gym to 77's home on floor 2: out, into the block, up, in at door 1
        assert_eq!(next_leg(Loc::Gym, Loc::Home(77), home), door("town"));
        assert_eq!(next_leg(Loc::Town, Loc::Home(77), home), door("lobby"));
        assert_eq!(next_leg(Loc::Lobby, Loc::Home(77), home), Some(Leg::Ride(Loc::Floor(2))));
        assert_eq!(next_leg(Loc::Floor(1), Loc::Home(77), home), Some(Leg::Ride(Loc::Floor(2))));
        assert_eq!(next_leg(Loc::Floor(2), Loc::Home(77), home), door("apt1"));
        // and back out to the shop
        assert_eq!(next_leg(Loc::Home(77), Loc::Shop, home), door("floor"));
        assert_eq!(next_leg(Loc::Floor(2), Loc::Shop, home), Some(Leg::Ride(Loc::Lobby)));
        assert_eq!(next_leg(Loc::Lobby, Loc::Shop, home), door("town"));
        assert_eq!(next_leg(Loc::Town, Loc::Shop, home), door("shop"));
        // nobody's home to go to once they've gone offline
        assert_eq!(next_leg(Loc::Lobby, Loc::Home(5), home), None);
    }

    #[test]
    fn furniture_fits_on_open_floor_without_walling_anyone_in() {
        let mut shell = place(&["#######", "#.....#", "#.....#", "#.....#", "#######"]);
        shell.key = "home".into();
        shell.spawn = [3.5 * TILE, 3.5 * TILE]; // the doormat, tile (3, 3)
        let art = |size, solid| FurniArt { sprite: 0, size, low: false, solid, stand: [0.0, 8.0] };
        let mut furni: HashMap<Furni, FurniArt> = Furni::ALL.iter().map(|&f| (f, art([1, 1], true))).collect();
        furni.insert(Furni::Rug, art([2, 2], false));
        furni.insert(Furni::Sofa, art([3, 1], true));
        let world = World::new(vec![shell], furni, &[[0.0, 0.0, 16.0, 16.0]]);
        let at = |f, x, y| Placed { f, x, y };
        let plant = at(Furni::Plant, 2, 1);
        assert!(world.fits(&[], plant));
        assert!(!world.fits(&[], at(Furni::Plant, 0, 1)), "on the wall");
        assert!(!world.fits(&[], at(Furni::Plant, 3, 3)), "on the doormat");
        assert!(!world.fits(&[], at(Furni::Sofa, 4, 1)), "half off the floor");
        assert!(!world.fits(&[plant], at(Furni::Cactus, 2, 1)), "on another piece");
        assert!(world.fits(&[plant], at(Furni::Rug, 1, 1)), "a rug goes under things");
        assert!(!world.fits(&[plant], at(Furni::Cactus, 1, 2)), "walls the corner tile in");
        // a furnished home's floor is blocked where the solid pieces stand, and a bed is a place to sleep
        let mut w = world;
        w.furnish(9, &[plant, at(Furni::Bed, 4, 1), at(Furni::Rug, 1, 2)]);
        let home = w.place(Loc::Home(9));
        assert!(!home.open(2, 1) && home.open(1, 2) && home.props.len() == 3);
        assert_eq!(home.slots("sleep"), &[[4.5 * TILE, 2.0 * TILE + 8.0]]);
    }

    #[test]
    fn locations_survive_the_network() {
        for l in [Loc::Town, Loc::Gym, Loc::Home(u64::MAX - 3), Loc::Lobby, Loc::Floor(3)] {
            let s = serde_json::to_string(&l).unwrap();
            assert_eq!(serde_json::from_str::<Loc>(&s).unwrap(), l);
        }
        assert_eq!(Loc::from_key("home", 7), Some(Loc::Home(7)));
    }
}
