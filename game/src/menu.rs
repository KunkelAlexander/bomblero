// The start screen is a small level. You walk to the option you want and stand
// on its hexagon until the ring closes: play offline, host a room, join a room.
//
// Why not a list of text lines? Because this is a game about moving and
// shooting, and the first thing a new player does here is move and shoot. The
// menu doubles as the controls tutorial (the hints are on the floor), there is a
// shooting range to try the weapons on, and nothing about it needs a second set
// of rules: it is an ordinary match (one player, no bots, `MAP_HUB`) running on
// the same host, sim and presenter as a real game. "Inputs in, state out" pays
// off again: the menu is just another client reading a `Replica`.
//
// The old keys still work (P, H, J) for anyone who just wants to get going.

use macroquad::prelude::*;
use protocol::relay::{CODE_ALPHABET, CODE_LEN};
use sim::Input;

use crate::controls::Controls;
use crate::draw::{button, center_text, center_wide, player_color, DANGER};
use crate::lights::Light;
use crate::view::{self, Glow};
use crate::HostGame;

pub enum Choice {
    Offline,
    Host,
    Join(String),
}

/// A place on the floor that starts something.
struct Pad {
    pos: Vec2,
    label: &'static str,
    hint: &'static str,
}

/// One per alcove of `MAP_HUB`, in the middle of the alcove.
const PADS: [Pad; 3] = [
    Pad { pos: vec2(240.0, 544.0), label: "PLAY", hint: "against 3 bots" },
    Pad { pos: vec2(640.0, 544.0), label: "HOST", hint: "open a room for friends" },
    Pad { pos: vec2(1040.0, 544.0), label: "JOIN", hint: "enter a room code" },
];

const PAD_RADIUS: f32 = 40.0;
/// How close the player's centre must be to count as standing on a pad.
const STAND_RADIUS: f32 = 32.0;
/// Seconds you must stay on a pad. Long enough that walking past never starts
/// anything, short enough that nobody waits.
const DWELL: f32 = 0.8;

const TEXT: Color = Color::new(0.85, 0.87, 0.92, 1.0);
const TEXT_DIM: Color = Color::new(0.45, 0.47, 0.52, 1.0);
const NEON: Color = Color::new(0.1, 1.0, 0.95, 1.0);

pub struct Menu {
    hub: HostGame,
    /// The pad the player stands on, and for how long.
    on: Option<usize>,
    dwell: f32,
    /// A pad that must be left before it works again (so backing out of the
    /// code entry does not immediately re-open it).
    ignore: Option<usize>,
    /// `Some` while the player is typing a room code.
    typing: Option<String>,
}

impl Menu {
    pub fn new() -> Menu {
        // Throw away letters typed on the previous screen so they do not
        // leak into the code field.
        while get_char_pressed().is_some() {}
        Menu { hub: HostGame::menu(), on: None, dwell: 0.0, ignore: None, typing: None }
    }

    /// One frame: run the hub, see whether a choice was made, draw.
    pub fn frame(&mut self, controls: &mut Controls, glow: &Glow) -> Option<Choice> {
        let me = self.hub.game.replica.me as usize;
        // While a code is being typed the letters must not also walk the player.
        let input = if self.typing.is_some() {
            Input { aim: self.hub.game.replica.world.players[me].aim, ..Input::default() }
        } else {
            controls.read(&self.hub.game.replica.world, me)
        };
        self.hub.advance(input);

        let choice = if self.typing.is_some() { self.type_code() } else { self.walk(me) };
        // The on-screen keypad reports its key while drawing (see `button`).
        match self.draw(me, glow) {
            Some(key) => choice.or_else(|| self.press(key)),
            None => choice,
        }
    }

    fn walk(&mut self, me: usize) -> Option<Choice> {
        if is_key_pressed(KeyCode::P) {
            return Some(Choice::Offline);
        }
        if is_key_pressed(KeyCode::H) {
            return Some(Choice::Host);
        }
        if is_key_pressed(KeyCode::J) {
            return self.pick(2);
        }

        let player = &self.hub.game.replica.world.players[me];
        let mut on = if player.alive { PADS.iter().position(|p| p.pos.distance(player.pos) < STAND_RADIUS) } else { None };
        if on != self.ignore {
            self.ignore = None;
        }
        if on.is_some() && on == self.ignore {
            on = None;
        }
        if on != self.on {
            self.on = on;
            self.dwell = 0.0;
        }
        if let Some(pad) = on {
            self.dwell += get_frame_time();
            if self.dwell >= DWELL {
                return self.pick(pad);
            }
        }
        None
    }

    fn pick(&mut self, pad: usize) -> Option<Choice> {
        match pad {
            0 => Some(Choice::Offline),
            1 => Some(Choice::Host),
            _ => {
                while get_char_pressed().is_some() {} // the "j" itself
                self.typing = Some(String::new());
                self.ignore = Some(pad);
                self.dwell = 0.0;
                None
            }
        }
    }

    /// The physical keyboard, turned into the same keys the keypad sends.
    fn type_code(&mut self) -> Option<Choice> {
        let mut choice = None;
        while let Some(c) = get_char_pressed() {
            if c.is_ascii_alphanumeric() {
                choice = choice.or(self.press(Key::Char(c.to_ascii_uppercase())));
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.press(Key::Delete);
        }
        if is_key_pressed(KeyCode::Escape) {
            self.press(Key::Back);
        } else if is_key_pressed(KeyCode::Enter) {
            choice = choice.or(self.press(Key::Join));
        }
        choice
    }

    fn press(&mut self, key: Key) -> Option<Choice> {
        let code = self.typing.as_mut()?;
        match key {
            Key::Char(c) if code.len() < CODE_LEN => code.push(c),
            Key::Char(_) => {}
            Key::Delete => {
                code.pop();
            }
            Key::Back => self.typing = None,
            Key::Join if code.len() == CODE_LEN => return Some(Choice::Join(code.clone())),
            Key::Join => {}
        }
        None
    }

    fn draw(&mut self, me: usize, glow: &Glow) -> Option<Key> {
        // The pads light their alcoves a little, so you can see where to go.
        let lights: Vec<Light> = PADS.iter().map(|p| Light { pos: p.pos, radius: 120.0, color: Color::new(0.7, 0.85, 1.0, 1.0), strength: 0.45 }).collect();

        let hub = &mut self.hub;
        hub.presenter.floor(&mut hub.game.replica, &lights, glow);
        // Signs painted on the floor, under the player and their shots.
        draw_title();
        if crate::touch::active() {
            center_text("left thumb: move    right thumb: aim and fire    tap a card: weapon", 336.0, 26.0, TEXT_DIM);
        } else {
            center_text("WASD move    mouse aim    click fire    1-0 weapons", 336.0, 26.0, TEXT_DIM);
        }
        center_text("walk onto a hexagon and stay to choose", 366.0, 26.0, TEXT_DIM);
        for (i, pad) in PADS.iter().enumerate() {
            let progress = if self.on == Some(i) { self.dwell / DWELL } else { 0.0 };
            draw_pad(pad, progress, player_color(me));
        }
        hub.presenter.actors(&hub.game.replica.world);

        let mut key = None;
        if let Some(code) = &self.typing {
            glow.overlay();
            key = draw_code_entry(code);
        }
        glow.present();
        key
    }
}

// ---- drawing ----------------------------------------------------------------

fn draw_pad(pad: &Pad, progress: f32, mine: Color) {
    let c = pad.pos;
    // It breathes while nobody stands on it, to say "this is something".
    let breath = 0.5 + 0.2 * (get_time() as f32 * 2.0).sin();
    let idle = Color::new(0.7, 0.75, 0.85, breath * (1.0 - progress));
    draw_poly_lines(c.x, c.y, 6, PAD_RADIUS, 0.0, 2.0, idle);
    draw_poly_lines(c.x, c.y, 6, PAD_RADIUS * 0.55, 0.0, 1.5, Color::new(0.7, 0.75, 0.85, 0.3));
    if progress > 0.0 {
        // The ring closes edge by edge in your colour, and the middle fills.
        draw_poly(c.x, c.y, 6, PAD_RADIUS * 0.55 * progress, 0.0, Color::new(mine.r, mine.g, mine.b, 0.5));
        let step = std::f32::consts::TAU / 6.0;
        for k in 0..6 {
            let (a, b) = (k as f32 * step, (k + 1) as f32 * step);
            let (from, to) = (c + vec2(a.cos(), a.sin()) * PAD_RADIUS, c + vec2(b.cos(), b.sin()) * PAD_RADIUS);
            let done = (progress * 6.0 - k as f32).clamp(0.0, 1.0);
            if done > 0.0 {
                let end = from + (to - from) * done;
                draw_line(from.x, from.y, end.x, end.y, 4.0, mine);
            }
        }
    }
    center_at(pad.label, c.x, c.y - 66.0, 26.0, TEXT);
    center_at(pad.hint, c.x, c.y + 72.0, 14.0, TEXT_DIM);
}

fn center_at(text: &str, x: f32, y: f32, size: f32, color: Color) {
    let width = measure_text(text, None, size as u16, 1.0).width;
    draw_text(text, x - width / 2.0, y, size, color);
}

/// A key of the code entry, from the keyboard or the on-screen keypad.
#[derive(Clone, Copy)]
enum Key {
    Char(char),
    Delete,
    Back,
    Join,
}

/// The code entry covers the arena: the code, then a keypad with every letter a
/// room code can contain (so a phone needs no keyboard). Drawn in picture
/// coordinates (after `Glow::overlay`). Returns the key tapped this frame.
fn draw_code_entry(code: &str) -> Option<Key> {
    let size = view::layout().size;
    draw_rectangle(0.0, 0.0, size.x, size.y, Color::new(0.03, 0.03, 0.04, 0.95));
    center_wide("type or tap the room code", 34.0, 26.0, TEXT_DIM);
    center_wide(&format!("{:_<width$}", code, width = CODE_LEN), 96.0, 64.0, TEXT);

    // 8 columns x 4 rows of letters, then a row of BACK / DEL / JOIN. Keys are
    // as big as fit, up to 52 px.
    const COLS: usize = 8;
    let key = ((size.x - 24.0) / COLS as f32 - 8.0).min((size.y - 130.0) / 5.0 - 8.0).min(52.0);
    let step = key + 8.0;
    let left = (size.x - (COLS as f32 * step - 8.0)) / 2.0;
    let top = 120.0;
    let mut pressed = None;
    for (i, c) in CODE_ALPHABET.chars().enumerate() {
        let r = Rect::new(left + (i % COLS) as f32 * step, top + (i / COLS) as f32 * step, key, key);
        if button(r, &c.to_string(), code.len() < CODE_LEN, TEXT) {
            pressed = Some(Key::Char(c));
        }
    }
    let rows = CODE_ALPHABET.len().div_ceil(COLS) as f32;
    let y = top + rows * step;
    let third = (COLS as f32 * step - 8.0 - 16.0) / 3.0;
    let actions = [("BACK", Key::Back, true), ("DEL", Key::Delete, !code.is_empty()), ("JOIN", Key::Join, code.len() == CODE_LEN)];
    for (i, (label, k, enabled)) in actions.into_iter().enumerate() {
        let r = Rect::new(left + i as f32 * (third + 8.0), y, third, key);
        if button(r, label, enabled, if i == 2 { NEON } else { TEXT }) {
            pressed = Some(k);
        }
    }
    pressed
}

// The title is drawn with strokes, not text: the built-in font is a small pixel
// font that turns to mush when it is enlarged, and strokes stay sharp and take
// the glow well. Each letter is a few polylines on a grid 2 wide and 4 tall; the
// O is a hexagon, the game's signature shape.
fn letter(c: char) -> &'static [&'static [(f32, f32)]] {
    match c {
        'N' => &[&[(0.0, 4.0), (0.0, 0.0), (2.0, 4.0), (2.0, 0.0)]],
        'E' => &[&[(2.0, 0.0), (0.0, 0.0), (0.0, 4.0), (2.0, 4.0)], &[(0.0, 2.0), (1.5, 2.0)]],
        'O' => &[&[(0.5, 0.0), (1.5, 0.0), (2.0, 2.0), (1.5, 4.0), (0.5, 4.0), (0.0, 2.0), (0.5, 0.0)]],
        'B' => &[
            &[(0.0, 0.0), (0.0, 4.0)],
            &[(0.0, 0.0), (1.5, 0.0), (2.0, 0.7), (2.0, 1.3), (1.5, 2.0), (0.0, 2.0)],
            &[(1.5, 2.0), (2.0, 2.7), (2.0, 3.3), (1.5, 4.0), (0.0, 4.0)],
        ],
        'M' => &[&[(0.0, 4.0), (0.0, 0.0), (1.0, 2.0), (2.0, 0.0), (2.0, 4.0)]],
        'L' => &[&[(0.0, 0.0), (0.0, 4.0), (2.0, 4.0)]],
        'R' => &[&[(0.0, 4.0), (0.0, 0.0), (1.5, 0.0), (2.0, 0.7), (2.0, 1.3), (1.5, 2.0), (0.0, 2.0)], &[(1.0, 2.0), (2.0, 4.0)]],
        _ => &[],
    }
}

fn draw_title() {
    let unit = 26.0;
    let text = "BOMBLERO";
    let advance = |c: char| if c == ' ' { 2.0 } else { 3.0 };
    let width: f32 = (text.chars().map(advance).sum::<f32>() - 1.0) * unit;
    let mut x = (1280.0 - width) / 2.0;
    for (i, c) in text.chars().enumerate() {
        // "BOMB" is danger-orange, like everything that explodes.
        let color = if i < 4 { DANGER } else { NEON };
        for stroke in letter(c) {
            for pair in stroke.windows(2) {
                let (a, b) = (vec2(x + pair[0].0 * unit, 56.0 + pair[0].1 * unit), vec2(x + pair[1].0 * unit, 56.0 + pair[1].1 * unit));
                draw_line(a.x, a.y, b.x, b.y, 6.0, color);
                draw_line(a.x, a.y, b.x, b.y, 1.8, WHITE);
            }
            // Round the corners: a dot at each joint closes the gaps.
            for p in stroke.iter() {
                let at = vec2(x + p.0 * unit, 56.0 + p.1 * unit);
                draw_circle(at.x, at.y, 3.0, color);
            }
        }
        x += advance(c) * unit;
    }
}
