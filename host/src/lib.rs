// The host: the authority of a match.
//
// "Host" is a ROLE, not a place. This struct runs the real `World`, collects
// the players' inputs and produces the snapshots everybody else draws. Where it
// runs is somebody else's problem:
//
//   * offline / loopback: inside the game process, wired straight to the local
//     client (today);
//   * one player's browser, with friends connected through a relay (step 3);
//   * a dedicated server binary that links this crate (maybe, one day).
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

pub mod throttle;

pub use throttle::Throttle;

use protocol::{ClientMsg, PlayerId, Rules, Snapshot, Welcome};
use sim::bots::Bot;
use sim::tilemap::{Tilemap, MAPS};
use sim::weapons::{valid_loadout, DEFAULT_LOADOUT};
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
        let mut host = Host::on_map(MAPS[rules.map as usize].art, slots, seed, &DEFAULT_LOADOUT);
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
        Snapshot {
            tick: self.world.tick,
            players: self.world.players.clone(),
            projectiles: self.world.projectiles.clone(),
            winner: self.world.winner,
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
    use sim::vec2;

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
        use sim::weapons::{BOMB, NOVA, SWARM, TELEPORT, CLUSTER};
        let ids = |host: &Host, slot: usize| host.world().players[slot].weapons.iter().map(|w| w.id).collect::<Vec<_>>();
        let mut host = Host::new(3, Rules::default(), 1);
        host.join(); // slot 0
        host.handle(0, ClientMsg::Loadout(vec![NOVA, BOMB, SWARM, TELEPORT, CLUSTER]));
        assert_eq!(ids(&host, 0), [NOVA, BOMB, SWARM, TELEPORT, CLUSTER]);

        // A second choice (a refill trick) and nonsense are both ignored.
        host.handle(0, ClientMsg::Loadout(DEFAULT_LOADOUT.to_vec()));
        assert_eq!(ids(&host, 0), [NOVA, BOMB, SWARM, TELEPORT, CLUSTER]);
        host.join(); // slot 1
        host.handle(1, ClientMsg::Loadout(vec![NOVA; 5]));
        host.handle(1, ClientMsg::Loadout(vec![0, 1, 2]));
        assert_eq!(ids(&host, 1), DEFAULT_LOADOUT, "invalid choices change nothing");

        // Slot 2 is a bot: messages claiming to come from it are not obeyed.
        host.handle(2, ClientMsg::Loadout(vec![NOVA, BOMB, SWARM, TELEPORT, CLUSTER]));
        assert_eq!(ids(&host, 2), DEFAULT_LOADOUT);
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
