// The weapon picker: choose which of the weapons you carry into the match, like
// Liero's weapon menu. It sits between the menu and the match, so it is reached
// from every way of playing (offline, hosting, joining).
//
// Why a loadout at all: with ten weapons a fixed set would be a list of
// everything, and the choice is half the game. Picking five means a player can
// be the one with the Nova and the Railgun, and the others have to play around it.
//
// This screen only collects a choice. It touches no world and no network: the
// choice is handed to the match, which sends it to the host as a message, and
// the host decides whether it is acceptable (see `Host::handle`).

use macroquad::prelude::*;
use sim::weapons::{
    ARSENAL, BLASTER, BOMB, BOUNCER, CLUSTER, LOADOUT_SIZE, NOVA, RAILGUN, ROCKET, SCATTER, SWARM, TELEPORT, WEAPONS,
};

use crate::draw::{center_text, player_color};
use crate::menu::Choice;
use crate::view::{screen_to_world, Glow};

const KEYS: [KeyCode; 10] = [
    KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5,
    KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9, KeyCode::Key0,
];

const TEXT: Color = Color::new(0.92, 0.93, 0.96, 1.0);
const TEXT_DIM: Color = Color::new(0.45, 0.47, 0.52, 1.0);
const CARD_BG: Color = Color::new(0.075, 0.08, 0.095, 1.0);

const CARD_W: f32 = 560.0;
const CARD_H: f32 = 76.0;
const CARD_GAP: f32 = 10.0;

/// One line about each weapon, for the card. Client-only text, so it lives here
/// and not in the sim's weapon table (which only holds what the rules need).
fn blurb(id: u8) -> &'static str {
    match id {
        BLASTER => "fast bolts, a big magazine",
        SCATTER => "shotgun: eight pellets, short range",
        ROCKET => "explodes on impact and digs a crater",
        BOUNCER => "ricochets off walls",
        BOMB => "dropped at your feet: a plus-shaped blast",
        RAILGUN => "a beam through players and two bricks",
        CLUSTER => "a shell that bursts into seven bomblets",
        SWARM => "six missiles that curl towards the nearest enemy",
        TELEPORT => "jump to where it lands, or swap places with who it hits",
        NOVA => "one huge blast that clears a room; very slow to reload",
        _ => "",
    }
}

pub struct Armory {
    /// Where to go once the choice is made. `Some` until it is handed back.
    choice: Option<Choice>,
    /// The picked weapon ids, in the order they were picked: that order is the
    /// order of keys 1-5 in the match.
    picked: Vec<u8>,
}

impl Armory {
    /// `start` is the previous choice (or the default), so a player who is happy
    /// with it just presses Enter.
    pub fn new(choice: Choice, start: Vec<u8>) -> Armory {
        Armory { choice: Some(choice), picked: start }
    }

    /// One frame. Returns what to play, and with which weapons, once chosen.
    pub fn frame(&mut self, glow: &Glow) -> Option<(Choice, Vec<u8>)> {
        let mouse = screen_to_world(Vec2::from(mouse_position()));
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        for (i, &id) in ARSENAL.iter().enumerate() {
            if is_key_pressed(KEYS[i]) || (clicked && card_rect(i).contains(mouse)) {
                self.toggle(id);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.picked.clear();
        }
        let ready = self.picked.len() == LOADOUT_SIZE;

        self.draw(mouse, ready, glow);

        if ready && is_key_pressed(KeyCode::Enter) {
            return Some((self.choice.take()?, self.picked.clone()));
        }
        None
    }

    /// Pick a weapon, or put it back. A full set refuses a sixth.
    fn toggle(&mut self, id: u8) {
        if let Some(at) = self.picked.iter().position(|&p| p == id) {
            self.picked.remove(at);
        } else if self.picked.len() < LOADOUT_SIZE {
            self.picked.push(id);
        }
    }

    fn draw(&self, mouse: Vec2, ready: bool, glow: &Glow) {
        glow.begin();
        let mine = player_color(0);
        center_text(&format!("CHOOSE YOUR {LOADOUT_SIZE} WEAPONS"), 84.0, 44.0, TEXT);
        center_text("click a weapon or press its number (click a picked one to put it back). The order you pick is the order of keys 1-5.", 124.0, 22.0, TEXT_DIM);

        for (i, &id) in ARSENAL.iter().enumerate() {
            let r = card_rect(i);
            let slot = self.picked.iter().position(|&p| p == id);
            let hover = r.contains(mouse);
            draw_rectangle(r.x, r.y, r.w, r.h, if hover { Color::new(0.11, 0.12, 0.145, 1.0) } else { CARD_BG });
            if slot.is_some() {
                // Your colour marks what is yours, as it does on the HUD.
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(mine.r, mine.g, mine.b, 0.1));
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, mine);
            }

            // Left: the key to press. Picked weapons show their slot instead,
            // in a hexagon, the game's own shape.
            let (cx, cy) = (r.x + 34.0, r.y + r.h / 2.0);
            match slot {
                Some(n) => {
                    // Dark body, bright outline and number: a filled bright
                    // hexagon would glow and wash the number out.
                    draw_poly(cx, cy, 6, 20.0, 0.0, Color::new(mine.r * 0.2, mine.g * 0.2, mine.b * 0.2, 1.0));
                    draw_poly_lines(cx, cy, 6, 20.0, 0.0, 2.5, mine);
                    draw_text(&format!("{}", n + 1), cx - 5.0, cy + 8.0, 26.0, WHITE);
                }
                None => {
                    draw_poly_lines(cx, cy, 6, 20.0, 0.0, 1.5, TEXT_DIM);
                    draw_text(&format!("{}", (i + 1) % 10), cx - 4.0, cy + 6.0, 20.0, TEXT_DIM);
                }
            }

            let def = &WEAPONS[id as usize];
            let (name, detail) = if slot.is_some() { (TEXT, TEXT_DIM) } else { (Color::new(0.7, 0.72, 0.78, 1.0), TEXT_DIM) };
            draw_text(def.name, r.x + 70.0, r.y + 31.0, 26.0, name);
            draw_text(blurb(id), r.x + 70.0, r.y + 56.0, 18.0, detail);
            let rounds = if def.mag == 1 { "round" } else { "rounds" };
            let magazine = format!("{} {rounds}  reload {:.1}s", def.mag, def.reload);
            let width = measure_text(&magazine, None, 16, 1.0).width;
            draw_text(&magazine, r.x + r.w - 12.0 - width, r.y + 31.0, 16.0, TEXT_DIM);
        }

        if ready {
            center_text("Enter: go", 640.0, 34.0, mine);
        } else {
            let missing = LOADOUT_SIZE - self.picked.len();
            center_text(&format!("pick {missing} more"), 640.0, 34.0, TEXT_DIM);
        }
        center_text("Backspace: clear     Esc: back to the menu", 690.0, 20.0, TEXT_DIM);
        glow.present();
    }
}

/// Two columns of five cards, filled column by column: the left one holds the
/// arsenal's first five weapons.
fn card_rect(i: usize) -> Rect {
    let (col, row) = (i / 5, i % 5);
    let x = 60.0 + col as f32 * (CARD_W + 40.0);
    Rect::new(x, 168.0 + row as f32 * (CARD_H + CARD_GAP), CARD_W, CARD_H)
}
