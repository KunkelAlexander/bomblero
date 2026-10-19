// The client's copy of the match.
//
// A client never runs `World::step`. It keeps a `World` purely as a container
// and fills it with what the host said. That is a *replica*. Because it is a
// real `World`, all the drawing code works on it unchanged: rendering cannot
// tell whether it is looking at the real thing (offline) or a copy (online).
//
// Networking concept: interpolation. Snapshots arrive in steps (every 33 ms at
// 30/s) and with jitter (the network delivers them a little early or late).
// Drawing the newest one the moment it arrives makes everything move in jerks.
// Instead the replica plays the match back from a small buffer, deliberately
// `delay` ticks BEHIND the newest snapshot, and draws the moment *between* the
// two snapshots around the playhead. It trades 100 ms of extra delay for smooth
// motion, and the buffer absorbs late packets.
//
// Pattern: Double Buffer. The world on screen (`world`) is never half-updated:
// it is rewritten as a whole from the two snapshots in `queue` once per frame.
//
// Networking concept: client-side prediction. Everything above makes the
// picture smooth, but it is still the host's picture, so your own player moves
// only after your key press has gone to the host and the answer has come back
// (a full round trip) and then waited out the interpolation delay. Prediction
// cheats for ONE thing, your own movement: the client runs the same movement
// code as the host (`World::predict_body`) the moment you press a key, and
// shows that. The host stays the authority; its snapshots keep correcting us:
//
//   1. every input you send is also applied to your own body at once, and kept
//      in `pending` with its sequence number;
//   2. a snapshot arrives. It says "I have applied your inputs up to #N" (its
//      `acks`) and gives your body as the host has it, N inputs ago;
//   3. we drop the pending inputs up to #N and REPLAY the rest on top of the
//      host's body. That is where the host will have you by the time it has
//      read them. Normally it equals what we already show, so nothing happens.
//      When it does not (you were blown up, bumped into a wall we did not know
//      about) the picture is nudged, not jerked: see `Prediction::offset`.
//
// This is also called reconciliation. The other players are not predicted
// (their inputs are not ours to know), so they still move on the interpolated,
// delayed timeline: you see yourself in the present and the others in the past.

use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};

use sim::tilemap::MAPS;
use sim::{Event, Input, Player, Tilemap, Vec2, World, DT, TILE};

use crate::{HostMsg, PlayerId, Snapshot, Welcome};

/// How hard the playhead speeds up or slows down to stay `delay` ticks behind
/// the host (per tick of error). Gentle, so the correction itself is invisible.
const CATCH_UP: f32 = 0.1;
/// If we are this many ticks off (the tab was hidden, a lag spike), do not
/// glide back: jump.
const MAX_ERROR: f32 = 30.0;
/// A player that moved further than this between two snapshots teleported
/// (respawn); sliding them across the map would look wrong.
const TELEPORT: f32 = TILE * 3.0;
/// How fast a prediction error melts away, per second. 10 means about 100 ms
/// to forget most of it: slow enough not to be seen as a jerk, fast enough
/// that a wrong picture does not hang around.
const EASE_RATE: f32 = 10.0;
/// Inputs kept for replay. 120 is two seconds; beyond that the host has long
/// stopped answering and replaying more would only prolong a wrong picture.
const MAX_PENDING: usize = 120;

/// The client's guess about its own player (see the note at the top).
struct Prediction {
    /// Inputs the host has not acknowledged yet, oldest first.
    pending: VecDeque<Input>,
    /// My body as predicted: the host's last word plus every pending input.
    /// Kept apart from `World::players`, which `present` rewrites each frame.
    body: Player,
    /// How far the picture is still off from `body`, being eased to nothing.
    /// When a correction arrives, `body` jumps but the picture must not:
    /// the error goes in here and `advance` bleeds it out over ~100 ms.
    offset: Vec2,
}

pub struct Replica {
    /// What to draw.
    pub world: World,
    /// Which player this client controls.
    pub me: PlayerId,
    /// Events whose moment has come but that nobody has shown yet.
    events: Vec<Event>,
    /// Ticks to stay behind the newest snapshot. 0 = no interpolation (the
    /// local player, whose "network" is a function call, does not need it).
    delay: u32,
    /// Snapshots in tick order. `queue[0]` is the one the playhead has passed
    /// (already shown); `queue[1]` is what we are moving towards.
    queue: VecDeque<Snapshot>,
    /// The tick being shown right now. Fractional: it moves smoothly between
    /// the whole ticks the snapshots are labelled with.
    playhead: f32,
    /// Seconds since the newest snapshot arrived. Snapshots come in steps, but
    /// time does not, so this makes the target move smoothly too.
    since_newest: f32,
    /// `Some` while this client predicts its own player.
    prediction: Option<Prediction>,
}

impl Replica {
    pub fn new(welcome: Welcome, delay: u32) -> Replica {
        // Start from the map the host named, then lay the real tiles over it
        // (the match may be minutes old and full of craters). The number comes
        // from another machine, so an unknown one falls back to the first map
        // rather than crashing.
        let def = MAPS.get(welcome.rules.map as usize).unwrap_or(&MAPS[0]);
        let mut world = World::new(Tilemap::from_ascii(def.art), welcome.snapshot.players.len(), &[], 0);
        world.map.load_tiles(&welcome.tiles);
        world.score_limit = welcome.rules.score_limit;
        let playhead = welcome.snapshot.tick as f32;
        let mut replica = Replica {
            world,
            me: welcome.you,
            events: Vec::new(),
            delay,
            queue: VecDeque::from([welcome.snapshot]),
            playhead,
            since_newest: 0.0,
            prediction: None,
        };
        replica.present();
        replica
    }

    /// Feed in one message from the host.
    pub fn apply(&mut self, msg: HostMsg) {
        match msg {
            // A second Welcome means the host restarted the match: a fresh
            // replica, with nothing pending (those inputs were for the old one).
            HostMsg::Welcome(w) => {
                let predicting = self.predicting();
                *self = Replica::new(w, self.delay);
                self.set_prediction(predicting);
            }
            // Only queued: nothing is drawn from it until `advance` gets there.
            // The prediction, though, hears of it at once: its job is to be
            // ahead of the playhead, so it uses the newest news there is.
            HostMsg::Snapshot(s) => {
                self.reconcile(&s);
                self.queue.push_back(s);
                self.since_newest = 0.0;
            }
            // About the connection, not the match: the session reads it.
            HostMsg::Pong { .. } | HostMsg::Full => {}
        }
    }

    pub fn delay(&self) -> u32 {
        self.delay
    }

    pub fn set_delay(&mut self, delay: u32) {
        self.delay = delay;
    }

    pub fn predicting(&self) -> bool {
        self.prediction.is_some()
    }

    /// Turn prediction on or off. The local player of a hosted match leaves it
    /// off: its "network" is a function call, so there is nothing to hide.
    pub fn set_prediction(&mut self, on: bool) {
        // Start from the newest host state (the queue is never empty).
        let host_body = self.queue.back().map(|s| s.players[self.me as usize].clone());
        self.prediction = host_body.filter(|_| on).map(|body| Prediction { pending: VecDeque::new(), body, offset: Vec2::ZERO });
    }

    /// The player just sent `input` to the host: apply it to our own body now
    /// instead of waiting for the host's answer. Call once per input sent, in
    /// order, with the same `seq` the host will acknowledge.
    pub fn predict(&mut self, input: Input) {
        let me = self.me as usize;
        let Some(pred) = &mut self.prediction else { return };
        step_mine(&mut self.world, me, &mut pred.body, &input);
        pred.pending.push_back(input);
        if pred.pending.len() > MAX_PENDING {
            pred.pending.pop_front();
        }
    }

    /// A snapshot arrived: rebuild the prediction on top of the host's news
    /// (steps 2 and 3 in the note at the top).
    fn reconcile(&mut self, snapshot: &Snapshot) {
        let me = self.me as usize;
        let Some(pred) = &mut self.prediction else { return };
        let Some(host_body) = snapshot.players.get(me) else { return };

        let ack = snapshot.acks.get(me).copied().unwrap_or(0);
        pred.pending.retain(|input| input.seq > ack);
        let mut corrected = host_body.clone();
        for input in &pred.pending {
            step_mine(&mut self.world, me, &mut corrected, input);
        }

        // `corrected` and `pred.body` describe the same moment (the end of the
        // pending inputs), so their difference is purely how wrong we were.
        // The picture keeps what it showed; the error is paid off by `advance`.
        pred.offset += pred.body.pos - corrected.pos;
        if pred.offset.length() > TELEPORT {
            // Respawned or teleported: easing across the map would look wrong.
            pred.offset = Vec2::ZERO;
        }
        pred.body = corrected;
    }

    /// Draw my own player from the prediction instead of the delayed playhead.
    fn show_prediction(&mut self) {
        let Some(pred) = &self.prediction else { return };
        let mine = &mut self.world.players[self.me as usize];
        // Dead, or the host says so already: the host's picture is the right one.
        if !mine.alive || !pred.body.alive {
            return;
        }
        // Only the movement is predicted; health, ammo and score stay as the
        // playhead has them, so they change together with their events.
        mine.pos = pred.body.pos + pred.offset;
        mine.vel = pred.body.vel;
        mine.aim = pred.body.aim;
    }

    /// Move the playhead forward by `seconds` of real time and update `world`.
    /// Call once per rendered frame, before drawing.
    pub fn advance(&mut self, seconds: f32) {
        // The queue is never empty: it always keeps the snapshot passed last.
        let newest = self.queue.back().map_or(0.0, |s| s.tick as f32);
        self.since_newest += seconds;

        if self.delay == 0 {
            self.playhead = newest;
        } else {
            // Where we want to be: `delay` ticks behind where the host is now
            // (the newest snapshot, plus the time that passed since it came).
            let target = newest + self.since_newest / DT - self.delay as f32;
            let step = seconds / DT; // one tick per 1/60 s of real time
            let error = target - (self.playhead + step);
            if error.abs() > MAX_ERROR {
                self.playhead = target;
            } else {
                // A bit faster than real time when behind and slower when
                // ahead, so jitter is smoothed out instead of shown.
                let rate = (1.0 + error * CATCH_UP).clamp(0.5, 1.5);
                self.playhead += step * rate;
            }
        }
        // Never show a future we have no data for.
        self.playhead = self.playhead.min(newest);
        self.present();

        if let Some(pred) = &mut self.prediction {
            pred.offset *= (-EASE_RATE * seconds).exp();
        }
        self.show_prediction();
    }

    /// Fill `world` with the match as it looks at the playhead.
    fn present(&mut self) {
        // Everything the playhead has passed has now "happened" for this
        // client: release its events, and patch its tile changes into the map
        // (Pattern: Dirty Flag) at the same moment, so a brick breaks exactly
        // when its explosion is shown.
        while self.queue.len() >= 2 && self.queue[1].tick as f32 <= self.playhead {
            self.queue.pop_front();
            let passed = &mut self.queue[0];
            for event in &passed.events {
                if let Event::TileChanged { x, y, tile } = *event {
                    self.world.map.set(x, y, tile);
                }
            }
            self.events.append(&mut passed.events);
        }

        let from = &self.queue[0];
        self.playhead = self.playhead.max(from.tick as f32);
        self.world.tick = from.tick;
        self.world.winner = from.winner;
        // Overwrite, don't merge: the host is the authority, so its word is
        // final. (Over a lossy network like UDP we would also drop a snapshot
        // older than the one we already have. WebSocket is ordered, so we
        // don't need to.)
        self.world.players = from.players.clone();
        self.world.projectiles = from.projectiles.clone();
        self.world.deployables = from.deployables.clone();

        // Projectiles fly in straight lines, so instead of matching them up
        // between two snapshots (they have no id) we just move each one on
        // from where the older snapshot saw it.
        let since_from = (self.playhead - from.tick as f32) * DT;
        for p in &mut self.world.projectiles {
            p.pos += p.vel * since_from;
        }

        // Players: slide from where they were towards where they will be.
        if let Some(to) = self.queue.get(1) {
            let t = (self.playhead - from.tick as f32) / (to.tick - from.tick) as f32;
            for (p, next) in self.world.players.iter_mut().zip(&to.players) {
                if p.alive && next.alive && p.pos.distance(next.pos) < TELEPORT {
                    p.pos = p.pos.lerp(next.pos, t);
                    p.aim = lerp_angle(p.aim, next.aim, t);
                }
            }
        }
    }

    /// Same idea as `World::drain_events`, for the events that arrived by network.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

/// Move one body by one input, using the replica's world for the walls and
/// puddles around it. The body is swapped into its slot in `world` because the
/// sim's movement code works on `world.players[me]`; swapping avoids copying it.
fn step_mine(world: &mut World, me: usize, body: &mut Player, input: &Input) {
    std::mem::swap(&mut world.players[me], body);
    world.predict_body(me, input);
    std::mem::swap(&mut world.players[me], body);
}

/// Blend two angles the short way round (359° to 1° is a 2° turn, not 358°).
fn lerp_angle(from: f32, to: f32, t: f32) -> f32 {
    let diff = (to - from + PI).rem_euclid(TAU) - PI;
    from + diff * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::vec2;

    fn world() -> World {
        World::new(Tilemap::from_ascii(MAPS[0].art), 2, &[], 0)
    }

    /// Player 0 stands at x = `tick * 10`, so a position tells the tick.
    fn snapshot(tick: u32, events: Vec<Event>) -> Snapshot {
        let mut players = world().players;
        players[0].pos = vec2(tick as f32 * 10.0, 100.0);
        Snapshot { tick, players, projectiles: Vec::new(), deployables: Vec::new(), winner: None, acks: Vec::new(), events }
    }

    fn replica(delay: u32) -> Replica {
        let w = world();
        Replica::new(Welcome { you: 0, rules: crate::Rules::default(), tiles: w.map.tiles().to_vec(), snapshot: snapshot(0, Vec::new()) }, delay)
    }

    fn arrive(r: &mut Replica, tick: u32, events: Vec<Event>) {
        r.apply(HostMsg::Snapshot(snapshot(tick, events)));
    }

    #[test]
    fn the_playhead_blends_between_two_snapshots() {
        let mut r = replica(6);
        arrive(&mut r, 2, Vec::new());
        r.playhead = 1.0; // halfway between tick 0 and tick 2
        r.present();
        assert_eq!(r.world.players[0].pos.x, 10.0);
    }

    #[test]
    fn events_wait_for_the_playhead() {
        let mut r = replica(6);
        let boom = Event::Explosion { pos: vec2(0.0, 0.0), radius: 1.0 };
        arrive(&mut r, 2, vec![boom]);
        r.playhead = 1.0;
        r.present();
        assert!(r.drain_events().is_empty(), "the explosion is still in our future");
        r.playhead = 2.0;
        r.present();
        assert_eq!(r.drain_events().len(), 1);
        assert!(r.drain_events().is_empty(), "and it is shown once");
    }

    #[test]
    fn no_delay_means_the_newest_snapshot_at_once() {
        let mut r = replica(0);
        arrive(&mut r, 2, Vec::new());
        r.advance(0.0);
        assert_eq!(r.world.tick, 2);
        assert_eq!(r.world.players[0].pos.x, 20.0);
    }

    #[test]
    fn playback_settles_the_chosen_delay_behind_the_host() {
        let mut r = replica(6);
        // Five seconds of 60 fps frames; the host sends a snapshot every other tick.
        for frame in 1..=300u32 {
            if frame % 2 == 0 {
                arrive(&mut r, frame, Vec::new());
            }
            r.advance(DT);
        }
        // Measured from where the host is right now: the newest snapshot plus the time since.
        let behind = 300.0 + r.since_newest / DT - r.playhead;
        assert!((behind - 6.0).abs() < 1.5, "playhead is {behind} ticks behind, wanted 6");
    }

    #[test]
    fn a_hidden_tab_catches_up_by_jumping() {
        let mut r = replica(6);
        for tick in (2..=600).step_by(2) {
            arrive(&mut r, tick, Vec::new()); // ten seconds arrive with no frame drawn
        }
        r.advance(DT);
        assert!(r.playhead > 590.0 - 6.0, "playhead jumped to {}", r.playhead);
    }

    #[test]
    fn a_correction_is_eased_in_not_jerked() {
        let mut r = replica(0);
        r.set_prediction(true);
        r.advance(0.0);
        let before = r.world.players[0].pos;

        // The host says we are 20 px from where we think we are.
        arrive(&mut r, 2, Vec::new());
        r.advance(0.0);
        assert_eq!(r.world.players[0].pos, before, "the picture must not jump");

        r.advance(1.0);
        assert!((r.world.players[0].pos.x - 20.0).abs() < 0.1, "a second later we show the host's version");
    }

    #[test]
    fn a_teleport_is_not_eased() {
        let mut r = replica(0);
        r.set_prediction(true);
        arrive(&mut r, 40, Vec::new()); // 400 px away: a respawn, not a wobble
        r.advance(0.0);
        assert_eq!(r.world.players[0].pos.x, 400.0);
    }

    #[test]
    fn without_prediction_the_picture_is_the_hosts() {
        let mut r = replica(0);
        arrive(&mut r, 2, Vec::new());
        r.advance(0.0);
        assert!(!r.predicting());
        assert_eq!(r.world.players[0].pos.x, 20.0);
    }

    #[test]
    fn angles_blend_the_short_way() {
        let a = lerp_angle(PI - 0.1, -PI + 0.1, 0.5);
        assert!((a.abs() - PI).abs() < 1e-4, "got {a}");
    }
}
