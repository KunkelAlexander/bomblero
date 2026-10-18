// The heads-up display: one quiet strip under the map.
//
// Design rules:
//   * It lives in its own strip, never over the playfield, on an opaque dark
//     background, so nothing in it competes with what is happening in the arena.
//   * It is drawn in picture coordinates (see view.rs), so it keeps its size
//     on any screen. Narrow screens get a taller strip: weapons on the first
//     row, scores (and on very narrow ones the status) on the rows below.
//   * It is drawn after the glow, on the finished picture (see `Glow::ui`):
//     glowing text is hard to read. Colour is kept for *ownership*: your colour
//     outlines your weapon card and your score chip, and each player's hexagon
//     on the scoreboard is their in-game colour.
//   * Hierarchy: the selected weapon is bright and carries the ammo bar;
//     everything else is dim. A match has five weapons, so every card is full
//     width; the menu's shooting range has ten, so there only the selected card
//     is wide and the rest shrink to a number and an ammo bar.
//
// Read-only like all drawing: it takes `&World` and never changes it.

use macroquad::prelude::*;
use sim::world::{MAX_HP, RESPAWN_TIME};
use sim::{World, WEAPONS};

use crate::draw::{center_wide, player_color};
use crate::view::{self, Layout};

const BAR_BG: Color = Color::new(0.045, 0.05, 0.06, 1.0);
const CARD_BG: Color = Color::new(0.075, 0.08, 0.095, 1.0);
const CARD_BG_SELECTED: Color = Color::new(0.12, 0.13, 0.155, 1.0);
const TEXT: Color = Color::new(0.92, 0.93, 0.96, 1.0);
// Secondary text. At 0.6 it keeps WCAG's 4.5:1 contrast for small text
// against the dark cards (0.45 was about 3:1: hard to read on a dim screen).
const TEXT_DIM: Color = Color::new(0.6, 0.62, 0.67, 1.0);
const BAR_H: f32 = 44.0;

/// `down`/`up` are bytes per second on the wire; `banner` is a line of status
/// (room code, ping) from whoever is playing; `frame_ms` the time per frame.
pub fn draw(world: &World, me: usize, can_restart: bool, banner: &str, down: f32, up: f32, frame_ms: f32) {
    let l = view::layout();
    let (top, width) = (l.hud_top(), l.size.x);
    let bar_y = top + 8.0;
    draw_rectangle(0.0, top, width, l.size.y - top, BAR_BG);
    draw_health(world, me, top, width);
    // One row: weapons left, scores from x=706, status right. Two rows: the
    // scores and status move under the weapons. Three: the status gets its own.
    draw_weapons(world, me);
    let (scores_x, scores_y) = match l.hud_rows {
        1 => (cards_end(world, &l) + 12.0, bar_y),
        _ => (12.0, bar_y + 50.0),
    };
    draw_scores(world, me, scores_x, scores_y);

    // The status line, with the network cost under it in the quietest grey.
    // Normally at the right of the strip; a narrow one-row strip has no room
    // left, so it goes to the top left of the arena on a dark plate.
    let net = format!("in {:.1} KB/s   out {:.1} KB/s   {:.1} ms", down / 1024.0, up / 1024.0, frame_ms);
    if l.hud_rows == 1 && width < WIDE {
        let w = measure_text(banner, None, 18, 1.0).width.max(measure_text(&net, None, 14, 1.0).width);
        draw_rectangle(0.0, 0.0, w + 24.0, 48.0, Color::new(0.03, 0.03, 0.04, 0.7));
        draw_text(banner, 12.0, 20.0, 18.0, TEXT);
        draw_text(&net, 12.0, 39.0, 14.0, TEXT_DIM);
    } else {
        let status_y = bar_y + (l.hud_rows - 1) as f32 * 50.0;
        right_text(banner, width - 16.0, status_y + 18.0, 18.0, TEXT);
        right_text(&net, width - 16.0, status_y + 38.0, 14.0, TEXT_DIM);
    }

    draw_messages(world, me, can_restart, &l);
}

/// Your health is the top edge of the strip: a line in your colour that shrinks
/// from the right. It sits right under the arena, where the eye already goes,
/// and needs no room of its own. Below a third it pulses.
fn draw_health(world: &World, me: usize, top: f32, width: f32) {
    let player = &world.players[me];
    let c = player_color(me);
    draw_rectangle(0.0, top, width, 4.0, Color::new(0.11, 0.12, 0.15, 1.0));
    if player.alive {
        let frac = (player.hp / MAX_HP).clamp(0.0, 1.0);
        let pulse = if frac < 0.34 { 0.65 + 0.35 * (get_time() as f32 * 9.0).sin() } else { 1.0 };
        draw_rectangle(0.0, top, width * frac, 4.0, Color::new(c.r, c.g, c.b, pulse));
    }
}

/// From this width the HUD is the classic desktop strip: scores at a fixed
/// x=706 and the status line at the right.
const WIDE: f32 = 1270.0;

/// Where the weapon cards must stop.
fn cards_end(world: &World, l: &Layout) -> f32 {
    match l.hud_rows {
        // The classic strip: before the scores at x=706.
        1 if l.size.x >= WIDE => 694.0,
        // A narrow single row: leave room for the scores (58 px a player plus
        // "first to N").
        1 => l.size.x - 24.0 - world.players.len() as f32 * 58.0 - 90.0,
        _ => l.size.x - 12.0,
    }
}

/// Where each weapon card sits, in picture coordinates. Shared by drawing and
/// by `weapon_at`, so a tap always hits what is drawn.
fn card_rects(world: &World, me: usize) -> Vec<Rect> {
    let l = view::layout();
    let player = &world.players[me];
    let end = cards_end(world, &l);
    let n = player.weapons.len() as f32;
    // Compact when there are many weapons or not enough room for full cards;
    // compact cards shrink further on very narrow screens.
    let compact = n > 6.0 || 12.0 + n * 114.0 > end;
    let w_compact = ((end - 12.0 - 108.0) / (n - 1.0).max(1.0) - 4.0).min(50.0);
    let (w_other, gap) = if compact { (w_compact, 4.0) } else { (108.0, 6.0) };
    let mut x = 12.0;
    let mut rects = Vec::new();
    for slot in 0..player.weapons.len() {
        let w = if slot == player.current { 108.0 } else { w_other };
        rects.push(Rect::new(x, l.hud_top() + 8.0, w, BAR_H));
        x += w + gap;
    }
    rects
}

/// The weapon card under a picture-space point (a tap or click on the HUD).
pub fn weapon_at(world: &World, me: usize, pos: Vec2) -> Option<usize> {
    card_rects(world, me).iter().position(|r| r.contains(pos))
}

fn draw_weapons(world: &World, me: usize) {
    let player = &world.players[me];
    let mine = player_color(me);
    for (slot, (weapon, r)) in player.weapons.iter().zip(card_rects(world, me)).enumerate() {
        let def = &WEAPONS[weapon.id as usize];
        let selected = slot == player.current;
        let (x, y, w, h) = (r.x, r.y, r.w, r.h);
        let words = w >= 108.0; // text only fits on a wide card

        draw_rectangle(x, y, w, h, if selected { CARD_BG_SELECTED } else { CARD_BG });
        if selected {
            // Your colour marks the card as yours.
            draw_rectangle_lines(x, y, w, h, 2.0, mine);
        }

        let (name_color, ammo_color) = if selected { (TEXT, mine) } else { (TEXT_DIM, TEXT_DIM) };
        // Slot 10 is the 0 key.
        draw_text(&format!("{}", (slot + 1) % 10), x + 6.0, y + 15.0, 14.0, TEXT_DIM);
        if words {
            draw_text(def.name, x + 19.0, y + 15.0, 14.0, name_color);
        }

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
        if words && reloading {
            draw_text("reloading", x + 6.0, y + 40.0, 14.0, TEXT_DIM);
        } else if words && def.mag > 8 {
            draw_text(&format!("{}/{}", weapon.ammo, def.mag), x + 6.0, y + 40.0, 14.0, name_color);
        }
    }
}

fn draw_scores(world: &World, me: usize, mut x: f32, bar_y: f32) {
    let (chip_w, chip_h) = (58.0, 34.0);
    for (i, p) in world.players.iter().enumerate() {
        // A dead player's hexagon goes dark until they respawn.
        let c = player_color(i);
        let c = if p.alive { c } else { Color::new(c.r, c.g, c.b, 0.3) };
        let y = bar_y + 5.0;
        if i == me {
            // "This is you": a tinted chip with an outline in your colour.
            draw_rectangle(x, y, chip_w - 4.0, chip_h, Color::new(c.r, c.g, c.b, 0.14));
            draw_rectangle_lines(x, y, chip_w - 4.0, chip_h, 2.0, c);
        }
        draw_poly(x + 14.0, y + chip_h / 2.0, 6, 7.0, 0.0, c);
        draw_text(&format!("{}", p.kills), x + 27.0, y + 25.0, 26.0, TEXT);
        x += chip_w;
    }
    draw_text(&format!("first to {}", world.score_limit), x + 8.0, bar_y + 33.0, 14.0, TEXT_DIM);
}

/// The two messages that belong over the arena. They get a dark plate so they
/// stay readable on top of whatever is behind them.
fn draw_messages(world: &World, me: usize, can_restart: bool, l: &Layout) {
    // Centred on the arena part of the picture.
    let cy = l.hud_top() / 2.0;
    if !world.players[me].alive {
        let left = world.players[me].respawn_timer.max(0.0).min(RESPAWN_TIME);
        plate(cy - 35.0, 70.0, l.size.x);
        center_wide(&format!("respawn in {:.1}", left), cy - 5.0, 40.0, TEXT_DIM);
    }
    if let Some(w) = world.winner {
        plate(cy - 87.0, 120.0, l.size.x);
        center_wide(&format!("PLAYER {} WINS", w + 1), cy - 35.0, 70.0, player_color(w as usize));
        // Only the host owns the match, so only the host can start a new one.
        let hint = if can_restart && crate::touch::active() { "tap for a new match" } else if can_restart { "press R for a new match" } else { "waiting for the host to start a new match" };
        center_wide(hint, cy + 10.0, 28.0, TEXT_DIM);
    }
}

fn plate(y: f32, h: f32, width: f32) {
    draw_rectangle(0.0, y, width, h, Color::new(0.03, 0.03, 0.04, 0.85));
}

fn right_text(text: &str, right: f32, y: f32, size: f32, color: Color) {
    let width = measure_text(text, None, size as u16, 1.0).width;
    draw_text(text, right - width, y, size, color);
}
