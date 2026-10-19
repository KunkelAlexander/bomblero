# How the multiplayer works

This document follows one key press from your keyboard to a friend's screen,
then explains each idea you meet on the way. Every idea also has a comment next
to the code that uses it; the file names below tell you where.

## The big picture

```
 JOINER (a friend's browser)           RELAY (small server)           HOST (your browser)
 ───────────────────────────           ────────────────────           ───────────────────
 keyboard                                                              
   │ Input (≤ 60/s)                                                    
   └────────────────────────────────────> forwards bytes ───────────────> Host::handle(slot, input)
                                          (never decodes them)            │
                                                                          ▼  every 1/60 s
                                                                       Host::tick()  → World::step
                                                                          │ Snapshot
                                                                          ▼ Throttle: every 2nd tick
   ┌──────────────────────────────────── fans out to everyone <──────── Share::broadcast
   ▼
 Replica (queue, plays 100 ms late)
   │
   ▼
 Presenter draws it
```

Your own player, on the host, takes a shortcut: its input and its snapshots go
through an in-memory `Loopback` instead of the relay (`game/src/local.rs`). It
speaks the same messages, so offline play, hosting and joining all run the same
pipeline.

## Who does what (the three roles)

| Role | Runs | Knows about | Code |
|---|---|---|---|
| **Host** | the real `World`, at 60 ticks/s | slots, inputs, snapshots; *not* sockets | `host/src/lib.rs` |
| **Relay** | nothing of the game | rooms and who is connected; *not* the rules | `server/src/` |
| **Client** | a copy (`Replica`), never `World::step` | what to draw; *not* the rules | `protocol/src/replica.rs`, `game/src/join.rs` |

**Authority.** Only the host changes the world. A client sends *wishes*
("I am pressing fire") and receives *facts* ("player 2 has 40 hp"). It cannot
say "I hit him" or "I am at x = 5". That is what makes cheating hard and keeps
everyone's world identical.

**"Host is a role, not a place."** Today the host is one player's browser. The
host logic (`Host`) touches no window, socket or clock, so it can move into a
dedicated server program later without changing the clients. `Share`
(`game/src/share.rs`) is the glue that connects a `Host` to the relay; it would
move with it.

## The messages (`protocol/src/lib.rs`)

| Direction | Message | When | Size |
|---|---|---|---|
| client → host | `Input` | every sim tick the player is connected | ~25 bytes |
| client → host | `Loadout` | once, right after the `Welcome` | 5 bytes. The host accepts only the first valid one: a second would refill your ammo |
| client → host | `Ping` | once a second | tiny |
| host → client | `Welcome` | once, when you join (and after a rematch) | the match rules (map number, kills to win) and the whole terrain, several KB |
| host → client | `Snapshot` | 30 per second | ~400 bytes |
| host → client | `Pong`, `Full` | answers | tiny |

Messages are turned into bytes with `bincode` right before sending and back
right after receiving (even on the in-memory loopback, so the encoding is
exercised on every run). Bytes from the outside are never trusted: oversized
messages are rejected before decoding and garbage is dropped, never fatal.

## Ideas, in the order you meet them

### Tick
The sim advances in fixed steps of 1/60 s. The step counter (`tick`) labels
every snapshot, so a client knows *which moment* of the match it is looking at.
Fixed steps also mean the same inputs always give the same world (a test checks
this).

### Command: the `Input` struct
Keyboard, bots and network clients all produce the same small `Input`
(`sim/src/input.rs`). The host does not care where one came from. Only the
*newest* input per player is kept: if two arrive between ticks, the older one is
already out of date.

### Who is who: slots
The relay numbers connections (`PeerId`); the match numbers players
(`PlayerId`). `Share` keeps the table between them, and the slot always comes
from that table, never from the message, so a client cannot pretend to be
someone else. Inputs for a slot held by a bot are ignored.

### Late join and the `Welcome`
A friend can arrive mid-match. The host answers with a `Welcome`: the match's
rules (which map, how many kills win: the host chose them, so a joiner never
does), the full terrain, the current world and the slot you control. Afterwards snapshots carry
only what changes.

### Snapshots: state versus events
A snapshot has two kinds of content:

* **State** (positions, hp, projectiles): if one snapshot is lost or skipped,
  the next one replaces it. Skipping is harmless.
* **Events** (an explosion, a brick destroyed, a kill): they happen once. If an
  explosion was in a skipped snapshot, the friend would never see it. So events
  are *queued* and ride along with the next snapshot that is really sent
  (`host/src/throttle.rs`, Pattern: Event Queue).

The terrain is not resent. A destroyed brick travels as a small
`TileChanged` event and the client patches its map (Pattern: Dirty Flag).

### Snapshot rate and bandwidth
The sim runs at 60 Hz but friends get 30 snapshots per second. Measured with a
unit test (4 players):

| Snapshots per second | Host upload |
|---|---|
| 60 | 24 KB/s |
| 30 | 12 KB/s |

Home connections are slow *going up*, and the host's upload is the scarce
resource. The relay copies each snapshot to every friend, so the host uploads it
once however many friends there are. The price of the lower rate is that the
raw world moves in 33 ms steps; interpolation hides that. The host's own player
keeps all 60, because the loopback costs nothing.

### Interpolation (and why you see the past)
Snapshots arrive in steps and with jitter. Drawing the newest one the moment it
arrives would make everything jerk. Instead the `Replica` queues snapshots and
plays them back **6 ticks (100 ms) behind** the newest one, drawing the moment
*between* the two snapshots around its playhead (`protocol/src/replica.rs`,
Pattern: Double Buffer). A slightly late packet then arrives before it is
needed and nobody notices. Press **I** on a joiner to turn this off and see the
difference.

The cost: everything on a joiner's screen, including their own player, is about
100 ms old, on top of the network delay.

### Latency
Press a key on a joiner. The time until you see the result is roughly:

```
 input travels to the host      ~ ping / 2
 waits for the next host tick   ~ 0 to 16 ms
 snapshot travels back          ~ ping / 2
 waits for the next snapshot    ~ 0 to 33 ms
 playback delay                 100 ms
```

With a 50 ms ping that is about 175 ms. The ping shown in the corner is the
round trip to the *host* (a token the host sends straight back), so it includes
the host's frame time.

The host has an unfair advantage: its own player has no network delay at all.
There is no client-side prediction yet. If joining feels laggy, the usual cure
is to move your *own* player locally and correct it when the host disagrees;
we measure first and add it only if needed.

### Reliable and ordered: WebSocket
A browser may only open a WebSocket to a server (not raw UDP). A WebSocket is
TCP underneath: every message arrives, in order. That is convenient (no lost
events, no re-ordering code) but has a cost called head-of-line blocking: if one
packet is lost, everything behind it waits for the resend, which shows up as a
short freeze. Fast action games on native platforms often use UDP for exactly
this reason. For friends on decent connections it is fine; WebRTC data channels
(unreliable mode) would be the browser's way out if it ever is not.

The game loop never waits for the network. The socket (`game/src/socket.rs`)
only reports `Connecting`, `Open` or `Closed`, and the frame loop reads whatever
has arrived.

### The relay
`server/src/` is deliberately dumb. It creates rooms with 4-character codes
(no look-alike letters), numbers the people in them, and forwards opaque bytes:
a joiner can only talk to the host, the host can talk to one person or to
everybody. The relay stamps the sender on each message so nobody can forge it.
If the host leaves, the room closes. It cannot cheat because it knows no rules,
and it cannot break when the game changes because it never decodes game
messages.

### Matches the server hosts (the "O" key)
Press **O** on the menu and the relay does something new: it runs the match
itself (`server/src/matches.rs`). The creator sends `CreateHostedRoom` with the
rules and is seated in the room like any joiner; friends use the usual code.

The server needed almost no new machinery. The match task is one more client of
the room code: it creates a room like a browser host would, but reads its
messages from a channel instead of a WebSocket, and the logic in between is the
same `host::Session` the browser host uses (`host/src/session.rs`). Every player
is now a plain joiner, so nobody has the host's zero-lag advantage, the host's
wifi and hidden tab no longer matter, and the ping is one leg to the server
instead of two to the relay. The server restarts the match 8 seconds after
somebody wins (nobody holds the R key) and closes the room when the last player
leaves. It runs at most 8 matches at once; a ninth creator gets `ServerBusy`.
A match costs about 0.1 % of a core.

## Limits you should know about

* **A hidden host tab freezes the match.** Browsers pause or slow background
  tabs, and the host's clock is the match's clock. The host must keep its tab
  visible. A dedicated server would remove this.
* **The host's connection is everyone's connection.** If the host's wifi
  stutters, the whole match stutters.
* **No prediction, no rewind.** Firing at a moving target on a joiner is
  judged by the host's world, 100 ms or more after what the joiner saw.
* **Rooms live in the relay's memory.** If the relay restarts, rooms are gone.
  Run exactly one relay instance.
* **No rate limiting or accounts.** Fine for friends; not for the open internet.
* **The native build speaks `ws://` only** (no TLS), so it cannot reach a relay
  that requires `wss://`, which is what hosting services provide. Only the
  browser build can use a deployed relay.

## Where to read the code

| To see... | Read |
|---|---|
| the messages | `protocol/src/lib.rs`, `protocol/src/relay.rs` |
| the authority | `host/src/lib.rs` |
| snapshot thinning | `host/src/throttle.rs` |
| hosting a room | `game/src/share.rs`, `game/src/local.rs` |
| joining | `game/src/join.rs` |
| the client's copy and interpolation | `protocol/src/replica.rs` |
| the relay | `server/src/lib.rs`, `server/src/rooms.rs` |
| the browser WebSocket | `game/src/socket.rs`, `web/net.js` |
