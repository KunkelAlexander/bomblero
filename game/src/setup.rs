// The match setup: which map, how many bots, how many kills win. It comes
// right after PLAY or HOST, before the weapon picker: first the match, then
// what you carry into it.
//
// Joining skips this screen. Networking concept: authority. The host runs the
// match, so the host's choice IS the match, and a joiner learns it from the
// `Welcome` (`protocol::Rules`). Two players choosing different maps for the
// same match cannot even be expressed.
//
// Like the armory, this screen only collects a choice. It touches no world and
// no network.

use macroquad::prelude::*;
use protocol::Rules;
use sim::tilemap::{MapDef, MAPS};
use sim::{MAP_H, MAP_W};

use crate::draw::{button, center_wide, player_color};
use crate::menu::Choice;
use crate::view::{self, screen_to_picture, Glow};

const TEXT: Color = Color::new(0.92, 0.93, 0.96, 1.0);
// Secondary text, at the armory's readable contrast.
const TEXT_DIM: Color = Color::new(0.6, 0.62, 0.67, 1.0);
const CARD_BG: Color = Color::new(0.075, 0.08, 0.095, 1.0);

/// Opponents in an offline match. At most 3: a match has four player
/// colours, and the maps are made for four.
const BOTS: [usize; 3] = [1, 2, 3];
const SCORES: [i32; 4] = [5, 10, 15, 25];

/// Everything this screen decides. `rules` go to the host (and from there to
/// every joiner); `bots` only matter offline, because a room for friends always
/// has four seats, and bots keep the empty ones warm until friends arrive.
#[derive(Clone, Copy)]
pub struct Settings {
    pub rules: Rules,
    pub bots: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { rules: Rules::default(), bots: 3 }
    }
}

pub struct Setup {
    /// Where to go once the choice is made. `Some` until it is handed back.
    choice: Option<Choice>,
    settings: Settings,
}

impl Setup {
    /// `start` is the previous choice, so a player who is happy with it just
    /// presses NEXT.
    pub fn new(choice: Choice, start: Settings) -> Setup {
        Setup { choice: Some(choice), settings: start }
    }

    /// One frame. Returns the choice once NEXT is pressed.
    pub fn frame(&mut self, glow: &Glow) -> Option<(Choice, Settings)> {
        let spots = Spots::new();
        let mouse = screen_to_picture(Vec2::from(mouse_position()));
        let rules = &mut self.settings.rules;
        let count = MAPS.len() as u8;
        if is_key_pressed(KeyCode::Left) {
            rules.map = (rules.map + count - 1) % count;
        }
        if is_key_pressed(KeyCode::Right) {
            rules.map = (rules.map + 1) % count;
        }
        for i in 0..MAPS.len() {
            if crate::touch::clicked_in(spots.card(i)) {
                rules.map = i as u8;
            }
        }

        let next = self.draw(&spots, mouse, glow);
        if is_key_pressed(KeyCode::Enter) || next {
            return Some((self.choice.take()?, self.settings));
        }
        None
    }

    /// Draws the screen and handles its buttons. Returns whether NEXT was pressed.
    fn draw(&mut self, spots: &Spots, mouse: Vec2, glow: &Glow) -> bool {
        let mine = player_color(0);
        let chosen = self.settings.rules.map as usize;

        // The neon layer: the heading and the frame of the chosen map glow.
        glow.begin();
        glow.overlay();
        let sign = Color::new(0.55 + 0.45 * mine.r, 0.55 + 0.45 * mine.g, 0.55 + 0.45 * mine.b, 1.0);
        center_wide("MATCH SETUP", if spots.roomy { 84.0 } else { 44.0 }, 44.0, sign);
        for i in 0..MAPS.len() {
            let r = spots.card(i);
            let hover = r.contains(mouse);
            draw_rectangle(r.x, r.y, r.w, r.h, if hover { Color::new(0.11, 0.12, 0.145, 1.0) } else { CARD_BG });
            if i == chosen {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(mine.r, mine.g, mine.b, 0.1));
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, mine);
            }
        }
        glow.present();

        // The crisp layer: map previews, text and buttons.
        glow.ui();
        if spots.roomy {
            center_wide("click to choose     Left/Right: map     Enter: next     Esc: back to the menu", 124.0, 20.0, TEXT_DIM);
        }
        for (i, def) in MAPS.iter().enumerate() {
            let r = spots.card(i);
            let preview = spots.preview(i);
            draw_preview(def, preview);
            let name = if i == chosen { TEXT } else { Color::new(0.7, 0.72, 0.78, 1.0) };
            draw_text(def.name, r.x + 12.0, preview.bottom() + 28.0, 26.0, name);
            // The blurb only where it fits: a phone's cards keep just the name.
            if spots.roomy && measure_text(def.blurb, None, 18, 1.0).width < r.w - 24.0 {
                draw_text(def.blurb, r.x + 12.0, preview.bottom() + 52.0, 18.0, TEXT_DIM);
            }
        }

        // BOTS: offline only (see `Settings`).
        let s = &mut self.settings;
        row_label("BOTS", spots.bots_y, spots);
        if matches!(self.choice, Some(Choice::Host)) {
            draw_text("fill the seats until friends join", spots.options_x, spots.bots_y + spots.row_h / 2.0 + 7.0, 20.0, TEXT_DIM);
        } else {
            for (i, &n) in BOTS.iter().enumerate() {
                if option(spots.option(i, spots.bots_y), &n.to_string(), s.bots == n, mine) {
                    s.bots = n;
                }
            }
        }
        row_label("FIRST TO", spots.score_y, spots);
        for (i, &n) in SCORES.iter().enumerate() {
            if option(spots.option(i, spots.score_y), &n.to_string(), s.rules.score_limit == n, mine) {
                s.rules.score_limit = n;
            }
        }

        let width = view::layout().size.x;
        let w_next = 220.0f32.min(width - 36.0);
        button(Rect::new((width - w_next) / 2.0, spots.next_y, w_next, spots.row_h), "NEXT", true, mine)
    }
}

/// A map in miniature, straight from its ASCII art: rock solid, earth as
/// smaller blocks. Enough to tell the maps apart at a glance.
fn draw_preview(def: &MapDef, r: Rect) {
    let tile = r.w / MAP_W as f32;
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.03, 0.03, 0.04, 1.0));
    for (y, row) in def.art.lines().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let (px, py) = (r.x + x as f32 * tile, r.y + y as f32 * tile);
            match ch {
                '#' => draw_rectangle(px, py, tile, tile, Color::new(0.38, 0.4, 0.47, 1.0)),
                'b' => draw_rectangle(px + tile * 0.15, py + tile * 0.15, tile * 0.7, tile * 0.7, Color::new(0.6, 0.5, 0.36, 1.0)),
                _ => {}
            }
        }
    }
}

fn row_label(text: &str, y: f32, spots: &Spots) {
    let width = measure_text(text, None, 20, 1.0).width;
    draw_text(text, spots.options_x - 20.0 - width, y + spots.row_h / 2.0 + 7.0, 20.0, TEXT_DIM);
}

/// One choice of a row: a button, tinted in your colour when it is the
/// current one. Returns whether it was clicked.
fn option(r: Rect, label: &str, on: bool, mine: Color) -> bool {
    let clicked = button(r, label, true, if on { mine } else { TEXT_DIM });
    if on {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(mine.r, mine.g, mine.b, 0.12));
    }
    clicked
}

/// Where everything goes on this screen, from the bottom up: the NEXT button,
/// the two rows of options, and the map cards in whatever height is left. A
/// short screen (a landscape phone) drops the help lines and the blurbs.
struct Spots {
    roomy: bool,
    row_h: f32,
    next_y: f32,
    score_y: f32,
    bots_y: f32,
    /// Where the option buttons of both rows start (their labels go left of it).
    options_x: f32,
    cards_left: f32,
    cards_top: f32,
    card_w: f32,
    preview_h: f32,
}

const CARD_GAP: f32 = 16.0;
const OPTION_W: f32 = 64.0;

impl Spots {
    fn new() -> Spots {
        let size = view::layout().size;
        let roomy = size.y >= 700.0;
        let row_h = if roomy { 44.0 } else { 38.0 };
        let next_y = if roomy { size.y - 110.0 } else { size.y - 44.0 };
        let score_y = next_y - 16.0 - row_h;
        let bots_y = score_y - 10.0 - row_h;
        // The wider row (four scores) and its label, centred.
        let options_x = (size.x - (4.0 * OPTION_W + 3.0 * 10.0)) / 2.0 + 60.0;

        let cards_top = if roomy { 150.0 } else { 60.0 };
        let text_h = if roomy { 60.0 } else { 36.0 };
        let n = MAPS.len() as f32;
        let card_w = ((size.x - 48.0 - (n - 1.0) * CARD_GAP) / n).min(360.0);
        // The preview keeps the arena's shape: as wide as the card allows, unless
        // the height runs out first.
        let aspect = MAP_H as f32 / MAP_W as f32;
        let preview_h = ((card_w - 16.0) * aspect).min(bots_y - 16.0 - cards_top - text_h - 8.0).max(0.0);
        let cards_left = (size.x - n * card_w - (n - 1.0) * CARD_GAP) / 2.0;
        // On a tall screen the cards end well above the rows: move the rows up
        // under them, so the options are not stranded at the bottom.
        let bots_y = bots_y.min(cards_top + 8.0 + preview_h + text_h + 48.0);
        let score_y = bots_y + row_h + 10.0;
        Spots { roomy, row_h, next_y, score_y, bots_y, options_x, cards_left, cards_top, card_w, preview_h }
    }

    fn card(&self, i: usize) -> Rect {
        let text_h = if self.roomy { 60.0 } else { 36.0 };
        Rect::new(self.cards_left + i as f32 * (self.card_w + CARD_GAP), self.cards_top, self.card_w, 8.0 + self.preview_h + text_h)
    }

    fn preview(&self, i: usize) -> Rect {
        let card = self.card(i);
        let w = self.preview_h * MAP_W as f32 / MAP_H as f32;
        Rect::new(card.x + (card.w - w) / 2.0, card.y + 8.0, w, self.preview_h)
    }

    fn option(&self, i: usize, y: f32) -> Rect {
        Rect::new(self.options_x + i as f32 * (OPTION_W + 10.0), y, OPTION_W, self.row_h)
    }
}
