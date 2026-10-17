use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Everything a player can ask the game to do during one tick.
///
/// Pattern: Command. A button press becomes a plain data object instead of a
/// direct call like `player.shoot()`. Because it is just data:
///   * the keyboard, a bot and (later) a packet from a friend all produce the
///     same thing, so the sim has ONE code path for all of them;
///   * it can be sent over the network, stored, or replayed in a test.
///
/// Networking note: the sim treats an Input as *untrusted*. A cheating client
/// can send any numbers, so the sim clamps and sanitises them instead of
/// assuming they are polite.
#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    /// Increases by one per input a client produces. The sim ignores it; the
    /// network layer uses it later to drop stale/duplicate packets.
    pub seq: u32,
    /// Desired movement, length 0..=1 (the sim clamps it).
    pub move_dir: Vec2,
    /// Aim direction as an angle in radians. An angle (not a mouse position)
    /// keeps the input independent of screen size and camera.
    pub aim: f32,
    /// Is the fire button held?
    pub fire: bool,
    /// Which loadout slot is selected. Sent as an absolute slot rather than a
    /// "next weapon" press: if one packet is lost, the next one still carries
    /// the right answer (state beats events on an unreliable network).
    pub weapon: u8,
}
