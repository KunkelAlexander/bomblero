// The client's copy of the match.
//
// A client never runs `World::step`. It keeps a `World` purely as a container
// and overwrites it with whatever the host last said. That is a *replica*.
// Because it is a real `World`, all the drawing code works on it unchanged:
// rendering cannot tell whether it is looking at the real thing (offline) or a
// copy (online).

use sim::tilemap::MAP_GRID;
use sim::{Event, Tilemap, World};

use crate::{HostMsg, PlayerId, Snapshot, Welcome};

pub struct Replica {
    pub world: World,
    /// Which player this client controls.
    pub me: PlayerId,
    /// Events received but not yet shown (explosions, sounds...).
    events: Vec<Event>,
}

impl Replica {
    pub fn new(welcome: Welcome) -> Replica {
        // Any valid map will do as a starting point: the real tiles replace it
        // on the next line. (With several maps, `Welcome` will name the map.)
        let map = Tilemap::from_ascii(MAP_GRID);
        let mut world = World::new(map, welcome.snapshot.players.len(), &[], 0);
        world.map.load_tiles(&welcome.tiles);
        let mut replica = Replica { world, me: welcome.you, events: Vec::new() };
        replica.apply_snapshot(welcome.snapshot);
        replica
    }

    /// Feed in one message from the host.
    pub fn apply(&mut self, msg: HostMsg) {
        match msg {
            // A second Welcome means the host restarted the match.
            HostMsg::Welcome(w) => *self = Replica::new(w),
            HostMsg::Snapshot(s) => self.apply_snapshot(s),
        }
    }

    fn apply_snapshot(&mut self, snap: Snapshot) {
        // Overwrite, don't merge: the host is the authority, so its word is
        // final. (Over a lossy network like UDP we would also drop a snapshot
        // older than the one we already have. WebSocket is ordered, so we
        // don't need to.)
        self.world.tick = snap.tick;
        self.world.players = snap.players;
        self.world.projectiles = snap.projectiles;
        self.world.winner = snap.winner;

        for event in &snap.events {
            // Pattern: Dirty Flag. The map is not resent; we patch only the
            // tiles the host says changed.
            if let Event::TileChanged { x, y, tile } = *event {
                self.world.map.set(x, y, tile);
            }
        }
        self.events.extend(snap.events);
    }

    /// Same idea as `World::drain_events`, for the events that arrived by network.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}
