// The weapon picker: choose which of the weapons you carry into the match, like
// Liero's weapon menu. It sits between the menu and the match, so it is reached
// from every way of playing (offline, hosting, joining).
//
// Why a loadout at all: with seventeen weapons a fixed set would be a list of
// everything, and the choice is half the game. Picking five means a player can
// be the one with the Nova and the Railgun, and the others have to play around it.
// The weapons stand in columns by kind and you may take two of each kind at most,
// so nobody carries only guns, or the strongest weapon of every kind. And for
// anyone who would rather not choose (and so not choose "the best"): RANDOM.
//
// This screen only collects a choice. It touches no world and no network: the
// choice is handed to the match, which sends it to the host as a message, and
// the host decides whether it is acceptable (see `Host::handle`).

use macroquad::prelude::*;
use sim::rng::Rng;
use sim::weapons::{
    can_add, class_of, random_loadout, ARSENAL, BLACKHOLE, BLASTER, BOMB, BOUNCER, BUILD, CLASSES, CLUSTER, CONFUSER, FLASH,
    LASER, LOADOUT_SIZE, MAX_PER_CLASS, MIRROR, NOVA, OIL, RAILGUN, ROCKET, SHEEP, SWARM, TELEPORT, WEAPONS,
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

/// One line about each weapon, for the card. Client-only text, so it lives here
/// and not in the sim's weapon table (which only holds what the rules need).
/// Keep them short: four columns of cards leave about 30 characters.
fn blurb(id: u8) -> &'static str {
    match id {
        BLASTER => "fast bolts, a big magazine",
        BUILD => "puts up a brick: cover",
        ROCKET => "explodes and digs a crater",
        BOUNCER => "ricochets off walls",
        BOMB => "short fuse at your feet; a mine",
        RAILGUN => "instant beam; breaks a brick",
        CLUSTER => "bursts into seven bomblets",
        SWARM => "six missiles curl in on you",
        TELEPORT => "jump to where it lands",
        NOVA => "one huge blast; slow reload",
        SHEEP => "an enemy becomes a sheep",
        CONFUSER => "mirrors an enemy's screen",
        MIRROR => "sends enemy shots back",
        OIL => "no steering; walls hurt",
        LASER => "hold: a beam that bounces",
        BLACKHOLE => "pulls everyone in and holds",
        FLASH => "a cone that stuns a moment",
        _ => "",
    }
}

/// Draw `text`, shrunk if needed so it is at most `max_w` wide.
fn fit_text(text: &str, x: f32, y: f32, max_w: f32, size: f32, color: Color) {
    let width = measure_text(text, None, size as u16, 1.0).width;
    let size = if width > max_w { size * max_w / width } else { size };
    draw_text(text, x, y, size, color);
}

pub struct Armory {
    /// Where to go once the choice is made. `Some` until it is handed back.
    choice: Option<Choice>,
    /// The picked weapon ids, in the order they were picked: that order is the
    /// order of keys 1-5 in the match.
    picked: Vec<u8>,
    /// For RANDOM. Seeded from the clock, so every visit rolls different sets.
    rng: Rng,
}

impl Armory {
    /// `start` is the previous choice (or the default), so a player who is happy
    /// with it just presses Enter.
    pub fn new(choice: Choice, start: Vec<u8>) -> Armory {
        Armory { choice: Some(choice), picked: start, rng: Rng::new((macroquad::miniquad::date::now() * 1000.0) as u32) }
    }

    /// One frame. Returns what to play, and with which weapons, once chosen.
    pub fn frame(&mut self, glow: &Glow) -> Option<(Choice, Vec<u8>)> {
        let mouse = screen_to_picture(Vec2::from(mouse_position()));
        let cards = Cards::new();
        for (i, &id) in ARSENAL.iter().enumerate() {
            // Only the first ten have a key (1-9, 0); the rest are clicked or tapped.
            let key = KEYS.get(i).is_some_and(|&k| is_key_pressed(k));
            if key || crate::touch::clicked_in(cards.rect(i)) {
                self.toggle(id);
            }
        }
        let ready = self.picked.len() == LOADOUT_SIZE;
        // The buttons stand in for the keys on a touchscreen.
        let (clear, random, go) = self.draw(&cards, mouse, ready, glow);
        if is_key_pressed(KeyCode::Backspace) || clear {
            self.picked.clear();
        }
        if is_key_pressed(KeyCode::R) || random {
            self.picked = random_loadout(&mut self.rng);
        }

        if ready && (is_key_pressed(KeyCode::Enter) || go) {
            return Some((self.choice.take()?, self.picked.clone()));
        }
        None
    }

    /// Pick a weapon, or put it back. A full set refuses a sixth, and so does a
    /// kind that already has its two (`can_add` is the host's rule too).
    fn toggle(&mut self, id: u8) {
        if let Some(at) = self.picked.iter().position(|&p| p == id) {
            self.picked.remove(at);
        } else if can_add(&self.picked, id) {
            self.picked.push(id);
        }
    }

    /// Returns whether the CLEAR, RANDOM and GO buttons were pressed.
    fn draw(&self, cards: &Cards, mouse: Vec2, ready: bool, glow: &Glow) -> (bool, bool, bool) {
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
            let (cx, cy) = (r.x + cards.hex_x(), r.y + r.h / 2.0);
            let hex = cards.hex_radius();
            if slot.is_some() {
                // Your colour marks what is yours, as it does on the HUD.
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(mine.r, mine.g, mine.b, 0.1));
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, mine);
                // Dark body, bright outline: a filled bright hexagon would glow
                // and wash the number out.
                draw_poly(cx, cy, 6, hex, 0.0, Color::new(mine.r * 0.2, mine.g * 0.2, mine.b * 0.2, 1.0));
                draw_poly_lines(cx, cy, 6, hex, 0.0, 2.5, mine);
            } else {
                draw_poly_lines(cx, cy, 6, hex, 0.0, 1.5, TEXT_DIM);
            }
        }
        glow.present();

        // The crisp layer: everything that is read, on the finished picture.
        glow.ui();
        if cards.roomy() {
            center_wide("click a weapon or press its number (again to put it back). Two of each kind at most. The order you pick is the order of keys 1-5.", 124.0, 22.0, TEXT_DIM);
        }
        // The kind of each column, and how many of its two you have taken.
        if cards.by_class {
            for &class in &CLASSES {
                let first = ARSENAL.iter().position(|&id| class_of(id) == class).unwrap_or(0);
                let r = cards.rect(first);
                let taken = self.picked.iter().filter(|&&id| class_of(id) == class).count();
                let color = if taken == MAX_PER_CLASS { mine } else { TEXT_DIM };
                draw_text(&format!("{}  {}/{}", class.name(), taken, MAX_PER_CLASS), r.x, r.y - 8.0, 20.0, color);
            }
        }
        for (i, &id) in ARSENAL.iter().enumerate() {
            let r = cards.rect(i);
            let slot = self.picked.iter().position(|&p| p == id);
            let (cx, cy) = (r.x + cards.hex_x(), r.y + r.h / 2.0);
            if let Some(n) = slot {
                draw_text(&format!("{}", n + 1), cx - 5.0, cy + 8.0, 26.0, WHITE);
            } else if i < KEYS.len() {
                draw_text(&format!("{}", (i + 1) % 10), cx - 4.0, cy + 6.0, 20.0, TEXT_DIM);
            }

            let def = &WEAPONS[id as usize];
            // Out of reach: its kind already has two picks. A card that cannot
            // be chosen should look it.
            let closed = slot.is_none() && self.picked.len() < LOADOUT_SIZE && !can_add(&self.picked, id);
            let (name, detail) = if slot.is_some() {
                (TEXT, TEXT_DIM)
            } else if closed {
                (Color::new(0.4, 0.41, 0.45, 1.0), Color::new(0.35, 0.36, 0.4, 1.0))
            } else {
                (Color::new(0.7, 0.72, 0.78, 1.0), TEXT_DIM)
            };
            // Small cards keep only the name; the details need room.
            let (text_x, size) = (r.x + cards.text_x(), cards.name_size());
            let room = r.w - cards.text_x() - 8.0;
            if r.h >= 64.0 {
                fit_text(def.name, text_x, r.y + 31.0, room, size, name);
                fit_text(blurb(id), text_x, r.y + 56.0, room, 18.0, detail);
            } else {
                fit_text(def.name, text_x, r.y + r.h / 2.0 + 9.0, room, size, name);
            }
            if r.w >= 460.0 {
                let rounds = if def.mag == 1 { "round" } else { "rounds" };
                let magazine = format!("{} {rounds}  reload {:.1}s", def.mag, def.reload);
                let width = measure_text(&magazine, None, 16, 1.0).width;
                draw_text(&magazine, r.x + r.w - 12.0 - width, r.y + 31.0, 16.0, TEXT_DIM);
            }
        }

        // Footer: CLEAR, RANDOM and GO buttons, and on a roomy screen the keys under them.
        let width = view::layout().size.x;
        let (button_y, button_h) = if cards.roomy() { (height - 110.0, 44.0) } else { (height - 44.0, 38.0) };
        let usable = width - 48.0;
        let (w_clear, w_random, w_go) = (120.0f32.min(usable * 0.25), 160.0f32.min(usable * 0.3), 220.0f32.min(usable * 0.4));
        let left = (width - w_clear - w_random - w_go - 24.0) / 2.0;
        let clear = button(Rect::new(left, button_y, w_clear, button_h), "CLEAR", !self.picked.is_empty(), TEXT);
        let random = button(Rect::new(left + w_clear + 12.0, button_y, w_random, button_h), "RANDOM", true, TEXT);
        let missing = LOADOUT_SIZE - self.picked.len();
        let label = if ready { "GO".to_string() } else { format!("pick {missing} more") };
        let go = button(Rect::new(left + w_clear + w_random + 24.0, button_y, w_go, button_h), &label, ready, mine);
        if cards.roomy() {
            center_wide("Enter: go     R: random     Backspace: clear     Esc: back to the menu", height - 36.0, 20.0, TEXT_DIM);
        }
        (clear, random, go)
    }
}

/// Where the cards go on this screen. One column per kind of weapon (the arsenal
/// is listed kind by kind), five rows at most. Cards shrink to fit the height (a
/// phone's cards get short and show only the name, but stay big enough to tap),
/// narrow cards use smaller type, and a short screen trades the help lines for
/// room. A very narrow window cannot fit four columns: it gets two, filled
/// column by column in arsenal order, without the headings.
struct Cards {
    /// Is a column a kind of weapon (so it gets a heading)?
    by_class: bool,
    rows: usize,
    left: f32,
    top: f32,
    w: f32,
    h: f32,
    /// Space between cards: down a column, and between columns.
    gap: f32,
    col_gap: f32,
}

impl Cards {
    fn new() -> Cards {
        let size = view::layout().size;
        let by_class = size.x >= 560.0;
        let cols = if by_class { CLASSES.len() } else { 2 };
        let rows = if by_class {
            CLASSES.iter().map(|&c| ARSENAL.iter().filter(|&&id| class_of(id) == c).count()).max().unwrap_or(1)
        } else {
            ARSENAL.len().div_ceil(cols)
        };
        // Rows of cards need every pixel on a short screen: tighter gaps, and
        // on a roomy one a shorter heading. Four columns leave room above the
        // cards for their headings.
        let (top, bottom, gap) = match (size.y >= 700.0, by_class) {
            (true, _) => (170.0, 130.0, 10.0),
            (false, true) => (80.0, 50.0, 6.0),
            (false, false) => (60.0, 50.0, 6.0),
        };
        let col_gap = if size.x < 900.0 { 16.0 } else { 30.0 };
        let w = ((size.x - 60.0 - (cols - 1) as f32 * col_gap) / cols as f32).min(560.0);
        let h = ((size.y - top - bottom) / rows as f32 - gap).min(76.0);
        let left = (size.x - cols as f32 * w - (cols - 1) as f32 * col_gap) / 2.0;
        Cards { by_class, rows, left, top, w, h, gap, col_gap }
    }

    /// Whether there is room for the help lines.
    fn roomy(&self) -> bool {
        self.top > 100.0
    }

    /// A narrow card (a phone's, or one of four) squeezes its hexagon and name.
    fn narrow(&self) -> bool {
        self.w < 300.0
    }

    fn hex_x(&self) -> f32 {
        if self.narrow() { 24.0 } else { 34.0 }
    }

    fn hex_radius(&self) -> f32 {
        if self.narrow() { 14.0 } else { 20.0 }
    }

    fn text_x(&self) -> f32 {
        if self.narrow() { 46.0 } else { 70.0 }
    }

    fn name_size(&self) -> f32 {
        if self.narrow() { 20.0 } else { 26.0 }
    }

    /// Card `i` of the arsenal: its column is its kind (or, in a narrow window,
    /// the next in line), its row is its place within it.
    fn rect(&self, i: usize) -> Rect {
        let (col, row) = if self.by_class {
            let class = class_of(ARSENAL[i]);
            let col = CLASSES.iter().position(|&c| c == class).unwrap_or(0);
            (col, ARSENAL[..i].iter().filter(|&&id| class_of(id) == class).count())
        } else {
            (i / self.rows, i % self.rows)
        };
        Rect::new(self.left + col as f32 * (self.w + self.col_gap), self.top + row as f32 * (self.h + self.gap), self.w, self.h)
    }
}
