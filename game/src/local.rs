// A match that runs inside this process, with one human playing it.
//
// Even offline, the human does NOT touch the world directly. The whole online
// pipeline runs, just over an in-memory wire:
//
//   keyboard -> Input -> [wire] -> Host (steps the sim) -> Snapshot -> [wire] -> Replica -> screen
//
// Why bother? Because then online play is not a second code path to get right
// later. Going online means swapping the wire and moving the Host to another
// machine; everything else already works, and runs on every offline game.
//
// Hosting for friends is the same thing plus a `Share`: the Host also listens
// to a relay, and every snapshot goes to the friends as well as to the local
// player. The host's own player never uses the relay; it has the loopback.

use host::Host;
use protocol::replica::Replica;
use protocol::{ClientMsg, HostMsg, PlayerId};
use sim::tilemap::MAP_HUB;
use sim::weapons::ARSENAL;
use sim::Input;

use crate::net::Loopback;
use crate::share::Share;

pub const PLAYER_COUNT: usize = 4; // you + 3 bots (or friends)

pub struct LocalGame {
    host: Host,
    link: Loopback,
    /// Which player the host gave this connection (host-side knowledge: it
    /// is how `host.handle` is told who a message came from).
    peer: PlayerId,
    /// `Some` once a room is open for friends.
    share: Option<Share>,
    /// The client's copy of the world. This is what gets drawn.
    pub replica: Replica,
    /// The weapons this player picked. A rematch is a brand-new `Host`, which
    /// has to be told again, exactly like a friend who rejoins.
    loadout: Vec<u8>,
}

impl LocalGame {
    pub fn new(seed: u32, loadout: &[u8]) -> LocalGame {
        let mut game = LocalGame::with_host(Host::new(PLAYER_COUNT, seed));
        // Even the local player's choice goes through the wire as a message,
        // like a friend's: one code path, and the host checks it either way.
        game.loadout = loadout.to_vec();
        game.link.client_send(&ClientMsg::Loadout(game.loadout.clone()));
        game
    }

    /// The menu hub: one player alone on a small map (see `MAP_HUB`), with every
    /// weapon so the shooting range can try them all.
    pub fn menu(seed: u32) -> LocalGame {
        LocalGame::with_host(Host::on_map(MAP_HUB, 1, seed, &ARSENAL))
    }

    fn with_host(mut host: Host) -> LocalGame {
        let mut link = Loopback::default();

        // The handshake, exactly as it will happen online: the newcomer
        // arrives, the host answers with a Welcome (the full state of the
        // match), and the client builds its replica from it.
        let welcome = host.join().expect("a fresh match has free slots");
        let peer = welcome.you;
        link.host_send(&HostMsg::Welcome(welcome));
        let replica = match link.client_recv() {
            // Delay 0: no interpolation. The wire is a function call, so there
            // is no jitter to hide, and the local player gets no extra lag.
            Some(HostMsg::Welcome(w)) => Replica::new(w, 0),
            _ => unreachable!("the loopback delivers the Welcome we just sent"),
        };

        LocalGame { host, link, peer, share: None, replica, loadout: Vec::new() }
    }

    /// Let friends in: open a room on the relay. Idempotent.
    pub fn open_room(&mut self) {
        if self.share.is_none() {
            self.share = Some(Share::open());
        }
    }

    /// Rematch. Friends stay in the room and get a fresh `Welcome`.
    pub fn restart(&mut self, seed: u32) {
        let share = self.share.take();
        let loadout = std::mem::take(&mut self.loadout);
        *self = LocalGame::new(seed, &loadout);
        if let Some(mut share) = share {
            share.welcome_again(&mut self.host);
            self.share = Some(share);
        }
    }

    /// What to show about the room, if there is one.
    pub fn room_status(&self) -> Option<String> {
        self.share.as_ref().map(Share::status)
    }

    // ---- the client's half ----

    /// Tell the host what the player wants. Called once per rendered frame.
    pub fn send_input(&mut self, input: Input) {
        self.link.client_send(&ClientMsg::Input(input));
    }

    /// Apply everything the host has said since last time.
    pub fn receive(&mut self) {
        while let Some(msg) = self.link.client_recv() {
            self.replica.apply(msg);
        }
    }

    // ---- the host's half ----

    /// One 1/60 s tick: read the inputs that arrived, step the sim, and send
    /// the result out: every tick to the local player (free, in-memory), and
    /// to friends at the rate `Share` chooses.
    pub fn host_tick(&mut self) {
        if let Some(share) = &mut self.share {
            share.receive(&mut self.host);
        }
        while let Some(msg) = self.link.host_recv() {
            self.host.handle(self.peer, msg);
        }
        let snapshot = self.host.tick();
        if let Some(share) = &mut self.share {
            share.broadcast(snapshot.clone());
        }
        self.link.host_send(&HostMsg::Snapshot(snapshot));
    }

    /// Bandwidth for the HUD. Once a room is open this is what the host really
    /// sends and receives over the network; the loopback costs nothing real.
    pub fn bytes_up(&self) -> u64 {
        self.share.as_ref().map_or(self.link.bytes_up, |s| s.bytes_up)
    }

    pub fn bytes_down(&self) -> u64 {
        self.share.as_ref().map_or(self.link.bytes_down, |s| s.bytes_down)
    }
}
