// The host session: the bridge between a match and the people connected to it.
//
// The relay numbers connections (`PeerId`), the match numbers players
// (`PlayerId`). This is the table between them, plus the per-link snapshot
// throttle. It speaks only decoded relay messages, never sockets:
//
//   FromRelay::Data { from, bytes } ──> Session::on_relay ──> Host::handle(slot, msg)
//   Host::tick() snapshot           ──> Session::broadcast ──> ToRelay::Data { to: None }
//
// Why it lives here and not in the game: the same code must run wherever the
// `Host` runs. Today that is a player's browser (`game/src/share.rs` wraps a
// WebSocket around it) and on the dedicated server (`server/src/matches.rs`
// wraps a timer around it). Neither wrapper contains any match logic.

use std::collections::HashMap;

use protocol::relay::PeerId;
use protocol::{decode_client, encode_host, ClientMsg, FromRelay, HostMsg, PlayerId, Snapshot, ToRelay};

use crate::{Host, Throttle};

/// Send friends a snapshot every N sim ticks: 2 = 30 per second.
///
/// The trade-off, in numbers (4 players, ~350 bytes per snapshot):
///   1 -> 60/s -> ~21 KB/s of host upload, friends see 60 Hz steps
///   2 -> 30/s -> ~10 KB/s,                friends see 30 Hz steps
///   3 -> 20/s -> ~7 KB/s,                 friends see 20 Hz steps (visibly choppy)
/// 30 is the usual compromise; interpolation hides the steps. Only the link to
/// friends is throttled: a player sitting on the host itself keeps all 60.
const SEND_EVERY: u32 = 2;

pub struct Session {
    throttle: Throttle,
    /// Which player slot each friend got.
    peers: HashMap<PeerId, PlayerId>,
}

impl Session {
    pub fn new() -> Session {
        Session { throttle: Throttle::new(SEND_EVERY), peers: HashMap::new() }
    }

    /// How many friends are connected.
    pub fn friends(&self) -> usize {
        self.peers.len()
    }

    /// One message from the relay in; whatever must go back out is returned,
    /// for the caller to put on the wire. Messages that are not about the
    /// match (room codes, pongs from the relay itself) are ignored here.
    pub fn on_relay(&mut self, host: &mut Host, msg: FromRelay) -> Vec<ToRelay> {
        match msg {
            FromRelay::PeerJoined(peer) => self.welcome(host, peer),
            FromRelay::PeerLeft(peer) => {
                if let Some(slot) = self.peers.remove(&peer) {
                    host.leave(slot);
                }
                Vec::new()
            }
            FromRelay::Data { from, bytes } => self.on_data(host, from, bytes),
            _ => Vec::new(),
        }
    }

    /// Offer this tick's snapshot to the friends; every `SEND_EVERY`-th one
    /// really goes out. One `Data { to: None }` message: the relay copies it
    /// to everybody, so the host uploads each snapshot once no matter how many
    /// friends there are.
    pub fn broadcast(&mut self, snapshot: Snapshot) -> Option<ToRelay> {
        if self.peers.is_empty() {
            // Nobody listening: do not spend upload on it. Also forget the
            // events held back for them. They are older than whatever a future
            // joiner's `Welcome` will say, and replaying them would undo it.
            self.throttle = Throttle::new(SEND_EVERY);
            return None;
        }
        let snapshot = self.throttle.push(snapshot)?;
        Some(ToRelay::Data { to: None, bytes: encode_host(&HostMsg::Snapshot(snapshot)) })
    }

    /// The host started a new `Host` (rematch): every friend needs a fresh
    /// `Welcome` for the new match, exactly like a late joiner.
    pub fn welcome_again(&mut self, host: &mut Host) -> Vec<ToRelay> {
        let peers: Vec<PeerId> = self.peers.keys().copied().collect();
        self.peers.clear();
        // Events held back from the old match must not leak into the new one.
        self.throttle = Throttle::new(SEND_EVERY);
        peers.into_iter().flat_map(|peer| self.welcome(host, peer)).collect()
    }

    /// The relay is gone, so every friend is too: bots take their slots.
    pub fn drop_everyone(&mut self, host: &mut Host) {
        for (_, slot) in self.peers.drain() {
            host.leave(slot);
        }
    }

    /// A friend arrived: give them a slot and the full state of the match.
    fn welcome(&mut self, host: &mut Host, peer: PeerId) -> Vec<ToRelay> {
        match host.join() {
            Some(welcome) => {
                self.peers.insert(peer, welcome.you);
                vec![to_peer(peer, &HostMsg::Welcome(welcome))]
            }
            None => vec![to_peer(peer, &HostMsg::Full)],
        }
    }

    fn on_data(&mut self, host: &mut Host, from: PeerId, bytes: Vec<u8>) -> Vec<ToRelay> {
        // `from` was stamped by the relay and the slot comes from OUR table,
        // never from the message, so a friend cannot act as another player.
        // Someone without a slot (the match was full) is ignored.
        let Some(&slot) = self.peers.get(&from) else { return Vec::new() };
        match decode_client(&bytes) {
            Some(ClientMsg::Ping { token }) => vec![to_peer(from, &HostMsg::Pong { token })],
            Some(msg) => {
                host.handle(slot, msg);
                Vec::new()
            }
            None => Vec::new(), // garbage is dropped, never fatal
        }
    }
}

fn to_peer(peer: PeerId, msg: &HostMsg) -> ToRelay {
    ToRelay::Data { to: Some(peer), bytes: encode_host(msg) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{decode_host, encode_client, Rules};
    use sim::{vec2, Input};

    fn sent_host_msg(out: &ToRelay) -> (Option<PeerId>, HostMsg) {
        match out {
            ToRelay::Data { to, bytes } => (*to, decode_host(bytes).expect("a host message")),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    fn from(peer: PeerId, msg: &ClientMsg) -> FromRelay {
        FromRelay::Data { from: peer, bytes: encode_client(msg) }
    }

    #[test]
    fn a_friend_gets_a_slot_and_a_welcome() {
        let mut host = Host::new(2, Rules::default(), 1);
        let mut session = Session::new();
        let out = session.on_relay(&mut host, FromRelay::PeerJoined(3));
        let (to, msg) = sent_host_msg(&out[0]);
        assert_eq!(to, Some(3), "the Welcome goes to the newcomer only");
        assert!(matches!(msg, HostMsg::Welcome(w) if w.you == 0));
        assert_eq!(session.friends(), 1);
    }

    #[test]
    fn a_full_match_says_so() {
        let mut host = Host::new(1, Rules::default(), 1);
        let mut session = Session::new();
        session.on_relay(&mut host, FromRelay::PeerJoined(1));
        let out = session.on_relay(&mut host, FromRelay::PeerJoined(2));
        assert!(matches!(sent_host_msg(&out[0]).1, HostMsg::Full));
        assert_eq!(session.friends(), 1);
    }

    #[test]
    fn the_slot_comes_from_the_table_not_from_the_message() {
        let mut host = Host::new(2, Rules::default(), 1);
        let mut session = Session::new();
        session.on_relay(&mut host, FromRelay::PeerJoined(1)); // slot 0
        session.on_relay(&mut host, FromRelay::PeerJoined(2)); // slot 1
        let before = host.world().players[0].pos;

        // Peer 2 pushes right. It moves player 1; player 0 stays put.
        let input = Input { move_dir: vec2(1.0, 0.0), ..Input::default() };
        session.on_relay(&mut host, from(2, &ClientMsg::Input(input)));
        for _ in 0..30 {
            host.tick();
        }
        assert_eq!(host.world().players[0].pos, before);
    }

    #[test]
    fn pings_are_answered_and_strangers_ignored() {
        let mut host = Host::new(2, Rules::default(), 1);
        let mut session = Session::new();
        session.on_relay(&mut host, FromRelay::PeerJoined(1));

        let out = session.on_relay(&mut host, from(1, &ClientMsg::Ping { token: 9 }));
        assert!(matches!(sent_host_msg(&out[0]), (Some(1), HostMsg::Pong { token: 9 })));
        // Peer 5 never joined: no slot, no answer.
        assert!(session.on_relay(&mut host, from(5, &ClientMsg::Ping { token: 9 })).is_empty());
        assert!(session.on_relay(&mut host, FromRelay::Data { from: 1, bytes: vec![1, 2, 3] }).is_empty());
    }

    #[test]
    fn leaving_gives_the_slot_back_to_a_bot() {
        let mut host = Host::new(1, Rules::default(), 1);
        let mut session = Session::new();
        session.on_relay(&mut host, FromRelay::PeerJoined(1));
        session.on_relay(&mut host, FromRelay::PeerLeft(1));
        assert_eq!(session.friends(), 0);
        assert!(host.join().is_some(), "the slot is free again");
    }

    #[test]
    fn snapshots_go_out_only_while_somebody_listens() {
        let mut host = Host::new(2, Rules::default(), 1);
        let mut session = Session::new();
        assert!(session.broadcast(host.tick()).is_none());
        session.on_relay(&mut host, FromRelay::PeerJoined(1));
        // Every second tick is due.
        let sent = (0..4).filter_map(|_| session.broadcast(host.tick())).count();
        assert_eq!(sent, 2);
    }

    #[test]
    fn a_rematch_welcomes_everyone_again() {
        let mut host = Host::new(2, Rules::default(), 1);
        let mut session = Session::new();
        session.on_relay(&mut host, FromRelay::PeerJoined(1));
        session.on_relay(&mut host, FromRelay::PeerJoined(2));
        let mut fresh = Host::new(2, Rules::default(), 2);
        let out = session.welcome_again(&mut fresh);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|o| matches!(sent_host_msg(o).1, HostMsg::Welcome(_))));
    }
}
