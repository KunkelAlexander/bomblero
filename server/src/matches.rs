// Matches the server hosts itself: the "dedicated server" role.
//
// "Host is a role, not a place" (see the `host` crate). Until now the role ran
// in one player's browser. Here it runs on the server, next to the relay, and
// nothing about the clients changes: a player in a server-hosted match is an
// ordinary joiner, talking to a room whose host happens to be this program.
//
//   player ──> relay (rooms.rs) ──> virtual host connection ──> this task
//   player <── relay <────────────────────────────────────────  Host::tick()
//
// The trick that keeps this small: the match task is just one more client of
// `Rooms`. It creates a room like a browser host would, and reads the room's
// messages from a channel instead of a WebSocket. So the relay code is not
// touched, and the match logic is the very same `host::Session` that
// `game/src/share.rs` uses.
//
// Networking concept: why a server in the middle beats a browser host. A
// browser host's own player has no network delay and everybody else pays the
// whole round trip, and if the host's wifi hiccups the match hiccups for all.
// Here every player has the same single leg to the same machine, so nobody has
// an advantage, and with the server placed between the players (say Singapore
// for friends in Asia and Europe) nobody has a long way to go.

use std::time::Duration;

use host::{Host, Session};
use protocol::{decode_from_relay, FromRelay, Reject, Rules, ToRelay};
use sim::tilemap::MAPS;
use sim::DT;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::rooms::{send, Role, Tx};
use crate::Shared;

/// Matches running at once. Each ticks the sim 60 times a second, and a free
/// hosting plan has a fraction of a CPU, so this is deliberately small.
pub const MAX_MATCHES: usize = 8;

/// Seats in a server match: four players, bots in the empty ones (as in a
/// browser-hosted room).
const SEATS: usize = 4;

/// After somebody wins, wait this long (8 s) on the result screen and start
/// the next match. In a browser-hosted match the host presses R; nobody is in
/// charge here, so the server does it.
const REMATCH_AFTER_TICKS: u32 = 8 * 60;

/// Handle `CreateHostedRoom` from `tx`: open a room, start the match task that
/// hosts it, and seat the creator in it.
pub fn start(rooms: &Shared, role: &mut Role, tx: &Tx, mut rules: Rules) {
    // Never trust a client's numbers: an unknown map would index past the end
    // of `MAPS` and kill the match task.
    rules.map = rules.map.min(MAPS.len() as u8 - 1);
    rules.score_limit = rules.score_limit.clamp(1, 99);

    let mut guard = rooms.lock().unwrap();
    if !matches!(role, Role::Lobby) {
        return send(tx, &FromRelay::Rejected(Reject::AlreadyInRoom));
    }
    if guard.hosted_matches >= MAX_MATCHES {
        return send(tx, &FromRelay::Rejected(Reject::ServerBusy));
    }

    // The virtual host connection: a room host whose "socket" is a channel.
    let (host_tx, host_rx) = unbounded_channel();
    let mut host_role = Role::Lobby;
    guard.handle(&mut host_role, &host_tx, ToRelay::CreateRoom);
    let Role::Host { code } = &host_role else { unreachable!("a lobby connection can always create a room") };
    let code = code.clone();

    // The creator hears the code first (to share it), then is simply joined.
    send(tx, &FromRelay::RoomCreated { code: code.clone() });
    guard.handle(role, tx, ToRelay::JoinRoom { code });
    guard.hosted_matches += 1;
    drop(guard);

    tokio::spawn(run(rooms.clone(), host_role, host_tx, host_rx, rules));
}

/// One match, from the first player's arrival to the last one's departure.
async fn run(rooms: Shared, mut role: Role, tx: Tx, mut inbox: UnboundedReceiver<Vec<u8>>, rules: Rules) {
    let mut host = Host::new(SEATS, rules, rand::random());
    let mut session = Session::new();
    let mut won_for = 0;

    // Pattern: Game Loop, server edition. There is no display here, so the
    // timer alone drives the sim at a fixed 60 Hz. If the server stalls, the
    // interval catches up with the missed ticks (its default behaviour).
    let mut clock = tokio::time::interval(Duration::from_secs_f32(DT));
    loop {
        clock.tick().await;

        // What the relay said since the last tick: players arriving, leaving,
        // and their inputs. Collect the answers and send them in one go, so
        // the lock on `Rooms` is taken once a tick.
        let mut out = Vec::new();
        while let Ok(bytes) = inbox.try_recv() {
            if let Some(msg) = decode_from_relay(&bytes) {
                out.extend(session.on_relay(&mut host, msg));
            }
        }
        // The last player left: nobody is watching, so the match ends.
        if session.friends() == 0 {
            break;
        }

        let snapshot = host.tick();
        if snapshot.winner.is_some() {
            won_for += 1;
        }
        out.extend(session.broadcast(snapshot));
        if won_for >= REMATCH_AFTER_TICKS {
            won_for = 0;
            host = Host::new(SEATS, rules, rand::random());
            out.extend(session.welcome_again(&mut host));
        }

        let mut guard = rooms.lock().unwrap();
        for msg in out {
            guard.handle(&mut role, &tx, msg);
        }
    }

    // Close the room (the relay tells everyone still in it) and free the slot.
    let mut guard = rooms.lock().unwrap();
    guard.leave(&role);
    guard.hosted_matches -= 1;
}
