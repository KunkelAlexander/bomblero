// The heads-up display: one quiet strip under the map.
//
// Design rules:
//   * It lives in its own strip, never over the playfield, on an opaque dark
//     background, so nothing in it competes with what is happening in the arena.
//   * The glow shader is switched off below `view::HUD_TOP` (see shaders.rs):
//     glowing text is hard to read. Colour is kept for *ownership*: your colour
//     outlines your weapon card and your score chip, and each player's hexagon
//     on the scoreboard is their in-game colour.
//   * Hierarchy: the selected weapon is bright and carries the ammo bar;
//     everything else is dim.
//
// Read-only like all drawing: it takes `&World` and never changes it.

use macroquad::prelude::*;
use sim::world::{RESPAWN_TIME, SCORE_LIMIT};
use sim::{World, WEAPONS};

use crate::draw::{center_text, player_color};
use crate::view::HUD_TOP;

const BAR_BG: Color = Color::new(0.045, 0.05, 0.06, 1.0);
const CARD_BG: Color = Color::new(0.075, 0.08, 0.095, 1.0);
const CARD_BG_SELECTED: Color = Color::new(0.12, 0.13, 0.155, 1.0);
const TEXT: Color = Color::new(0.92, 0.93, 0.96, 1.0);
const TEXT_DIM: Color = Color::new(0.45, 0.47, 0.52, 1.0);
const BAR_Y: f32 = HUD_TOP + 4.0;
const BAR_H: f32 = 44.0;

/// `down`/`up` are bytes per second on the wire; `banner` is a line of status
/// (room code, ping) from whoever is playing.
pub fn draw(world: &World, me: usize, can_restart: bool, banner: &str, down: f32, up: f32) {
    draw_rectangle(0.0, HUD_TOP, 1280.0, 768.0 - HUD_TOP, BAR_BG);
    draw_line(0.0, HUD_TOP, 1280.0, HUD_TOP, 1.0, Color::new(0.16, 0.17, 0.2, 1.0));

    draw_weapons(world, me);
    draw_scores(world, me);

    // Right: the status line, with the network cost under it in the quietest grey.
    right_text(banner, 1264.0, BAR_Y + 18.0, 18.0, TEXT);
    right_text(&format!("net  in {:.1} KB/s   out {:.1} KB/s", down / 1024.0, up / 1024.0), 1264.0, BAR_Y + 38.0, 16.0, TEXT_DIM);

    draw_messages(world, me, can_restart);
}

fn draw_weapons(world: &World, me: usize) {
    let player = &world.players[me];
    let mine = player_color(me);
    let (w, h, gap) = (172.0, BAR_H, 8.0);
    for (slot, weapon) in player.weapons.iter().enumerate() {
        let def = &WEAPONS[weapon.id as usize];
        let selected = slot == player.current;
        let x = 12.0 + slot as f32 * (w + gap);
        let y = BAR_Y;

        draw_rectangle(x, y, w, h, if selected { CARD_BG_SELECTED } else { CARD_BG });
        if selected {
            // Your colour marks the card as yours.
            draw_rectangle_lines(x, y, w, h, 2.0, mine);
        }

        let (name_color, ammo_color) = if selected { (TEXT, mine) } else { (TEXT_DIM, TEXT_DIM) };
        draw_text(&format!("{}", slot + 1), x + 7.0, y + 17.0, 16.0, TEXT_DIM);
        draw_text(def.name, x + 22.0, y + 17.0, 16.0, name_color);

        // Ammo bar: pips of the magazine; while reloading it refills as progress.
        let reloading = weapon.reload > 0.0;
        let fill = if reloading { 1.0 - weapon.reload / def.reload } else { weapon.ammo as f32 / def.mag as f32 };
        let (bar_x, bar_w) = (x + 8.0, w - 16.0);
        draw_rectangle(bar_x, y + 28.0, bar_w, 5.0, Color::new(0.03, 0.03, 0.04, 1.0));
        draw_rectangle(bar_x, y + 28.0, bar_w * fill, 5.0, if reloading { TEXT_DIM } else { ammo_color });
        let status = if reloading { "reloading".to_string() } else { format!("{}/{}", weapon.ammo, def.mag) };
        right_text(&status, x + w - 8.0, y + 17.0, 16.0, if reloading { TEXT_DIM } else { name_color });
    }
}

fn draw_scores(world: &World, me: usize) {
    let (chip_w, chip_h) = (66.0, 34.0);
    // A fixed spot (not centred) so it never collides with the weapon cards.
    let mut x = 580.0;
    for (i, p) in world.players.iter().enumerate() {
        let c = player_color(i);
        let y = BAR_Y + 5.0;
        if i == me {
            // "This is you": a tinted chip with an outline in your colour.
            draw_rectangle(x, y, chip_w - 4.0, chip_h, Color::new(c.r, c.g, c.b, 0.14));
            draw_rectangle_lines(x, y, chip_w - 4.0, chip_h, 2.0, c);
        }
        draw_poly(x + 17.0, y + chip_h / 2.0, 6, 8.0, 0.0, c);
        draw_text(&format!("{}", p.kills), x + 32.0, y + 25.0, 28.0, TEXT);
        x += chip_w;
    }
    draw_text(&format!("first to {SCORE_LIMIT}"), x + 8.0, BAR_Y + 28.0, 16.0, TEXT_DIM);
}

/// The two messages that belong over the arena. They get a dark plate so they
/// stay readable on top of whatever is behind them.
fn draw_messages(world: &World, me: usize, can_restart: bool) {
    if !world.players[me].alive {
        let left = world.players[me].respawn_timer.max(0.0).min(RESPAWN_TIME);
        plate(320.0, 70.0);
        center_text(&format!("respawn in {:.1}", left), 350.0, 40.0, TEXT_DIM);
    }
    if let Some(w) = world.winner {
        plate(268.0, 120.0);
        center_text(&format!("PLAYER {} WINS", w + 1), 320.0, 70.0, player_color(w as usize));
        // Only the host owns the match, so only the host can start a new one.
        let hint = if can_restart { "press R for a new match" } else { "waiting for the host to start a new match" };
        center_text(hint, 365.0, 28.0, TEXT_DIM);
    }
}

fn plate(y: f32, h: f32) {
    draw_rectangle(0.0, y, 1280.0, h, Color::new(0.03, 0.03, 0.04, 0.85));
}

fn right_text(text: &str, right: f32, y: f32, size: f32, color: Color) {
    let width = measure_text(text, None, size as u16, 1.0).width;
    draw_text(text, right - width, y, size, color);
}
