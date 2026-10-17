# CLAUDE.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.
---

# Project: Neon Bomber-Liero

Top-down, real-time arena shooter ("Bomberman meets Liero") in a neon style. Rust + macroquad, runs in the browser (WASM, GitHub Pages) and natively. The owner mostly directs and wants to **learn** how online multiplayer is built, so the code should teach.

## Code style (project-specific)
- **Keep the code simple.** Plain structs, enums and functions. No generics, traits or macros unless there is a clear payoff. No ECS.
- **Comment the *why*** of design decisions next to the code that implements them, not what each line does.
- **Name the pattern.** When a pattern from *Game Programming Patterns* is used, say so in a comment (e.g. `// Pattern: Type Object`) and why it fits here. Planned uses: Game Loop (fixed timestep), Command (the `Input` struct), Type Object (`WeaponDef` table), Flyweight, State, Object Pool, Event Queue, Double Buffer (snapshots), Dirty Flag (tile diffs), Spatial Partition (tile grid).
- **Explain networking concepts where they appear** (tick, authority, snapshot, interpolation, latency, reliable vs. unreliable), so reading the code teaches online multiplayer. Build netcode in small steps and stop after each one so the owner can run it.
- **No external assets.** Graphics are procedural shapes plus a glow shader; sounds are synthesized in code. Shaders are GLSL 100 strings in the source so they run on both WebGL and native.

## Architecture rules
- All game logic lives in the `sim` **crate** (no macroquad, no I/O, no clock) and is driven only by per-player `Input` structs ("inputs in, state out"). Rendering, audio and networking only *read* state and events; the sim never calls into them. This is what makes bots, local play, online play and unit tests share one code path. If a change would make `sim` depend on the window, GPU, sound or sockets, stop and redesign.
- **Central-server-ready (owner requirement).** "Host" is a role, not a place: whoever calls `World::step` is the authority. Today that will be a player's browser; it must stay possible to move that role into a dedicated server binary that links `sim`, with clients unchanged. So: the host logic (collect inputs by player id, step, build snapshots) is the `Host` struct in the `host` crate: it takes and returns already-decoded messages and never touches rendering or the transport. Message types and the client's `Replica` live in the shared `protocol` crate. Clients never call `World::step`; they draw a `Replica` built from snapshots, even in offline play (the game runs host and client over an in-memory loopback).
- The sim runs at a fixed 60 Hz. Rendering runs at the display rate.
- Weapons are data rows (`WeaponDef`), not types.
- Online is host-authoritative: one player's browser runs the sim, a small relay server (`server` crate, rooms by code, forwards opaque bytes, never decodes game messages) only creates rooms and forwards messages. The game's own WebSocket wrapper is `game/src/socket.rs` + `web/net.js` (quad-net was rejected: it panics on connect failure and cannot report a closed socket). No client-side prediction in v1.

## Commands
- Native: `cargo run -p game --release`
- Web build: `cargo build -p game --release --target wasm32-unknown-unknown`, then serve `web/` (see `web/README` once it exists, or `python -m http.server` in `web/`).
- Relay server: `cargo run -p server --release` (port 9001, or $PORT). The game connects to `ws://127.0.0.1:9001/ws` unless built with `RELAY_URL=wss://...`.
- Tests: `cargo test` (sim, protocol and host tests are headless: no window or GPU needed)

## Plan
The full approved plan (decisions, weapon list, milestones M0–M5) is in `PLAN.md`.
