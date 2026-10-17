// The waiting room for an online game: connect to the relay, then either open
// a room (host) or enter one by code (joiner).
//
// Right now this only proves the plumbing. It shows who is in the room and
// measures the round trip joiner -> relay -> host -> relay -> joiner. Step 3
// replaces the ping with the real match.
//
// Networking concept: latency / RTT (round-trip time). Every message takes time
// to travel. We cannot measure one-way time without synchronised clocks, but we
// can measure there-and-back with one clock: send a token, wait for the host
// to echo it, subtract.

use macroquad::prelude::*;
use protocol::relay::PeerId;
use protocol::{decode_client, decode_host, encode_client, encode_host, ClientMsg, FromRelay, HostMsg, Reject, ToRelay};

use crate::draw::center_text;
use crate::net::{RelayLink, RELAY_URL};
use crate::socket::SocketState;

enum Role {
    Host { code: Option<String>, peers: Vec<PeerId> },
    Joiner { code: String, joined: bool },
}

pub struct Lobby {
    link: RelayLink,
    role: Role,
    /// Have we asked the relay to create/join yet? (Only possible once open.)
    asked: bool,
    /// Something went wrong or the room ended; shown instead of the lobby.
    problem: Option<String>,
    rtt_ms: Option<f32>,
    last_ping: f64,
}

impl Lobby {
    pub fn host() -> Lobby {
        Lobby::new(Role::Host { code: None, peers: Vec::new() })
    }

    pub fn join(code: String) -> Lobby {
        Lobby::new(Role::Joiner { code, joined: false })
    }

    fn new(role: Role) -> Lobby {
        Lobby { link: RelayLink::connect(RELAY_URL), role, asked: false, problem: None, rtt_ms: None, last_ping: 0.0 }
    }

    /// Call once per frame. `now` is in seconds.
    pub fn update(&mut self, now: f64) {
        match self.link.state() {
            SocketState::Connecting => return,
            SocketState::Closed => {
                if self.problem.is_none() {
                    self.problem = Some(format!("Lost the connection to the relay ({RELAY_URL})."));
                }
                return;
            }
            SocketState::Open => {}
        }

        if !self.asked {
            self.asked = true;
            match &self.role {
                Role::Host { .. } => self.link.send(&ToRelay::CreateRoom),
                Role::Joiner { code, .. } => self.link.send(&ToRelay::JoinRoom { code: code.clone() }),
            }
        }

        while let Some(msg) = self.link.recv() {
            self.on_relay_message(msg, now);
        }

        // A joiner pings the host once a second.
        if let Role::Joiner { joined: true, .. } = self.role {
            if now - self.last_ping >= 1.0 {
                self.last_ping = now;
                let ping = ClientMsg::Ping { token: millis(now) };
                self.link.send(&ToRelay::Data { to: None, bytes: encode_client(&ping) });
            }
        }
    }

    fn on_relay_message(&mut self, msg: FromRelay, now: f64) {
        match (msg, &mut self.role) {
            (FromRelay::RoomCreated { code: c }, Role::Host { code, .. }) => *code = Some(c),
            (FromRelay::RoomJoined, Role::Joiner { joined, .. }) => *joined = true,
            (FromRelay::PeerJoined(p), Role::Host { peers, .. }) => peers.push(p),
            (FromRelay::PeerLeft(p), Role::Host { peers, .. }) => peers.retain(|&x| x != p),
            (FromRelay::RoomClosed, _) => self.problem = Some("The host left, so the room is closed.".into()),
            (FromRelay::Rejected(why), _) => {
                self.problem = Some(match why {
                    Reject::NoSuchRoom => "No room with that code.".into(),
                    Reject::RoomFull => "That room is full.".into(),
                    Reject::AlreadyInRoom => "Already in a room.".into(),
                })
            }
            (FromRelay::Data { from, bytes }, Role::Host { .. }) => {
                // The host answers pings. (In step 3 this moves into the real
                // host session, next to `Host::handle`.)
                if let Some(ClientMsg::Ping { token }) = decode_client(&bytes) {
                    let pong = HostMsg::Pong { token };
                    self.link.send(&ToRelay::Data { to: Some(from), bytes: encode_host(&pong) });
                }
            }
            (FromRelay::Data { bytes, .. }, Role::Joiner { .. }) => {
                if let Some(HostMsg::Pong { token }) = decode_host(&bytes) {
                    // Wrapping subtraction: the token is a clock reading that
                    // wraps around every ~49 days.
                    self.rtt_ms = Some(millis(now).wrapping_sub(token) as f32);
                }
            }
            _ => {}
        }
    }

    pub fn draw(&self) {
        if let Some(problem) = &self.problem {
            center_text(problem, 300.0, 36.0, ORANGE);
            center_text("Esc: back to the menu", 360.0, 26.0, GRAY);
            return;
        }
        if self.link.state() == SocketState::Connecting {
            center_text("Connecting to the relay...", 300.0, 40.0, WHITE);
            center_text("(a free server can take up to a minute to wake up)", 345.0, 22.0, GRAY);
            center_text("Esc: back", 400.0, 24.0, GRAY);
            return;
        }
        match &self.role {
            Role::Host { code, peers } => {
                center_text("HOSTING", 200.0, 40.0, GRAY);
                match code {
                    Some(code) => {
                        center_text(&format!("room code   {code}"), 290.0, 80.0, Color::new(0.1, 1.0, 0.95, 1.0));
                        center_text("tell your friends to press J and type it", 340.0, 24.0, GRAY);
                    }
                    None => center_text("opening a room...", 290.0, 40.0, WHITE),
                }
                center_text(&format!("friends in the room: {}", peers.len()), 420.0, 36.0, WHITE);
            }
            Role::Joiner { code, joined } => {
                center_text(&format!("JOINING {code}"), 200.0, 40.0, GRAY);
                if *joined {
                    center_text("connected to the host", 290.0, 50.0, Color::new(0.6, 1.0, 0.2, 1.0));
                    match self.rtt_ms {
                        Some(ms) => center_text(&format!("round trip via relay: {ms:.0} ms"), 380.0, 36.0, WHITE),
                        None => center_text("measuring round trip...", 380.0, 36.0, GRAY),
                    }
                } else {
                    center_text("knocking...", 290.0, 40.0, WHITE);
                }
            }
        }
        center_text("Esc: leave", 520.0, 24.0, GRAY);
    }
}

/// Seconds as a wrapping millisecond counter, small enough for a `u32` token.
fn millis(now: f64) -> u32 {
    (now * 1000.0) as u64 as u32
}
