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
    ARSENAL, BLASTER, BOMB, BOUNCER, BUILD, CLUSTER, LOADOUT_SIZE, NOVA, RAILGUN, ROCKET, SWARM, TELEPORT, WEAPONS,
};

use crate::draw::{button, center_wide, player_color};
use crate::menu::Choice;
use crate::view::{self, screen_to_picture, Glow};

const KEYS: [KeyCode; 10] = [
    KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5,
    KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9, KeyCode::Key0,
];

const TEXT: Color = Color::new(0.92, 0.93, 0.96, 1.0);
// Secondary text. At 0.6 it keeps WCAG's 4.5:1 contrast for small text
// against the dark cards (0.45 was about 3:1: hard to read on a dim screen).
const TEXT_DIM: Color = Color::new(0.6, 0.62, 0.67, 1.0);
const CARD_BG: Color = Color::new(0.075, 0.08, 0.095, 1.0);

const CARD_GAP: f32 = 10.0;

/// One line about each weapon, for the card. Client-only text, so it lives here
/// and not in the sim's weapon table (which only holds what the rules need).
fn blurb(id: u8) -> &'static str {
    match id {
        BLASTER => "fast bolts, a big magazine",
        BUILD => "builds a brick where you aim: cover from blasts",
        ROCKET => "explodes on impact and digs a crater",
        BOUNCER => "ricochets off walls",
        BOMB => "dropped at your feet: a short fuse, and a mine",
        RAILGUN => "an instant beam through players; bricks stop it",
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
        let mouse = screen_to_picture(Vec2::from(mouse_position()));
        let cards = Cards::new();
        for (i, &id) in ARSENAL.iter().enumerate() {
            if is_key_pressed(KEYS[i]) || crate::touch::clicked_in(cards.rect(i)) {
                self.toggle(id);
            }
        }
        let ready = self.picked.len() == LOADOUT_SIZE;
        // The buttons stand in for the keys on a touchscreen.
        let (clear, go) = self.draw(&cards, mouse, ready, glow);
        if is_key_pressed(KeyCode::Backspace) || clear {
            self.picked.clear();
        }

        if ready && (is_key_pressed(KeyCode::Enter) || go) {
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

    /// Returns whether the CLEAR and GO buttons were pressed.
    fn draw(&self, cards: &Cards, mouse: Vec2, ready: bool, glow: &Glow) -> (bool, bool) {
        let mine = player_color(0);
        let height = view::layout().size.y;

        // The neon layer: the heading, the frames of the picked cards and their
        // hexagons glow. Laid out in picture coordinates, so it adapts to the
        // screen's shape.
        glow.begin();
        glow.overlay();
        // A sign: a pale tube in your colour, so the glow around it is
        // coloured and the letters stay readable (see `DOWN_FRAGMENT`).
        let sign = Color::new(0.55 + 0.45 * mine.r, 0.55 + 0.45 * mine.g, 0.55 + 0.45 * mine.b, 1.0);
        center_wide(&format!("CHOOSE YOUR {LOADOUT_SIZE} WEAPONS"), if cards.roomy() { 84.0 } else { 44.0 }, 44.0, sign);
        for (i, &id) in ARSENAL.iter().enumerate() {
            let r = cards.rect(i);
            let slot = self.picked.iter().position(|&p| p == id);
            let hover = r.contains(mouse);
            draw_rectangle(r.x, r.y, r.w, r.h, if hover { Color::new(0.11, 0.12, 0.145, 1.0) } else { CARD_BG });
            // Left: the key to press. Picked weapons show their slot instead,
            // in a hexagon, the game's own shape.
            let (cx, cy) = (r.x + 34.0, r.y + r.h / 2.0);
            if slot.is_some() {
                // Your colour marks what is yours, as it does on the HUD.
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(mine.r, mine.g, mine.b, 0.1));
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, mine);
                // Dark body, bright outline: a filled bright hexagon would glow
                // and wash the number out.
                draw_poly(cx, cy, 6, 20.0, 0.0, Color::new(mine.r * 0.2, mine.g * 0.2, mine.b * 0.2, 1.0));
                draw_poly_lines(cx, cy, 6, 20.0, 0.0, 2.5, mine);
            } else {
                draw_poly_lines(cx, cy, 6, 20.0, 0.0, 1.5, TEXT_DIM);
            }
        }
        glow.present();

        // The crisp layer: everything that is read, on the finished picture.
        glow.ui();
        if cards.roomy() {
            center_wide("click a weapon or press its number (click a picked one to put it back). The order you pick is the order of keys 1-5.", 124.0, 22.0, TEXT_DIM);
        }
        for (i, &id) in ARSENAL.iter().enumerate() {
            let r = cards.rect(i);
            let slot = self.picked.iter().position(|&p| p == id);
            let (cx, cy) = (r.x + 34.0, r.y + r.h / 2.0);
            if let Some(n) = slot {
                draw_text(&format!("{}", n + 1), cx - 5.0, cy + 8.0, 26.0, WHITE);
            } else {
                draw_text(&format!("{}", (i + 1) % 10), cx - 4.0, cy + 6.0, 20.0, TEXT_DIM);
            }

            let def = &WEAPONS[id as usize];
            let (name, detail) = if slot.is_some() { (TEXT, TEXT_DIM) } else { (Color::new(0.7, 0.72, 0.78, 1.0), TEXT_DIM) };
            // Small cards keep only the name; the details need room.
            if r.h >= 64.0 {
                draw_text(def.name, r.x + 70.0, r.y + 31.0, 26.0, name);
                draw_text(blurb(id), r.x + 70.0, r.y + 56.0, 18.0, detail);
            } else {
                draw_text(def.name, r.x + 70.0, r.y + r.h / 2.0 + 9.0, 26.0, name);
            }
            if r.w >= 460.0 {
                let rounds = if def.mag == 1 { "round" } else { "rounds" };
                let magazine = format!("{} {rounds}  reload {:.1}s", def.mag, def.reload);
                let width = measure_text(&magazine, None, 16, 1.0).width;
                draw_text(&magazine, r.x + r.w - 12.0 - width, r.y + 31.0, 16.0, TEXT_DIM);
            }
        }

        // Footer: CLEAR and GO buttons, and on a roomy screen the keys under them.
        let width = view::layout().size.x;
        let (button_y, button_h) = if cards.roomy() { (height - 110.0, 44.0) } else { (height - 44.0, 38.0) };
        let (w_clear, w_go) = (140.0f32.min((width - 36.0) * 0.4), 220.0f32.min((width - 36.0) * 0.6));
        let left = (width - w_clear - 12.0 - w_go) / 2.0;
        let clear = button(Rect::new(left, button_y, w_clear, button_h), "CLEAR", !self.picked.is_empty(), TEXT);
        let missing = LOADOUT_SIZE - self.picked.len();
        let label = if ready { "GO".to_string() } else { format!("pick {missing} more") };
        let go = button(Rect::new(left + w_clear + 12.0, button_y, w_go, button_h), &label, ready, mine);
        if cards.roomy() {
            center_wide("Enter: go     Backspace: clear     Esc: back to the menu", height - 36.0, 20.0, TEXT_DIM);
        }
        (clear, go)
    }
}

/// Where the cards go on this screen. Two columns of five, filled column by
/// column (the left one holds the arsenal's first five weapons); one column of
/// ten only on a narrow, tall window. A landscape phone keeps two columns: its
/// cards get short (name only) but stay big enough to tap. Cards shrink to fit the
/// height, and a short screen trades the help lines for room.
struct Cards {
    cols: usize,
    left: f32,
    top: f32,
    w: f32,
    h: f32,
}

impl Cards {
    fn new() -> Cards {
        let size = view::layout().size;
        let cols = if size.x >= 600.0 { 2 } else { 1 };
        let rows = (ARSENAL.len() / cols) as f32;
        let (top, bottom) = if size.y >= 700.0 { (168.0, 130.0) } else { (60.0, 50.0) };
        let w = ((size.x - 60.0 - (cols - 1) as f32 * 40.0) / cols as f32).min(560.0);
        let h = ((size.y - top - bottom) / rows - CARD_GAP).min(76.0);
        let left = (size.x - cols as f32 * w - (cols - 1) as f32 * 40.0) / 2.0;
        Cards { cols, left, top, w, h }
    }

    /// Whether there is room for the help lines.
    fn roomy(&self) -> bool {
        self.top > 100.0
    }

    fn rect(&self, i: usize) -> Rect {
        let rows = ARSENAL.len() / self.cols;
        let (col, row) = (i / rows, i % rows);
        Rect::new(self.left + col as f32 * (self.w + 40.0), self.top + row as f32 * (self.h + CARD_GAP), self.w, self.h)
    }
}
