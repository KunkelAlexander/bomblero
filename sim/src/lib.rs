// The `sim` crate is the game's rules and nothing else.
//
// What it must NEVER depend on: windows, GPU, audio, sockets, wall-clock time.
// Its whole interface is:
//
//     World::step(&mut self, inputs: &[Input])     <- inputs in
//     world.players / projectiles / map            <- state out (read-only for others)
//     world.drain_events()                         <- things that just happened
//
// Rendering, sound and (later) networking are all *consumers* of that
// interface. This separation is what lets the same code run in a browser tab,
// in a bot, in a unit test, or on a dedicated server.

pub mod bots;
pub mod input;
pub mod rng;
pub mod tilemap;
pub mod weapons;
pub mod world;

pub use glam::{vec2, Vec2};
pub use input::Input;
pub use tilemap::{Tile, Tilemap, MAP_H, MAP_W, TILE};
pub use weapons::{WEAPONS, WeaponDef};
pub use world::{Event, Player, Projectile, World, DT, PLAYER_RADIUS};
