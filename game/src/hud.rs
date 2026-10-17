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
use sim::world::{MAX_HP, RESPAWN_TIME, SCORE_LIMIT};
use sim::{World, WEAPONS};

use crate::draw::{center_text, player_color};
use crate::view::HUD_TOP;

const BAR_BG: Color = Color::new(0.045, 0.05, 0.06, 1.0);
const CARD_BG: Color = Color::new(0.075, 0.08, 0.095, 1.0);
const CARD_BG_SELECTED: Color = Color::new(0.12, 0.13, 0.155, 1.0);
const TEXT: Color = Color::new(0.92, 0.93, 0.96, 1.0);
const TEXT_DIM: Color = Color::new(0.45, 0.47, 0.52, 1.0);
const BAR_Y: f32 = HUD_TOP + 8.0;
const BAR_H: f32 = 44.0;

/// `down`/`up` are bytes per second on the wire; `banner` is a line of status
/// (room code, ping) from whoever is playing; `frame_ms` the time per frame.
pub fn draw(world: &World, me: usize, can_restart: bool, banner: &str, down: f32, up: f32, frame_ms: f32) {
    draw_rectangle(0.0, HUD_TOP, 1280.0, 768.0 - HUD_TOP, BAR_BG);
    draw_health(world, me);
    draw_weapons(world, me);
    draw_scores(world, me);

    // Right: the status line, with the network cost under it in the quietest grey.
    right_text(banner, 1264.0, BAR_Y + 18.0, 18.0, TEXT);
    right_text(&format!("in {:.1} KB/s   out {:.1} KB/s   {:.1} ms", down / 1024.0, up / 1024.0, frame_ms), 1264.0, BAR_Y + 38.0, 14.0, TEXT_DIM);

    draw_messages(world, me, can_restart);
}

/// Your health is the top edge of the strip: a line in your colour that shrinks
/// from the right. It sits right under the arena, where the eye already goes,
/// and needs no room of its own. Below a third it pulses.
fn draw_health(world: &World, me: usize) {
    let player = &world.players[me];
    let c = player_color(me);
    draw_rectangle(0.0, HUD_TOP, 1280.0, 4.0, Color::new(0.11, 0.12, 0.15, 1.0));
    if player.alive {
        let frac = (player.hp / MAX_HP).clamp(0.0, 1.0);
        let pulse = if frac < 0.34 { 0.65 + 0.35 * (get_time() as f32 * 9.0).sin() } else { 1.0 };
        draw_rectangle(0.0, HUD_TOP, 1280.0 * frac, 4.0, Color::new(c.r, c.g, c.b, pulse));
    }
}

fn draw_weapons(world: &World, me: usize) {
    let player = &world.players[me];
    let mine = player_color(me);
    let (w, h, gap) = (108.0, BAR_H, 6.0);
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
        draw_text(&format!("{}", slot + 1), x + 6.0, y + 15.0, 14.0, TEXT_DIM);
        draw_text(def.name, x + 19.0, y + 15.0, 14.0, name_color);

        // Ammo. A small magazine is drawn as one segment per round (a Railgun is
        // one fat block, a Bomb two), so you can count what is left at a glance;
        // a big one is a plain bar plus the number. While reloading, either
        // becomes a progress bar.
        let reloading = weapon.reload > 0.0;
        let (bar_x, bar_w, bar_y) = (x + 6.0, w - 12.0, y + 22.0);
        let dark = Color::new(0.03, 0.03, 0.04, 1.0);
        if reloading {
            draw_rectangle(bar_x, bar_y, bar_w, 6.0, dark);
            draw_rectangle(bar_x, bar_y, bar_w * (1.0 - weapon.reload / def.reload), 6.0, TEXT_DIM);
        } else if def.mag <= 8 {
            let gap = 3.0;
            let seg = (bar_w - gap * (def.mag as f32 - 1.0)) / def.mag as f32;
            for k in 0..def.mag {
                let color = if k < weapon.ammo { ammo_color } else { dark };
                draw_rectangle(bar_x + k as f32 * (seg + gap), bar_y, seg, 6.0, color);
            }
        } else {
            draw_rectangle(bar_x, bar_y, bar_w, 6.0, dark);
            draw_rectangle(bar_x, bar_y, bar_w * weapon.ammo as f32 / def.mag as f32, 6.0, ammo_color);
        }
        if reloading {
            draw_text("reloading", x + 6.0, y + 40.0, 14.0, TEXT_DIM);
        } else if def.mag > 8 {
            draw_text(&format!("{}/{}", weapon.ammo, def.mag), x + 6.0, y + 40.0, 14.0, name_color);
        }
    }
}

fn draw_scores(world: &World, me: usize) {
    let (chip_w, chip_h) = (58.0, 34.0);
    // A fixed spot (not centred) so it never collides with the weapon cards.
    let mut x = 706.0;
    for (i, p) in world.players.iter().enumerate() {
        // A dead player's hexagon goes dark until they respawn.
        let c = player_color(i);
        let c = if p.alive { c } else { Color::new(c.r, c.g, c.b, 0.3) };
        let y = BAR_Y + 5.0;
        if i == me {
            // "This is you": a tinted chip with an outline in your colour.
            draw_rectangle(x, y, chip_w - 4.0, chip_h, Color::new(c.r, c.g, c.b, 0.14));
            draw_rectangle_lines(x, y, chip_w - 4.0, chip_h, 2.0, c);
        }
        draw_poly(x + 14.0, y + chip_h / 2.0, 6, 7.0, 0.0, c);
        draw_text(&format!("{}", p.kills), x + 27.0, y + 25.0, 26.0, TEXT);
        x += chip_w;
    }
    draw_text(&format!("first to {SCORE_LIMIT}"), x + 8.0, BAR_Y + 33.0, 14.0, TEXT_DIM);
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
