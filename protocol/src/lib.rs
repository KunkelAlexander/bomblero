// The language that clients and the host speak. Nothing in here does anything:
// it is only message shapes plus turning them into bytes and back.
//
// Why a crate of its own: the game (client), the host and the relay server all
// have to agree on these shapes. One shared definition means they cannot drift
// apart, and the compiler checks every side whenever a message changes.
//
// The conversation, in full:
//
//   client                                  host
//     │ ── ClientMsg::Loadout ──────────────> │   once: "these are my five weapons"
//     │ ── ClientMsg::Input ────────────────> │   "here is what my player wants"
//     │                                       │   (host steps the sim)
//     │ <──────────── HostMsg::Welcome ────── │   once, when you join
//     │ <──────────── HostMsg::Snapshot ───── │   again and again: "here is the world"
//
// Networking concepts used below:
//   * Authority: only the host changes the world. Clients send *wishes*
//     (inputs) and receive *facts* (snapshots). A client cannot say "I hit
//     him" or "I am at x=5"; it can only say "I am pressing fire".
//   * Tick: the sim's step counter. It labels snapshots so a client knows which
//     moment of the match it is looking at.

pub mod relay;
pub mod replica;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
pub use relay::{FromRelay, Reject, ToRelay};
use sim::{Event, Input, Player, Projectile, Tile};

/// Which player slot a connection controls. The host decides it, never the
/// client (otherwise a client could claim to be someone else).
pub type PlayerId = u8;

/// Client -> host.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMsg {
    /// The player's wishes for now. Sent once per rendered frame; the host
    /// keeps only the newest one per player.
    Input(Input),
    /// The weapons this player picked before the match (ids into `WEAPONS`).
    /// Sent once, right after the `Welcome`. The host checks it and accepts only
    /// the first one: a second would be a free refill of every magazine.
    Loadout(Vec<u8>),
    /// "Are you there? How long does a round trip take?" The token is echoed
    /// back unchanged in `HostMsg::Pong`.
    Ping { token: u32 },
}

/// Host -> client.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HostMsg {
    Welcome(Welcome),
    Snapshot(Snapshot),
    Pong { token: u32 },
    /// Answer to a join when every slot is taken (there is no `Welcome`).
    /// Without it the newcomer would wait forever.
    Full,
}

/// The rules of a match, chosen by the host on the setup screen. Only the host
/// chooses (it runs the match); a joiner learns them from the `Welcome`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// Index into `sim::tilemap::MAPS`.
    pub map: u8,
    /// Kills to win.
    pub score_limit: i32,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules { map: 0, score_limit: sim::world::SCORE_LIMIT }
    }
}

/// Sent once to a new arrival: everything needed to start from scratch.
/// This is the "late joiner" message.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Welcome {
    /// The slot this client controls.
    pub you: PlayerId,
    /// Which map, and how many kills win. The map is named by its number: the
    /// art is in both programs already, and only the tiles that changed since
    /// the start need to travel (`tiles`, below).
    pub rules: Rules,
    /// The whole terrain right now. Snapshots only carry *changes* to it
    /// (`Event::TileChanged`), so a newcomer needs the full grid exactly once.
    pub tiles: Vec<Tile>,
    /// The world right now, so there is something valid to draw immediately.
    pub snapshot: Snapshot,
}

/// "This is what the world looks like." Everything that moves, plus what
/// happened since the previous snapshot.
///
/// The terrain is deliberately absent (see `Welcome`).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    /// The sim tick this snapshot was taken after.
    pub tick: u32,
    pub players: Vec<Player>,
    pub projectiles: Vec<Projectile>,
    pub winner: Option<u8>,
    /// Events since the last snapshot. Clients turn them into explosions and
    /// sounds, and apply `TileChanged` to their copy of the map.
    pub events: Vec<Event>,
}

// ---- bytes ---------------------------------------------------------------
//
// A network carries bytes, not Rust values. Every message is encoded right
// before sending and decoded right after receiving. We do this even for the
// in-process loopback so the encoding is exercised on every single run.

/// Anything bigger than this is rejected before decoding. The bytes come from
/// the outside world, so we never trust their size.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    bincode::serialize(msg).expect("encoding a message cannot fail")
}

/// `None` means the bytes were garbage. Callers drop such messages: a peer
/// sending nonsense must never be able to crash us.
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return None;
    }
    bincode::deserialize(bytes).ok()
}

pub fn encode_client(msg: &ClientMsg) -> Vec<u8> {
    encode(msg)
}

pub fn encode_host(msg: &HostMsg) -> Vec<u8> {
    encode(msg)
}

pub fn decode_client(bytes: &[u8]) -> Option<ClientMsg> {
    decode(bytes)
}

pub fn decode_host(bytes: &[u8]) -> Option<HostMsg> {
    decode(bytes)
}

pub fn encode_to_relay(msg: &ToRelay) -> Vec<u8> {
    encode(msg)
}

pub fn encode_from_relay(msg: &FromRelay) -> Vec<u8> {
    encode(msg)
}

pub fn decode_to_relay(bytes: &[u8]) -> Option<ToRelay> {
    decode(bytes)
}

pub fn decode_from_relay(bytes: &[u8]) -> Option<FromRelay> {
    decode(bytes)
}
