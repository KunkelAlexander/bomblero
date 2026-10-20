// The host: the authority of a match.
//
// "Host" is a ROLE, not a place. This struct runs the real `World`, collects
// the players' inputs and produces the snapshots everybody else draws. Where it
// runs is somebody else's problem:
//
//   * offline / loopback: inside the game process, wired straight to the local
//     client (today);
//   * one player's browser, with friends connected through a relay;
//   * the dedicated server (`server/src/matches.rs`), which links this crate.
//
// In the last two, `Session` (session.rs) is the glue between relay peers and
// player slots, so that code is written once.
//
// To make that possible it knows nothing about windows, sockets or clocks. Its
// whole interface is plain function calls with already-decoded messages:
//
//     join()                  -> Welcome        a new player arrives
//     leave(id)                                 a player goes away
//     handle(id, ClientMsg)                     a message arrived from player `id`
//     tick()                  -> Snapshot       advance 1/60 s; the world afterwards
//
// The code around it moves bytes between these calls and the wire.
//
// `tick` returns a snapshot EVERY tick. How often a snapshot is really sent is
// a property of each link (the local player can have all 60 per second for
// free, a friend across the internet gets fewer), so it is decided by whoever
// owns the link, using a `Throttle`.

pub mod session;
pub mod throttle;

pub use session::Session;
pub use throttle::Throttle;

use protocol::{ClientMsg, PlayerId, Rules, Snapshot, Welcome};
use sim::bots::Bot;
use sim::tilemap::{Tilemap, MAPS};
use sim::weapons::{valid_loadout, BOT_LOADOUT};
use sim::{Event, Input, World};

/// Who drives a player slot.
///
/// Pattern: State. A slot is either a bot or a connected human, and what
/// `tick` does depends on which, without scattering `if is_human` flags.
enum Controller {
    /// Host-side AI. Bots live here (not in the clients) because they need the
    /// authoritative world, and so every client sees the same bots.
    Bot(Bot),
    /// A connected human: their most recent input.
    Human(Input),
}

pub struct Host {
    world: World,
    controllers: Vec<Controller>,
    seed: u32,
    /// The match's rules, repeated in every `Welcome`.
    rules: Rules,
    /// Which humans have already chosen their weapons (see `handle`).
    chose: Vec<bool>,
}

impl Host {
    /// A new match with `slots` players, all bots until humans join, played
    /// by the given rules.
    pub fn new(slots: usize, rules: Rules, seed: u32) -> Host {
        let mut host = Host::on_map(MAPS[rules.map as usize].art, slots, seed, &BOT_LOADOUT);
        host.world.score_limit = rules.score_limit;
        host.rules = rules;
        host
    }

    /// The same, on another map and with another starting set of weapons for
    /// everyone (the menu's hub is a map too, and hands out every weapon).
    pub fn on_map(map: &str, slots: usize, seed: u32, loadout: &[u8]) -> Host {
        let world = World::new(Tilemap::from_ascii(map), slots, loadout, seed);
        let controllers = (0..slots).map(|i| Controller::Bot(bot_for(seed, i))).collect();
        Host { world, controllers, seed, rules: Rules::default(), chose: vec![false; slots] }
    }

    /// A human arrives: give them the first slot a bot is holding.
    /// `None` means the match is full.
    pub fn join(&mut self) -> Option<Welcome> {
        let slot = self.controllers.iter().position(|c| matches!(c, Controller::Bot(_)))?;
        self.controllers[slot] = Controller::Human(Input::default());
        self.chose[slot] = false;
        // No events: the tiles above are already up to date, and replaying
        // explosions from before the newcomer arrived would be wrong.
        Some(Welcome { you: slot as PlayerId, rules: self.rules, tiles: self.world.map.tiles().to_vec(), snapshot: self.snapshot(Vec::new()) })
    }

    /// A human goes away: a bot takes the slot back, so the match carries on.
    pub fn leave(&mut self, id: PlayerId) {
        if let Some(c) = self.controllers.get_mut(id as usize) {
            *c = Controller::Bot(bot_for(self.seed, id as usize));
        }
    }

    /// A message arrived from player `id`.
    ///
    /// `id` comes from the transport (it knows which connection the bytes came
    /// from), NOT from the message. A client therefore cannot pose as another
    /// player.
    pub fn handle(&mut self, id: PlayerId, msg: ClientMsg) {
        match msg {
            ClientMsg::Input(input) => {
                // Only humans are driven by messages. Ignore anyone else.
                if let Some(Controller::Human(latest)) = self.controllers.get_mut(id as usize) {
                    // Keep only the newest input: if two arrive between ticks
                    // the older one is already out of date.
                    *latest = input;
                }
            }
            // The weapons a human picked. Never trust the client: the choice must
            // be valid, and only the first one counts, because choosing refills
            // the magazines (a client could otherwise "reload" by choosing again).
            // Bots keep the default set.
            ClientMsg::Loadout(ids) => {
                let human = matches!(self.controllers.get(id as usize), Some(Controller::Human(_)));
                if human && !self.chose[id as usize] && valid_loadout(&ids) {
                    self.world.set_loadout(id as usize, &ids);
                    self.chose[id as usize] = true;
                }
            }
            // A ping tests the connection, not the game, so whoever owns the
            // connection answers it (the host session), not this struct.
            ClientMsg::Ping { .. } => {}
        }
    }

    /// Advance the match by one tick. Returns the world as it is now, plus the
    /// events of this tick. Sending it (or not) is up to the caller.
    pub fn tick(&mut self) -> Snapshot {
        // Pattern: Command. Bots and humans both turn into the same `Input`.
        // A human whose packet is late simply repeats their last input.
        let world = &self.world;
        let inputs: Vec<Input> = self
            .controllers
            .iter_mut()
            .enumerate()
            .map(|(i, c)| match c {
                Controller::Bot(bot) => bot.think(world, i),
                Controller::Human(input) => *input,
            })
            .collect();

        self.world.step(&inputs);
        let events = self.world.drain_events();
        self.snapshot(events)
    }

    /// Read-only view of the real world (for tests and debugging).
    pub fn world(&self) -> &World {
        &self.world
    }

    fn snapshot(&self, events: Vec<Event>) -> Snapshot {
        // The input a human's slot is driven by is the one `tick` just used, so
        // its `seq` says how far the host has read that player's inputs.
        let acks = self
            .controllers
            .iter()
            .map(|c| match c {
                Controller::Human(input) => input.seq,
                Controller::Bot(_) => 0,
            })
            .collect();
        Snapshot {
            tick: self.world.tick,
            players: self.world.players.clone(),
            projectiles: self.world.projectiles.clone(),
            deployables: self.world.deployables.clone(),
            winner: self.world.winner,
            acks,
            events,
        }
    }
}

fn bot_for(seed: u32, slot: usize) -> Bot {
    Bot::new(seed.wrapping_add(slot as u32 * 7919))
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::replica::Replica;
    use protocol::{decode_host, encode_client, decode_client, encode_host, HostMsg};
    use sim::{vec2, DT};

    /// Send a host message through the byte encoding, like a real wire would.
    fn over_the_wire(msg: &HostMsg) -> HostMsg {
        decode_host(&encode_host(msg)).expect("host message survives encoding")
    }

    fn join_as_client(host: &mut Host) -> Replica {
        let welcome = host.join().expect("a free slot");
        match over_the_wire(&HostMsg::Welcome(welcome)) {
            HostMsg::Welcome(w) => Replica::new(w, 0),
            _ => unreachable!(),
        }
    }

    #[test]
    fn replica_follows_the_host_through_bytes() {
        let mut host = Host::new(4, Rules::default(), 7);
        let mut replica = join_as_client(&mut host);
        assert_eq!(replica.me, 0);

        // Walk right for a second, sending the input as bytes too.
        let input = Input { move_dir: vec2(1.0, 0.0), ..Input::default() };
        let start_x = replica.world.players[0].pos.x;
        for _ in 0..60 {
            let sent = decode_client(&encode_client(&ClientMsg::Input(input))).unwrap();
            host.handle(replica.me, sent);
            replica.apply(over_the_wire(&HostMsg::Snapshot(host.tick())));
            replica.advance(0.0); // what the frame loop does before drawing
        }

        assert!(replica.world.players[0].pos.x > start_x + 50.0, "player 0 should have walked right");
        // With a snapshot every tick, the copy is exactly the original.
        for (a, b) in replica.world.players.iter().zip(&host.world().players) {
            assert_eq!(a.pos, b.pos);
            assert_eq!(a.hp, b.hp);
        }
        assert_eq!(replica.world.tick, host.world().tick);
    }

    /// A walk over a slow link, three ways: an ideal machine with no lag, a
    /// client that predicts, and one that does not. Everything crosses `lag`
    /// ticks of delay in each direction, so the ping is `2 * lag` ticks.
    /// `jitter` makes every seventh input arrive late, to be corrected.
    ///
    /// Returns, per tick, how far (in px) the predicting client and the plain
    /// client are from the ideal one, and at the end how far the predicting
    /// client is from the real host.
    fn walk_over_a_slow_link(lag: usize, jitter: usize) -> (Vec<f32>, Vec<f32>, f32) {
        use std::collections::VecDeque;

        // Alone in the match: no bot can shoot or push us, so the ideal path is
        // exactly "the inputs, applied one per tick".
        let mut host = Host::new(1, Rules::default(), 7);
        let mut twin = Host::new(1, Rules::default(), 7);
        let welcome = host.join().unwrap();
        twin.join().unwrap();
        let mut predicting = Replica::new(welcome.clone(), 0);
        predicting.set_prediction(true);
        let mut plain = Replica::new(welcome, 0);

        // Stand still, walk right (the first push is a dash), stop, walk
        // down-left, stop.
        let script = |t: usize| match t {
            30..=89 => vec2(1.0, 0.0),
            120..=179 => vec2(-1.0, 1.0),
            _ => vec2(0.0, 0.0),
        };

        // (tick it arrives at, message). Kept in arrival order, as a WebSocket does.
        let mut to_host: VecDeque<(usize, Input)> = VecDeque::new();
        let mut to_client: VecDeque<(usize, Snapshot)> = VecDeque::new();
        let (mut errors, mut plain_errors) = (Vec::new(), Vec::new());

        for t in 0..300 {
            let input = Input { seq: t as u32 + 1, move_dir: script(t), ..Input::default() };
            predicting.predict(input);
            let late = if t % 7 == 0 { jitter } else { 0 };
            let arrives = (t + lag + late).max(to_host.back().map_or(0, |m| m.0));
            to_host.push_back((arrives, input));
            twin.handle(0, ClientMsg::Input(input));
            twin.tick();

            while to_host.front().is_some_and(|m| m.0 <= t) {
                host.handle(0, ClientMsg::Input(to_host.pop_front().unwrap().1));
            }
            to_client.push_back((t + lag, host.tick()));
            while to_client.front().is_some_and(|m| m.0 <= t) {
                let snapshot = over_the_wire(&HostMsg::Snapshot(to_client.pop_front().unwrap().1));
                predicting.apply(snapshot.clone());
                plain.apply(snapshot);
            }
            predicting.advance(DT);
            plain.advance(DT);

            let ideal = twin.world().players[0].pos;
            errors.push(predicting.world.players[0].pos.distance(ideal));
            plain_errors.push(plain.world.players[0].pos.distance(ideal));
        }
        let from_host = predicting.world.players[0].pos.distance(host.world().players[0].pos);
        (errors, plain_errors, from_host)
    }

    #[test]
    fn prediction_hides_the_round_trip() {
        // 12 ticks each way: 400 ms ping, like Europe to Singapore and back
        // with a bad day.
        let (errors, plain_errors, _) = walk_over_a_slow_link(12, 0);
        let worst = errors.iter().cloned().fold(0.0, f32::max);
        let plain_worst = plain_errors.iter().cloned().fold(0.0, f32::max);
        println!("worst distance from the ideal path: predicted {worst:.2} px, plain {plain_worst:.1} px");
        assert!(worst < 0.01, "with a steady link the prediction is exact, was {worst} px off");
        assert!(plain_worst > 60.0, "without it we draw the past, was only {plain_worst} px behind");
    }

    #[test]
    fn a_late_packet_is_corrected_not_lost() {
        let (errors, plain_errors, from_host) = walk_over_a_slow_link(12, 3);
        let worst = errors.iter().cloned().fold(0.0, f32::max);
        let plain_worst = plain_errors.iter().cloned().fold(0.0, f32::max);
        println!("with jitter: predicted {worst:.2} px, plain {plain_worst:.1} px");
        // A late input means the host moved us one tick behind what we guessed.
        // That is a few pixels, eased away, and nothing like the plain client.
        assert!(worst < 25.0, "a wobble of {worst} px is a visible jerk");
        assert!(worst < plain_worst / 3.0);
        // The host is the authority, not the ideal machine: a late input really
        // does move the host's player differently. What matters is that, once
        // everyone stands still, the client ends up exactly where the host says.
        assert!(from_host < 0.01, "the client ended {from_host} px from the host");
    }

    #[test]
    fn the_snapshot_says_how_many_inputs_the_host_has_read() {
        let mut host = Host::new(2, Rules::default(), 1);
        host.join(); // slot 0 is a human, slot 1 a bot
        host.handle(0, ClientMsg::Input(Input { seq: 41, ..Input::default() }));
        assert_eq!(host.tick().acks, [41, 0]);
    }

    #[test]
    fn tile_changes_reach_the_replica() {
        let mut host = Host::new(4, Rules::default(), 7);
        let mut replica = join_as_client(&mut host);
        // Tile (2,5) is a brick in the Grid map. Announce its destruction
        // without touching the host's own map, so that only the *event* can be
        // what changes the replica.
        assert_eq!(replica.world.map.get(2, 5), sim::Tile::Brick(3));
        let mut snapshot = host.tick();
        snapshot.events.push(Event::TileChanged { x: 2, y: 5, tile: sim::Tile::Empty });
        replica.apply(over_the_wire(&HostMsg::Snapshot(snapshot)));
        replica.advance(0.0);
        assert_eq!(replica.world.map.get(2, 5), sim::Tile::Empty);
    }

    #[test]
    fn a_late_joiner_gets_the_current_map() {
        let mut host = Host::new(4, Rules::default(), 7);
        host.world.map.set(2, 5, sim::Tile::Empty);
        let replica = join_as_client(&mut host);
        assert_eq!(replica.world.map.tiles(), host.world().map.tiles());
    }

    #[test]
    fn join_leave_and_full_match() {
        let mut host = Host::new(2, Rules::default(), 1);
        assert_eq!(host.join().unwrap().you, 0);
        assert_eq!(host.join().unwrap().you, 1);
        assert!(host.join().is_none(), "no free slot left");
        host.leave(0);
        assert_eq!(host.join().unwrap().you, 0, "a bot gave the slot back");
    }

    #[test]
    fn a_player_gets_the_weapons_they_chose_once() {
        use sim::weapons::{BLASTER, BOMB, FLASH, NOVA, TELEPORT};
        let ids = |host: &Host, slot: usize| host.world().players[slot].weapons.iter().map(|w| w.id).collect::<Vec<_>>();
        let mut host = Host::new(3, Rules::default(), 1);
        host.join(); // slot 0
        host.handle(0, ClientMsg::Loadout(vec![NOVA, BOMB, TELEPORT, FLASH, BLASTER]));
        assert_eq!(ids(&host, 0), [NOVA, BOMB, TELEPORT, FLASH, BLASTER]);

        // A second choice (a refill trick) and nonsense are both ignored.
        host.handle(0, ClientMsg::Loadout(BOT_LOADOUT.to_vec()));
        assert_eq!(ids(&host, 0), [NOVA, BOMB, TELEPORT, FLASH, BLASTER]);
        host.join(); // slot 1
        host.handle(1, ClientMsg::Loadout(vec![NOVA; 5]));
        host.handle(1, ClientMsg::Loadout(vec![0, 1, 2]));
        assert_eq!(ids(&host, 1), BOT_LOADOUT, "invalid choices change nothing");

        // Slot 2 is a bot: messages claiming to come from it are not obeyed.
        host.handle(2, ClientMsg::Loadout(vec![NOVA, BOMB, TELEPORT, FLASH, BLASTER]));
        assert_eq!(ids(&host, 2), BOT_LOADOUT);
    }

    #[test]
    fn inputs_for_bot_slots_are_ignored() {
        let mut host = Host::new(2, Rules::default(), 1);
        // Nobody joined, so slot 1 is a bot. A forged input must not move it.
        let forged = Input { fire: true, ..Input::default() };
        host.handle(1, ClientMsg::Input(forged));
        assert!(matches!(host.controllers[1], Controller::Bot(_)));
    }

    /// A bandwidth budget as a test: if someone adds a fat field to
    /// `Snapshot`, this fails and the cost shows up before the players feel it.
    #[test]
    fn one_second_of_snapshots_fits_the_budget() {
        let mut host = Host::new(4, Rules::default(), 7);
        let mut every_tick = 0;
        let mut every_second_tick = 0;
        let mut throttle = Throttle::new(2);
        for _ in 0..60 {
            let snapshot = host.tick();
            every_tick += encode_host(&HostMsg::Snapshot(snapshot.clone())).len();
            if let Some(s) = throttle.push(snapshot) {
                every_second_tick += encode_host(&HostMsg::Snapshot(s)).len();
            }
        }
        println!("60 snapshots/s: {every_tick} B/s,  30 snapshots/s: {every_second_tick} B/s");
        // Raised from 30_000 when the Scatter arrived: its 8 pellets per shot are
        // 8 projectiles in every snapshot. Friends get 30 snapshots/s anyway, so
        // about half of this. If it grows again, shrink `Projectile` on the wire
        // (e.g. positions as 16-bit integers) before raising the limit.
        assert!(every_tick < 40_000, "snapshots grew to {every_tick} B/s");
        // Events ride along, so halving the rate saves a bit less than half.
        assert!(every_second_tick * 10 < every_tick * 6);
    }

    #[test]
    fn garbage_bytes_are_dropped_not_fatal() {
        assert!(decode_client(&[1, 2, 3]).is_none());
        assert!(decode_host(&vec![0xFF; 100_000]).is_none());
    }
}
