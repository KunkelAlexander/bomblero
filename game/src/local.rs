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

use host::Host;
use protocol::replica::Replica;
use protocol::{ClientMsg, HostMsg, PlayerId};
use sim::Input;

use crate::net::Loopback;

pub const PLAYER_COUNT: usize = 4; // you + 3 bots

pub struct LocalGame {
    host: Host,
    link: Loopback,
    /// Which player the host gave this connection (host-side knowledge: it
    /// is how `host.handle` is told who a message came from).
    peer: PlayerId,
    /// The client's copy of the world. This is what gets drawn.
    pub replica: Replica,
}

impl LocalGame {
    pub fn new(seed: u32) -> LocalGame {
        let mut host = Host::new(PLAYER_COUNT, seed);
        let mut link = Loopback::default();

        // The handshake, exactly as it will happen online: the newcomer
        // arrives, the host answers with a Welcome (the full state of the
        // match), and the client builds its replica from it.
        let welcome = host.join().expect("a fresh match has free slots");
        let peer = welcome.you;
        link.host_send(&HostMsg::Welcome(welcome));
        let replica = match link.client_recv() {
            Some(HostMsg::Welcome(w)) => Replica::new(w),
            _ => unreachable!("the loopback delivers the Welcome we just sent"),
        };

        LocalGame { host, link, peer, replica }
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
    /// out a snapshot if one is due.
    pub fn host_tick(&mut self) {
        while let Some(msg) = self.link.host_recv() {
            self.host.handle(self.peer, msg);
        }
        if let Some(snapshot) = self.host.tick() {
            self.link.host_send(&snapshot);
        }
    }

    pub fn bytes_up(&self) -> u64 {
        self.link.bytes_up
    }

    pub fn bytes_down(&self) -> u64 {
        self.link.bytes_down
    }
}
