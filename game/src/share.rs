// The host's side of an online game: open a room on the relay and let friends
// into the match that is already running.
//
//   friend's browser ──> relay ──> Share ──> Host::handle(slot, msg)
//   friend's browser <── relay <── Share <── Host::tick() snapshot
//
// `Share` is the "host session": it is the only code that knows both the relay
// protocol (peers, rooms) and the match (slots). `Host` knows nothing about the
// relay, and the relay knows nothing about the match. If the host role ever
// moves to a dedicated server, this file moves with it and `Host` is untouched.

use std::collections::HashMap;

use host::{Host, Throttle};
use protocol::relay::PeerId;
use protocol::{decode_client, encode_host, ClientMsg, FromRelay, HostMsg, PlayerId, Snapshot, ToRelay};

use crate::net::{RelayLink, RELAY_URL};
use crate::socket::SocketState;

/// Send friends a snapshot every N sim ticks: 2 = 30 per second.
///
/// The trade-off, in numbers (4 players, ~350 bytes per snapshot):
///   1 -> 60/s -> ~21 KB/s of host upload, friends see 60 Hz steps
///   2 -> 30/s -> ~10 KB/s,                friends see 30 Hz steps
///   3 -> 20/s -> ~7 KB/s,                 friends see 20 Hz steps (visibly choppy)
/// 30 is the usual compromise; interpolation (next step) hides the steps.
/// Only the link to friends is throttled: the host's own player is wired in
/// memory and keeps all 60.
const SEND_EVERY: u32 = 2;

/// Ping the relay this often (in host ticks: 20 s). A host alone in its room
/// would otherwise send nothing at all, and an idle WebSocket is cut by the
/// proxies of hosting services, which also let a free server fall asleep after
/// 15 minutes without incoming messages. A tiny ping is the cheapest "I am
/// still here" there is.
const KEEPALIVE_TICKS: u32 = 20 * 60;

pub struct Share {
    link: RelayLink,
    throttle: Throttle,
    ticks_since_ping: u32,
    /// Have we asked the relay for a room yet? (Only possible once it is open.)
    asked: bool,
    code: Option<String>,
    /// Which player slot each friend got. The relay numbers connections
    /// (`PeerId`), the match numbers players (`PlayerId`); this is the bridge.
    peers: HashMap<PeerId, PlayerId>,
    problem: Option<String>,
    /// What really crossed the network, for the HUD. Only payloads are
    /// counted, not the relay's small envelope around them.
    pub bytes_up: u64,
    pub bytes_down: u64,
}

impl Share {
    /// Start connecting to the relay. The game does not wait for it: the host
    /// keeps playing while the connection (and a sleeping free server) wakes up.
    pub fn open() -> Share {
        Share {
            link: RelayLink::connect(RELAY_URL),
            throttle: Throttle::new(SEND_EVERY),
            ticks_since_ping: 0,
            asked: false,
            code: None,
            peers: HashMap::new(),
            problem: None,
            bytes_up: 0,
            bytes_down: 0,
        }
    }

    /// One line for the HUD.
    pub fn status(&self) -> String {
        if let Some(problem) = &self.problem {
            return problem.clone();
        }
        match &self.code {
            Some(code) => format!("room {code}  -  friends: {}", self.peers.len()),
            None => "opening a room...".into(),
        }
    }

    /// Take in everything the relay has delivered: friends arriving, leaving,
    /// and their messages. Called before each host tick.
    pub fn receive(&mut self, host: &mut Host) {
        match self.link.state() {
            SocketState::Connecting => return,
            SocketState::Closed => {
                // The relay is gone, so every friend is too: bots take their slots.
                for (_, slot) in self.peers.drain() {
                    host.leave(slot);
                }
                self.code = None;
                if self.problem.is_none() {
                    self.problem = Some(format!("lost the relay ({RELAY_URL}); playing offline"));
                }
                return;
            }
            SocketState::Open => {}
        }

        if !self.asked {
            self.asked = true;
            self.link.send(&ToRelay::CreateRoom);
        }

        self.ticks_since_ping += 1;
        if self.ticks_since_ping >= KEEPALIVE_TICKS {
            self.ticks_since_ping = 0;
            self.link.send(&ToRelay::Ping { token: 0 });
        }

        while let Some(msg) = self.link.recv() {
            match msg {
                FromRelay::RoomCreated { code } => self.code = Some(code),
                FromRelay::PeerJoined(peer) => self.welcome(host, peer),
                FromRelay::PeerLeft(peer) => {
                    if let Some(slot) = self.peers.remove(&peer) {
                        host.leave(slot);
                    }
                }
                FromRelay::Data { from, bytes } => self.on_data(host, from, bytes),
                // Messages that only a joiner gets.
                FromRelay::RoomJoined | FromRelay::RoomClosed | FromRelay::Rejected(_) | FromRelay::Pong { .. } => {}
            }
        }
    }

    /// Offer this tick's snapshot to the friends; every `SEND_EVERY`-th one
    /// really goes out. One `Data { to: None }` message: the relay copies it
    /// to everybody, so the host uploads each snapshot once no matter how many
    /// friends there are.
    pub fn broadcast(&mut self, snapshot: Snapshot) {
        if self.peers.is_empty() {
            // Nobody listening: do not spend upload on it. Also forget the
            // events held back for them. They are older than whatever a future
            // joiner's `Welcome` will say, and replaying them would undo it.
            self.throttle = Throttle::new(SEND_EVERY);
            return;
        }
        let Some(snapshot) = self.throttle.push(snapshot) else { return };
        let bytes = encode_host(&HostMsg::Snapshot(snapshot));
        self.bytes_up += bytes.len() as u64;
        self.link.send(&ToRelay::Data { to: None, bytes });
    }

    /// The host started a new `Host` (rematch): every friend needs a fresh
    /// `Welcome` for the new match, exactly like a late joiner.
    pub fn welcome_again(&mut self, host: &mut Host) {
        let peers: Vec<PeerId> = self.peers.keys().copied().collect();
        self.peers.clear();
        // Events held back from the old match must not leak into the new one.
        self.throttle = Throttle::new(SEND_EVERY);
        for peer in peers {
            self.welcome(host, peer);
        }
    }

    /// A friend arrived: give them a slot and the full state of the match.
    fn welcome(&mut self, host: &mut Host, peer: PeerId) {
        match host.join() {
            Some(welcome) => {
                self.peers.insert(peer, welcome.you);
                self.send_to(peer, &HostMsg::Welcome(welcome));
            }
            None => self.send_to(peer, &HostMsg::Full),
        }
    }

    fn on_data(&mut self, host: &mut Host, from: PeerId, bytes: Vec<u8>) {
        self.bytes_down += bytes.len() as u64;
        // `from` was stamped by the relay and the slot comes from OUR table,
        // never from the message, so a friend cannot act as another player.
        // Someone without a slot (the match was full) is ignored.
        let Some(&slot) = self.peers.get(&from) else { return };
        match decode_client(&bytes) {
            Some(ClientMsg::Ping { token }) => self.send_to(from, &HostMsg::Pong { token }),
            Some(msg) => host.handle(slot, msg),
            None => {} // garbage is dropped, never fatal
        }
    }

    fn send_to(&mut self, peer: PeerId, msg: &HostMsg) {
        let bytes = encode_host(msg);
        self.bytes_up += bytes.len() as u64;
        self.link.send(&ToRelay::Data { to: Some(peer), bytes });
    }
}
