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
//     tick()                  -> Option<HostMsg>  advance 1/60 s; maybe a snapshot
//
// The code around it moves bytes between these calls and the wire.

use protocol::{ClientMsg, HostMsg, PlayerId, Snapshot, Welcome};
use sim::bots::Bot;
use sim::tilemap::{Tilemap, MAP_GRID};
use sim::weapons::{BLASTER, RAILGUN, ROCKET};
use sim::{Event, Input, World};

/// Send a snapshot every N sim ticks. 1 = 60 per second. Online we will send
/// fewer (every 2 = 30/s) to save bandwidth and interpolate on the client; the
/// trade-off is explained where that happens.
const SNAPSHOT_EVERY: u32 = 1;

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
    /// Events produced since the last snapshot. Snapshots can be sent less
    /// often than ticks, so events pile up here until one goes out.
    pending_events: Vec<Event>,
    seed: u32,
}

impl Host {
    /// A new match with `slots` players, all bots until humans join.
    pub fn new(slots: usize, seed: u32) -> Host {
        let world = World::new(Tilemap::from_ascii(MAP_GRID), slots, &[BLASTER, ROCKET, RAILGUN], seed);
        let controllers = (0..slots).map(|i| Controller::Bot(bot_for(seed, i))).collect();
        Host { world, controllers, pending_events: Vec::new(), seed }
    }

    /// A human arrives: give them the first slot a bot is holding.
    /// `None` means the match is full.
    pub fn join(&mut self) -> Option<Welcome> {
        let slot = self.controllers.iter().position(|c| matches!(c, Controller::Bot(_)))?;
        self.controllers[slot] = Controller::Human(Input::default());
        Some(Welcome { you: slot as PlayerId, tiles: self.world.map.tiles().to_vec(), snapshot: self.snapshot() })
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
        }
    }

    /// Advance the match by one tick. Returns a message to broadcast to every
    /// player when a snapshot is due.
    pub fn tick(&mut self) -> Option<HostMsg> {
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
        self.pending_events.extend(self.world.drain_events());

        if self.world.tick % SNAPSHOT_EVERY == 0 {
            Some(HostMsg::Snapshot(self.snapshot()))
        } else {
            None
        }
    }

    /// Read-only view of the real world (for tests and debugging).
    pub fn world(&self) -> &World {
        &self.world
    }

    fn snapshot(&mut self) -> Snapshot {
        Snapshot {
            tick: self.world.tick,
            players: self.world.players.clone(),
            projectiles: self.world.projectiles.clone(),
            winner: self.world.winner,
            // Taking the events empties the pile, so each event is sent once.
            events: std::mem::take(&mut self.pending_events),
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
    use protocol::{decode_host, encode_client, decode_client, encode_host};
    use sim::vec2;

    /// Send a host message through the byte encoding, like a real wire would.
    fn over_the_wire(msg: &HostMsg) -> HostMsg {
        decode_host(&encode_host(msg)).expect("host message survives encoding")
    }

    fn join_as_client(host: &mut Host) -> Replica {
        let welcome = host.join().expect("a free slot");
        match over_the_wire(&HostMsg::Welcome(welcome)) {
            HostMsg::Welcome(w) => Replica::new(w),
            _ => unreachable!(),
        }
    }

    #[test]
    fn replica_follows_the_host_through_bytes() {
        let mut host = Host::new(4, 7);
        let mut replica = join_as_client(&mut host);
        assert_eq!(replica.me, 0);

        // Walk right for a second, sending the input as bytes too.
        let input = Input { move_dir: vec2(1.0, 0.0), ..Input::default() };
        let start_x = replica.world.players[0].pos.x;
        for _ in 0..60 {
            let sent = decode_client(&encode_client(&ClientMsg::Input(input))).unwrap();
            host.handle(replica.me, sent);
            if let Some(msg) = host.tick() {
                replica.apply(over_the_wire(&msg));
            }
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
        let mut host = Host::new(4, 7);
        let mut replica = join_as_client(&mut host);
        // Tile (5,5) is a brick in the Grid map. Announce its destruction
        // without touching the host's own map, so that only the *event* can be
        // what changes the replica.
        assert_eq!(replica.world.map.get(5, 5), sim::Tile::Brick(3));
        host.pending_events.push(Event::TileChanged { x: 5, y: 5, tile: sim::Tile::Empty });
        let msg = host.tick().unwrap();
        replica.apply(over_the_wire(&msg));
        assert_eq!(replica.world.map.get(5, 5), sim::Tile::Empty);
    }

    #[test]
    fn a_late_joiner_gets_the_current_map() {
        let mut host = Host::new(4, 7);
        host.world.map.set(5, 5, sim::Tile::Empty);
        let replica = join_as_client(&mut host);
        assert_eq!(replica.world.map.tiles(), host.world().map.tiles());
    }

    #[test]
    fn join_leave_and_full_match() {
        let mut host = Host::new(2, 1);
        assert_eq!(host.join().unwrap().you, 0);
        assert_eq!(host.join().unwrap().you, 1);
        assert!(host.join().is_none(), "no free slot left");
        host.leave(0);
        assert_eq!(host.join().unwrap().you, 0, "a bot gave the slot back");
    }

    #[test]
    fn inputs_for_bot_slots_are_ignored() {
        let mut host = Host::new(2, 1);
        // Nobody joined, so slot 1 is a bot. A forged input must not move it.
        let forged = Input { fire: true, ..Input::default() };
        host.handle(1, ClientMsg::Input(forged));
        assert!(matches!(host.controllers[1], Controller::Bot(_)));
    }

    #[test]
    fn garbage_bytes_are_dropped_not_fatal() {
        assert!(decode_client(&[1, 2, 3]).is_none());
        assert!(decode_host(&vec![0xFF; 100_000]).is_none());
    }
}
