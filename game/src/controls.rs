// Turns keyboard and mouse into a `sim::Input`.
//
// This is the only code that knows which key does what. The sim just receives
// an `Input` - the same struct a bot or (later) a remote player produces.
// Rebinding keys or adding gamepad support only touches this file.

use macroquad::prelude::*;
use sim::{Input, World};

use crate::view;

pub struct Controls {
    /// The selected loadout slot. Key presses are events, but the sim wants
    /// state ("slot 2 is selected"), so we remember it here and send it
    /// every tick.
    selected: u8,
    seq: u32,
}

impl Controls {
    pub fn new() -> Controls {
        Controls { selected: 0, seq: 0 }
    }

    /// Read the devices once per rendered frame. The result is reused for
    /// every sim tick that runs in that frame.
    pub fn read(&mut self, world: &World, me: usize) -> Input {
        let player = &world.players[me];
        let slots = player.weapons.len() as i32;

        let keys = [
            KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5,
            KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9, KeyCode::Key0,
        ];
        for (slot, key) in keys.iter().enumerate().take(slots as usize) {
            if is_key_pressed(*key) {
                self.selected = slot as u8;
            }
        }
        let wheel = mouse_wheel().1;
        let wheel_step = if wheel > 0.0 { -1 } else if wheel < 0.0 { 1 } else { 0 };
        let cycle = is_key_pressed(KeyCode::E) as i32 - is_key_pressed(KeyCode::Q) as i32 + wheel_step;
        if cycle != 0 {
            self.selected = (self.selected as i32 + cycle).rem_euclid(slots) as u8;
        }

        let mut dir = Vec2::ZERO;
        if is_key_down(KeyCode::W) { dir.y -= 1.0; }
        if is_key_down(KeyCode::S) { dir.y += 1.0; }
        if is_key_down(KeyCode::A) { dir.x -= 1.0; }
        if is_key_down(KeyCode::D) { dir.x += 1.0; }

        // Aim as an angle from the player to the mouse, in world coordinates.
        let mouse = view::screen_to_world(Vec2::from(mouse_position()));
        let to_mouse = mouse - player.pos;

        self.seq += 1;
        Input {
            seq: self.seq,
            move_dir: dir.normalize_or_zero(),
            aim: if to_mouse == Vec2::ZERO { player.aim } else { to_mouse.y.atan2(to_mouse.x) },
            fire: is_mouse_button_down(MouseButton::Left),
            weapon: self.selected,
        }
    }
}
