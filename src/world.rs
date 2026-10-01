//! The world: places on a 16 px tile grid (the town and the inside of its buildings), what stands
//! in them, and getting around (paths over the walkable tiles, doors between places).
//! Layouts come from `tools/gen_assets.py` through assets/meta.json.

use eframe::egui::{Pos2, Rect, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

pub const TILE: f32 = 16.0;

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
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Loc::Town => "Town",
            Loc::Library => "Library",
            Loc::Gym => "Gym",
            Loc::Portal => "Portal",
            Loc::Arena => "Arena",
            Loc::Shop => "Shop",
            Loc::Home(_) => "Home",
        }
    }

    /// "in town", "at home", "in the Gym".
    pub fn at(self) -> String {
        match self {
            Loc::Town => "in town".into(),
            Loc::Home(_) => "at home".into(),
            l => format!("in the {}", l.name()),
        }
    }

    /// A door's destination; "home" is always your own (the Apartment's door).
    pub fn from_key(key: &str, me: u64) -> Option<Loc> {
        Some(match key {
            "town" => Loc::Town,
            "library" => Loc::Library,
            "gym" => Loc::Gym,
            "portal" => Loc::Portal,
            "arena" => Loc::Arena,
            "shop" => Loc::Shop,
            "home" => Loc::Home(me),
            _ => return None,
        })
    }
}

#[derive(Deserialize)]
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
#[derive(Deserialize)]
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

#[derive(Deserialize)]
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
    fn locations_survive_the_network() {
        for l in [Loc::Town, Loc::Gym, Loc::Home(u64::MAX - 3)] {
            let s = serde_json::to_string(&l).unwrap();
            assert_eq!(serde_json::from_str::<Loc>(&s).unwrap(), l);
        }
        assert_eq!(Loc::from_key("home", 7), Some(Loc::Home(7)));
    }
}
