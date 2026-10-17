# Neon Bomber-Liero

A fast top-down arena shooter ("Bomberman meets Liero") in a neon style, written
in Rust with macroquad. It runs in the browser (WASM) and natively. It uses no
external assets: graphics are procedural shapes and sounds are synthesized in
code.

It is also a learning project: the code is commented to teach how online
multiplayer and game-programming patterns work.

## Run it

```
cargo run -p game --release          # native
cargo run -p server --release        # relay for online play (port 9001)
cargo test                           # headless tests, no window needed
```

Browser build: see [docs/DEPLOY.md](docs/DEPLOY.md). Menu keys: **P** practice
against bots, **H** host a room, **J** join one by its 4-letter code.
Controls: WASD move, mouse aim, left button fire, 1-5 or Q/E switch weapon.

## Where to read next

| | |
|---|---|
| [PLAN.md](PLAN.md) | decisions, weapon list, milestones |
| [docs/NETWORKING.md](docs/NETWORKING.md) | how a key press travels to a friend's screen |
| [docs/DEPLOY.md](docs/DEPLOY.md) | putting the game and the relay online |
| [CLAUDE.md](CLAUDE.md) | the coding rules and architecture rules |

## Why does online play need a server at all?

Short answer: browsers can't accept incoming connections, so two players can't
simply connect to each other.

* **Nobody can connect to you first.** A direct connection needs one side to
  open a connection *to* the other. Home computers sit behind a router (NAT)
  that drops unsolicited incoming connections, and a web page cannot listen on a
  port anyway. Browsers only make *outgoing* connections, and a WebSocket only
  goes to a server.
* **The two browsers must find each other.** Even WebRTC, the browser's real
  peer-to-peer technology, needs a middleman (a *signalling server*) to swap
  addresses and keys before the direct link exists. Often it also needs a STUN
  server to learn your public address, and sometimes a TURN relay, because some
  networks allow no direct path. Then the traffic goes through a server anyway.
* **Room codes are a lookup.** "Type `K7QX` to join" needs something that holds
  the table of codes.

So a server is unavoidable. What it *does* is a choice:

| | Who runs the match | Server's job |
|---|---|---|
| **Today: relay** | one player's browser (the *host*) | create rooms, forward bytes, never decode them |
| **Later: dedicated server** | the server, which links `sim` and `host` | everything; all players are plain clients |

The relay is the simplest middleman, a post office that knows no game rules. It
costs joiners one extra hop (maybe 10-30 ms) and is far simpler than WebRTC,
which would remove the hop but still need a signalling server, fail silently on
some networks, and has thin Rust/WASM support.

Peer-to-peer would not remove the *host* problem either: someone has to be the
authority, or every peer simulates its own copy and they drift apart. That is
why "host is a role, not a place" is an architecture rule here: moving the role
from a browser to a server must not change the clients.

### Should the server run the match?

Probably yes eventually. It is the dedicated-server move that `PLAN.md` keeps
open.

**Gains**

* The hidden-tab freeze disappears (a background browser tab is throttled,
  which today freezes the match for everyone).
* One player's bad wifi no longer hurts everyone.
* Nobody has a built-in latency edge, and joiners get faster, because their
  input goes straight to the match instead of via the relay and then the host.
* Cheating gets harder, and matches can outlive whoever started them.

**Costs**

* It needs a server that never sleeps. Render's free tier sleeps after 15
  minutes idle and takes about a minute to wake, so realistically that means
  Fly at roughly $2 a month. CPU and bandwidth are not the problem: a 4-6
  player match is trivial for Rust, at about 12 KB/s per player.
* The relay becomes a game server: it must decode messages, run the tick loop
  and own the player slots (the code in `game/src/share.rs` and `host/`).
* The hosting player loses their zero-lag advantage, which is fairer but makes
  client-side prediction (M5) worth considering.

**How much work.** Less than it sounds, because `Host` already takes decoded
messages and returns snapshots without touching a socket. The work is moving
`Share` and the tick loop into the `server` crate and making "Host a game"
mean "create a room on the server". `sim`, `protocol` and `Replica` stay as
they are.

**Serving the web files too.** The server could also return `index.html` and the
`.wasm`, giving one deployment and no `RELAY_URL` build setting. But on a
sleeping free tier even loading the page would wait a minute, so keep the
static files on GitHub Pages, which is free and always awake.

**Plan:** deploy the current version first and play it with a friend. If the
host-tab freeze or the host's lag bothers you, do the move then.
