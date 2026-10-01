//! LAN play over UDP broadcast. Every pet shouts a `Hello` every couple of seconds and a `Move`
//! whenever it sets off somewhere; challenges, waves and gifts go straight back to the sender's
//! address. Battles are simulated locally on both sides from the same seed + stat snapshots.

use crate::game::{Fighter, Item, Job, Species, Stage, ZONES, clean};
use crate::world::Loc;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

pub const PORT: u16 = 47474;
/// LP2: the walkable world (pets have a place and a position). LP1 pets are ignored.
const MAGIC: &[u8] = b"LP2";

/// A place position from the network: finite and roughly on the map (the app clamps it to the place).
fn sane(p: [f32; 2]) -> [f32; 2] {
    p.map(|v| if v.is_finite() { v.clamp(0.0, 4096.0) } else { 0.0 })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    pub id: u64,
    pub name: String,
    pub species: Species,
    /// Adult when an older LanPet (no life stages) says nothing.
    #[serde(default)]
    pub stage: Stage,
    pub level: u32,
    pub hat: Option<Item>,
    pub status: String,
    pub wins: u32,
    pub losses: u32,
    pub fighter: Fighter,
    /// Where the pet is; None while away on an expedition.
    pub loc: Option<Loc>,
    /// Its feet in that place (pixels).
    pub pos: [f32; 2],
    pub job: Option<Job>,
}

impl Card {
    fn sanitize(&mut self) {
        self.name = clean(&self.name, 16);
        self.status = clean(&self.status, 40);
        self.level = self.level.clamp(1, 999);
        self.pos = sane(self.pos);
        if let Some(Job::Explore(z)) = self.job {
            if z as usize >= ZONES.len() {
                self.job = None;
            }
        }
        self.fighter.sanitize();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Msg {
    Hello { card: Card },
    Challenge { battle: u64, card: Card },
    Accept { battle: u64, card: Card },
    Decline { battle: u64, why: String },
    Wave { from: String },
    Gift { from: String, item: Item },
    /// Chat in a place; everyone hears it, only pets in `loc` show it.
    Chat { id: u64, name: String, loc: Loc, text: String },
    /// Pet `id` set off from `from` to `to` in `loc`; everyone walks it there the same way.
    Move { id: u64, loc: Loc, from: [f32; 2], to: [f32; 2] },
}

impl Msg {
    fn sanitize(mut self) -> Option<Msg> {
        match &mut self {
            Msg::Hello { card } | Msg::Challenge { card, .. } | Msg::Accept { card, .. } => card.sanitize(),
            Msg::Decline { why, .. } => *why = clean(why, 60),
            Msg::Wave { from } | Msg::Gift { from, .. } => *from = clean(from, 16),
            Msg::Chat { name, text, .. } => {
                *name = clean(name, 16);
                *text = clean(text, 120);
                if text.is_empty() {
                    return None;
                }
            }
            Msg::Move { from, to, .. } => (*from, *to) = (sane(*from), sane(*to)),
        }
        Some(self)
    }
}

fn encode(m: &Msg) -> Vec<u8> {
    let mut v = MAGIC.to_vec();
    v.extend(serde_json::to_vec(m).unwrap_or_default());
    v
}

fn decode(b: &[u8]) -> Option<Msg> {
    let body = b.strip_prefix(MAGIC)?;
    serde_json::from_slice::<Msg>(body).ok()?.sanitize()
}

pub struct Net {
    sock: UdpSocket,
    pub rx: Receiver<(SocketAddr, Msg)>,
}

impl Net {
    /// `wake` is called from the receiver threads so the UI repaints on new messages.
    pub fn start(wake: impl Fn() + Send + Sync + Clone + 'static) -> std::io::Result<Net> {
        // Discovery socket shares the well-known port (SO_REUSEADDR lets several pets run on one PC).
        let disc = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        disc.set_reuse_address(true)?;
        #[cfg(target_os = "macos")] // BSD sockets also need SO_REUSEPORT to share a UDP port
        disc.set_reuse_port(true)?;
        disc.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into())?;
        let disc: UdpSocket = disc.into();
        // Our own socket: broadcasts go out from here, so peers reply straight to it.
        let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        sock.set_broadcast(true)?;
        let (tx, rx) = channel();
        for s in [disc, sock.try_clone()?] {
            let (tx, wake) = (tx.clone(), wake.clone());
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match s.recv_from(&mut buf) {
                        Ok((n, from)) => {
                            if let Some(m) = decode(&buf[..n]) {
                                if tx.send((from, m)).is_err() {
                                    return;
                                }
                                wake();
                            }
                        }
                        // e.g. Windows reports ICMP "port unreachable" as a recv error; keep going.
                        Err(_) => std::thread::sleep(Duration::from_millis(50)),
                    }
                }
            });
        }
        Ok(Net { sock, rx })
    }

    pub fn broadcast(&self, m: &Msg) {
        let _ = self.sock.send_to(&encode(m), (Ipv4Addr::BROADCAST, PORT));
    }

    pub fn send(&self, to: SocketAddr, m: &Msg) {
        let _ = self.sock.send_to(&encode(m), to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Pet, Species};

    fn card(id: u64) -> Card {
        let p = Pet::new(format!("Pet{id}"), Species::Frog, 0);
        Card {
            id,
            name: p.name.clone(),
            species: p.species,
            stage: p.stage(),
            level: 1,
            hat: None,
            status: "x".into(),
            wins: 0,
            losses: 0,
            fighter: p.fighter(true),
            loc: Some(Loc::Gym),
            pos: [40.0, 50.0],
            job: None,
        }
    }

    #[test]
    fn rejects_junk_and_clamps_hostile_cards() {
        assert!(decode(b"hello").is_none());
        assert!(decode(b"LP2{\"t\":\"Gift\",\"from\":\"x\",\"item\":\"NotAnItem\"}").is_none());
        assert!(decode(b"LP2{\"t\":\"Chat\",\"id\":1,\"name\":\"x\",\"loc\":\"Moon\",\"text\":\"hi\"}").is_none());
        let mut c = card(1);
        c.fighter.str = i32::MAX;
        c.name = "a\u{7}very long name that goes on".into();
        c.pos = [-3.0, 9999.0];
        let Some(Msg::Hello { card }) = decode(&encode(&Msg::Hello { card: c })) else { panic!() };
        assert_eq!(card.fighter.str, 100_000);
        assert!(card.name.starts_with("avery long") && card.name.chars().count() <= 16);
        assert_eq!(card.pos, [0.0, 4096.0]);
        let m = Msg::Move { id: 1, loc: Loc::Home(9), from: [1e9, 5.0], to: [3.0, 4.0] };
        let Some(Msg::Move { loc, from, .. }) = decode(&encode(&m)) else { panic!() };
        assert_eq!((loc, from), (Loc::Home(9), [4096.0, 5.0]));
    }

    #[test]
    fn two_pets_find_each_other() {
        let a = Net::start(|| {}).unwrap();
        let b = Net::start(|| {}).unwrap();
        a.broadcast(&Msg::Hello { card: card(1) });
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut from = None;
        while from.is_none() && std::time::Instant::now() < deadline {
            if let Ok((addr, Msg::Hello { card })) = b.rx.recv_timeout(Duration::from_millis(100)) {
                if card.id == 1 {
                    from = Some(addr);
                }
            }
        }
        let from = from.expect("broadcast not received (no LAN route?)");
        // reply goes straight to a's socket
        b.send(from, &Msg::Wave { from: "B".into() });
        let got = (0..30).find_map(|_| match a.rx.recv_timeout(Duration::from_millis(100)) {
            Ok((_, Msg::Wave { from })) => Some(from),
            _ => None,
        });
        assert_eq!(got.as_deref(), Some("B"));
    }
}
