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
// travels back: at least one round trip, plus up to one snapshot interval. The
// ping in the corner is that round trip, measured with a token the host echoes
// back.
//
// Two tricks work on it (both in `protocol::replica`):
//   * Interpolation plays snapshots back `INTERP_DELAY` ticks late so motion is
//     smooth. Press I to switch it off and see the raw 30 Hz steps; the cost of
//     smoothness is that everything is drawn ~100 ms in the past.
//   * Client-side prediction moves your own player at once, with the host's
//     movement code, and corrects it when the host's answer arrives. Press P to
//     switch it off and feel the full round trip on your own player: at a 300 ms
//     ping the difference is hard to miss.

use macroquad::prelude::*;
use protocol::replica::Replica;
use protocol::{decode_host, encode_client, ClientMsg, FromRelay, HostMsg, Reject, Rules, ToRelay};
use sim::{Input, DT};

use crate::armory::Armory;
use crate::controls::Controls;
use crate::draw::center_text;
use crate::menu::Choice;
use crate::net::{RelayLink, RELAY_URL};
use crate::present::Presenter;
use crate::socket::SocketState;
use crate::view::Glow;

/// How far behind the newest snapshot we draw: 6 ticks = 100 ms. Snapshots
/// come every 2 ticks, so this keeps two or three in the buffer: enough to
/// ride out one late packet without a stall.
const INTERP_DELAY: u32 = 6;

pub struct JoinGame {
    link: RelayLink,
    /// Interpolation on? (Toggled with I, to make the effect visible.)
    interpolate: bool,
    /// Prediction on? (Toggled with P, for the same reason.)
    predict: bool,
    /// Sequence number of the last input sent. The host acknowledges it in its
    /// snapshots, which is how the replica knows what to replay.
    seq: u32,
    /// The room to join; for a room we create on the server, filled in when
    /// the server answers with the code to share.
    code: String,
    /// `Some` when we ask the server to host a new match instead of joining
    /// one: the rules we chose on the setup screen.
    create: Option<Rules>,
    /// The weapons we picked, sent to the host with every `Welcome`.
    loadout: Vec<u8>,
    /// The weapon picker, open while we choose for the next match. Only the
    /// host picks the map (it owns the match); everybody picks their own weapons.
    armory: Option<Armory>,
    /// A new match's `Welcome` came while the picker was open: the match is
    /// running, and the choice goes to the host the moment it is made.
    loadout_due: bool,
    /// Picked for the next match already, and now waiting for it to start.
    chosen: bool,
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
    pub fn new(code: String, loadout: &[u8]) -> JoinGame {
        JoinGame {
            link: RelayLink::connect(RELAY_URL),
            interpolate: true,
            predict: true,
            seq: 0,
            code,
            create: None,
            loadout: loadout.to_vec(),
            armory: None,
            loadout_due: false,
            chosen: false,
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

    /// Open a room on the server (which runs the match) and play in it. From
    /// then on this is an ordinary joiner: the only difference is the first
    /// message, and that the server tells us the code our friends need.
    pub fn hosted(rules: Rules, loadout: &[u8]) -> JoinGame {
        JoinGame { create: Some(rules), ..JoinGame::new(String::new(), loadout) }
    }

    pub fn frame(&mut self, controls: &mut Controls, glow: &Glow) {
        let now = get_time();
        self.pump(now);

        if let Some(armory) = &mut self.armory {
            if self.problem.is_none() {
                if let Some((_, picked)) = armory.frame(glow) {
                    self.loadout = picked;
                    self.armory = None;
                    if self.loadout_due {
                        self.loadout_due = false;
                        self.send_loadout();
                    } else {
                        self.chosen = true;
                    }
                }
                return;
            }
            self.armory = None;
        }

        if self.problem.is_none() {
            if let Some(replica) = &mut self.replica {
                if is_key_pressed(KeyCode::I) {
                    self.interpolate = !self.interpolate;
                    replica.set_delay(delay_for(self.interpolate));
                }

                if is_key_pressed(KeyCode::P) {
                    self.predict = !self.predict;
                    replica.set_prediction(self.predict);
                }

                // Read the devices every frame (key *presses* are one-frame
                // events), but send exactly one input per sim tick, however
                // fast or slow the frames come: prediction counts inputs as
                // ticks, so a slow phone owes the host one per 1/60 s too (two
                // in a 30 fps frame). The cap stops a long stall (a hidden tab)
                // from sending a burst.
                let input = controls.read(&replica.world, replica.me as usize);
                self.send_clock = (self.send_clock + get_frame_time()).min(0.1);
                while self.send_clock >= DT {
                    self.send_clock -= DT;
                    self.seq += 1;
                    let input = Input { seq: self.seq, ..input };
                    replica.predict(input);
                    let bytes = encode_client(&ClientMsg::Input(input));
                    self.bytes_up += bytes.len() as u64;
                    self.link.send(&ToRelay::Data { to: None, bytes });
                }

                // After the match: choose weapons for the next one. The host
                // decides when that starts (or the server, after a pause), so
                // all we can do is be ready, and tell it when we are.
                let over = replica.world.winner.is_some();
                if over && !self.chosen && (is_key_pressed(KeyCode::R) || crate::touch::pressed()) {
                    self.armory = Some(Armory::new(Choice::Join(self.code.clone()), self.loadout.clone()));
                }
                let end_hint = if self.chosen { "waiting for the next match".to_string() } else { crate::hud::press_r("pick your weapons for the next match") };

                let smooth = if self.interpolate { "smoothing on" } else { "smoothing off" };
                let predicted = if self.predict { "prediction on" } else { "prediction off" };
                let banner = match self.rtt_ms {
                    Some(ms) => format!("room {}  -  ping {ms:.0} ms  -  {smooth} (I)  -  {predicted} (P)", self.code),
                    None => format!("room {}  -  {smooth} (I)  -  {predicted} (P)", self.code),
                };
                self.presenter.show(replica, self.bytes_up, self.bytes_down, &banner, &end_hint, glow);
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
            let request = match self.create {
                Some(rules) => ToRelay::CreateHostedRoom { rules },
                None => ToRelay::JoinRoom { code: self.code.clone() },
            };
            self.link.send(&request);
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
            FromRelay::RoomCreated { code } => self.code = code,
            FromRelay::RoomJoined => self.in_room = true,
            FromRelay::RoomClosed => self.problem = Some("The host left, so the match is over.".into()),
            FromRelay::Rejected(why) => {
                self.problem = Some(match why {
                    Reject::NoSuchRoom => "No room with that code.".into(),
                    Reject::RoomFull => "That room is full.".into(),
                    Reject::AlreadyInRoom => "Already in a room.".into(),
                    Reject::ServerBusy => "The server is busy, try again in a minute.".into(),
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
                    Some(HostMsg::Welcome(welcome)) => {
                        let mut replica = Replica::new(welcome, delay_for(self.interpolate));
                        replica.set_prediction(self.predict);
                        self.replica = Some(replica);
                        // Now that we have a slot, tell the host what we carry.
                        // (A rematch is a new match, so it is sent again, unless
                        // we are still choosing: then the choice follows.)
                        self.chosen = false;
                        if self.armory.is_some() {
                            self.loadout_due = true;
                        } else {
                            self.send_loadout();
                        }
                    }
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
            FromRelay::PeerJoined(_) | FromRelay::PeerLeft(_) | FromRelay::Pong { .. } => {}
        }
    }

    fn send_loadout(&mut self) {
        let bytes = encode_client(&ClientMsg::Loadout(self.loadout.clone()));
        self.bytes_up += bytes.len() as u64;
        self.link.send(&ToRelay::Data { to: None, bytes });
    }

    fn draw_status(&self) {
        if let Some(problem) = &self.problem {
            center_text(problem, 300.0, 36.0, ORANGE);
            center_text("Esc: back to the menu", 360.0, 26.0, GRAY);
            return;
        }
        let title = if self.create.is_some() { "OPENING A ROOM ON THE SERVER".to_string() } else { format!("JOINING {}", self.code) };
        center_text(&title, 200.0, 40.0, GRAY);
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

fn delay_for(interpolate: bool) -> u32 {
    if interpolate { INTERP_DELAY } else { 0 }
}

/// Seconds as a wrapping millisecond counter, small enough for a `u32` token.
fn millis(now: f64) -> u32 {
    (now * 1000.0) as u64 as u32
}
