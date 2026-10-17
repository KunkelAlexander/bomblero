// Joining a friend's game: connect to the relay, knock on the room, wait for
// the host's `Welcome`, then play by sending inputs and drawing snapshots.
//
// This is a pure client. It never calls `World::step`; the world it draws is a
// `Replica` that the host's snapshots overwrite (see `protocol::replica`).
//
//   keyboard ─ Input ──> relay ──> host
//   screen <─ Replica <─ Snapshot ─ relay <── host
//
// Networking concept: latency. Between pressing a key and seeing your player
// move, the news travels to the host, the host simulates it, and the snapshot
// travels back: at least one round trip, plus up to one snapshot interval.
// There is no client-side prediction (yet), so you feel all of it. The ping in
// the corner is that round trip, measured with a token the host echoes back.

use macroquad::prelude::*;
use protocol::replica::Replica;
use protocol::{decode_host, encode_client, ClientMsg, FromRelay, HostMsg, Reject, ToRelay};
use sim::DT;

use crate::controls::Controls;
use crate::draw::center_text;
use crate::net::{RelayLink, RELAY_URL};
use crate::present::Presenter;
use crate::socket::SocketState;
use crate::view::Glow;

pub struct JoinGame {
    link: RelayLink,
    code: String,
    /// Have we asked the relay to let us in yet? (Only possible once it is open.)
    asked: bool,
    /// The relay put us in the room; now we wait for the host's Welcome.
    in_room: bool,
    /// Built from the Welcome. `None` until the host has let us into the match.
    replica: Option<Replica>,
    /// Something went wrong or the match ended; shown instead of the game.
    problem: Option<String>,
    rtt_ms: Option<f32>,
    last_ping: f64,
    /// Real time since the last input was sent (see `frame`).
    send_clock: f32,
    bytes_up: u64,
    bytes_down: u64,
    presenter: Presenter,
}

impl JoinGame {
    pub fn new(code: String) -> JoinGame {
        JoinGame {
            link: RelayLink::connect(RELAY_URL),
            code,
            asked: false,
            in_room: false,
            replica: None,
            problem: None,
            rtt_ms: None,
            last_ping: 0.0,
            send_clock: 0.0,
            bytes_up: 0,
            bytes_down: 0,
            presenter: Presenter::default(),
        }
    }

    pub fn frame(&mut self, controls: &mut Controls, glow: &Glow) {
        let now = get_time();
        self.pump(now);

        if self.problem.is_none() {
            if let Some(replica) = &mut self.replica {
                // Read the devices every frame (key *presses* are one-frame
                // events), but send at most once per sim tick. The host keeps
                // only the newest input per tick anyway, so on a 144 Hz
                // monitor the extra messages would just waste bandwidth.
                let input = controls.read(&replica.world, replica.me as usize);
                self.send_clock += get_frame_time();
                if self.send_clock >= DT {
                    self.send_clock %= DT;
                    let bytes = encode_client(&ClientMsg::Input(input));
                    self.bytes_up += bytes.len() as u64;
                    self.link.send(&ToRelay::Data { to: None, bytes });
                }

                let banner = match self.rtt_ms {
                    Some(ms) => format!("room {}  -  ping {ms:.0} ms", self.code),
                    None => format!("room {}", self.code),
                };
                self.presenter.show(replica, self.bytes_up, self.bytes_down, &banner, false, glow);
                return;
            }
        }

        glow.begin();
        self.draw_status();
        glow.present();
    }

    /// Talk to the relay: connect, knock, read what the host and relay say,
    /// and ping once a second.
    fn pump(&mut self, now: f64) {
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
            self.link.send(&ToRelay::JoinRoom { code: self.code.clone() });
        }

        while let Some(msg) = self.link.recv() {
            self.on_relay_message(msg, now);
        }

        if self.in_room && now - self.last_ping >= 1.0 {
            self.last_ping = now;
            let bytes = encode_client(&ClientMsg::Ping { token: millis(now) });
            self.bytes_up += bytes.len() as u64;
            self.link.send(&ToRelay::Data { to: None, bytes });
        }
    }

    fn on_relay_message(&mut self, msg: FromRelay, now: f64) {
        match msg {
            FromRelay::RoomJoined => self.in_room = true,
            FromRelay::RoomClosed => self.problem = Some("The host left, so the match is over.".into()),
            FromRelay::Rejected(why) => {
                self.problem = Some(match why {
                    Reject::NoSuchRoom => "No room with that code.".into(),
                    Reject::RoomFull => "That room is full.".into(),
                    Reject::AlreadyInRoom => "Already in a room.".into(),
                })
            }
            FromRelay::Data { bytes, .. } => {
                self.bytes_down += bytes.len() as u64;
                match decode_host(&bytes) {
                    // Wrapping subtraction: the token is a clock reading that
                    // wraps around every ~49 days.
                    Some(HostMsg::Pong { token }) => self.rtt_ms = Some(millis(now).wrapping_sub(token) as f32),
                    Some(HostMsg::Full) => self.problem = Some("That match is full.".into()),
                    // The first Welcome starts our match; a later one means
                    // the host started a rematch.
                    Some(HostMsg::Welcome(welcome)) => self.replica = Some(Replica::new(welcome)),
                    // Snapshots that arrive before the Welcome are useless
                    // (there is nothing to apply them to) and are dropped.
                    Some(msg) => {
                        if let Some(replica) = &mut self.replica {
                            replica.apply(msg);
                        }
                    }
                    None => {}
                }
            }
            FromRelay::RoomCreated { .. } | FromRelay::PeerJoined(_) | FromRelay::PeerLeft(_) | FromRelay::Pong { .. } => {}
        }
    }

    fn draw_status(&self) {
        if let Some(problem) = &self.problem {
            center_text(problem, 300.0, 36.0, ORANGE);
            center_text("Esc: back to the menu", 360.0, 26.0, GRAY);
            return;
        }
        center_text(&format!("JOINING {}", self.code), 200.0, 40.0, GRAY);
        if self.link.state() == SocketState::Connecting {
            center_text("Connecting to the relay...", 300.0, 40.0, WHITE);
            center_text("(a free server can take up to a minute to wake up)", 345.0, 22.0, GRAY);
        } else if self.in_room {
            center_text("in the room, waiting for the host...", 300.0, 40.0, WHITE);
        } else {
            center_text("knocking...", 300.0, 40.0, WHITE);
        }
        center_text("Esc: back", 400.0, 24.0, GRAY);
    }
}

/// Seconds as a wrapping millisecond counter, small enough for a `u32` token.
fn millis(now: f64) -> u32 {
    (now * 1000.0) as u64 as u32
}
