// The host's side of an online game: open a room on the relay and let friends
// into the match that is already running.
//
//   friend's browser ──> relay ──> Share ──> Host::handle(slot, msg)
//   friend's browser <── relay <── Share <── Host::tick() snapshot
//
// `Share` is only the WebSocket around a `host::Session`, which holds the
// table between relay peers and match slots (and is shared with the dedicated
// server). `Host` knows nothing about the relay, and the relay knows nothing
// about the match.

use host::{Host, Session};
use protocol::{FromRelay, Snapshot, ToRelay};

use crate::net::{RelayLink, RELAY_URL};
use crate::socket::SocketState;

/// Ping the relay this often (in host ticks: 20 s). A host alone in its room
/// would otherwise send nothing at all, and an idle WebSocket is cut by the
/// proxies of hosting services, which also let a free server fall asleep after
/// 15 minutes without incoming messages. A tiny ping is the cheapest "I am
/// still here" there is.
const KEEPALIVE_TICKS: u32 = 20 * 60;

pub struct Share {
    link: RelayLink,
    session: Session,
    ticks_since_ping: u32,
    /// Have we asked the relay for a room yet? (Only possible once it is open.)
    asked: bool,
    code: Option<String>,
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
            session: Session::new(),
            ticks_since_ping: 0,
            asked: false,
            code: None,
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
            Some(code) => format!("room {code}  -  friends: {}", self.session.friends()),
            None => "opening a room...".into(),
        }
    }

    /// Take in everything the relay has delivered: friends arriving, leaving,
    /// and their messages. Called before each host tick.
    pub fn receive(&mut self, host: &mut Host) {
        match self.link.state() {
            SocketState::Connecting => return,
            SocketState::Closed => {
                self.session.drop_everyone(host);
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
                msg => {
                    if let FromRelay::Data { bytes, .. } = &msg {
                        self.bytes_down += bytes.len() as u64;
                    }
                    for out in self.session.on_relay(host, msg) {
                        self.send(out);
                    }
                }
            }
        }
    }

    /// Offer this tick's snapshot to the friends (see `Session::broadcast`).
    pub fn broadcast(&mut self, snapshot: Snapshot) {
        if let Some(out) = self.session.broadcast(snapshot) {
            self.send(out);
        }
    }

    /// The host started a new `Host` (rematch): every friend needs a fresh
    /// `Welcome` for the new match, exactly like a late joiner.
    pub fn welcome_again(&mut self, host: &mut Host) {
        for out in self.session.welcome_again(host) {
            self.send(out);
        }
    }

    fn send(&mut self, msg: ToRelay) {
        if let ToRelay::Data { bytes, .. } = &msg {
            self.bytes_up += bytes.len() as u64;
        }
        self.link.send(&msg);
    }
}
