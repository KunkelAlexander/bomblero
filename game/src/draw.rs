// Draws the world. Read-only: this module takes `&World` and never changes it.
//
// Neon look = thin, saturated outlines on black. The glow shader (see view.rs)
// supplies the halo, so here we just draw crisp lines and dim fills.

use macroquad::prelude::*;
use sim::world::{MAX_HP, RESPAWN_TIME, SCORE_LIMIT};
use sim::{World, PLAYER_RADIUS, WEAPONS};

pub fn player_color(i: usize) -> Color {
    match i % 4 {
        0 => Color::new(0.1, 1.0, 0.95, 1.0),  // cyan
        1 => Color::new(1.0, 0.2, 0.8, 1.0),   // magenta
        2 => Color::new(0.6, 1.0, 0.2, 1.0),   // lime
        _ => Color::new(1.0, 0.85, 0.2, 1.0),  // yellow
    }
}

/// Draws everything except the terrain (see terrain.rs), which the presenter
/// draws first so it sits underneath.
pub fn draw_world(world: &World, t: f32) {
    draw_projectiles(world);
    draw_players(world, t);
}

fn draw_projectiles(world: &World) {
    for p in &world.projectiles {
        let dir = p.vel.normalize_or_zero();
        let rocket = p.weapon == sim::weapons::ROCKET;
        let (trail, color) = if rocket {
            (18.0, Color::new(1.0, 0.55, 0.1, 1.0))
        } else {
            (9.0, player_color(p.owner as usize))
        };
        let tail = p.pos - dir * trail;
        draw_line(tail.x, tail.y, p.pos.x, p.pos.y, 2.5, color);
        draw_circle(p.pos.x, p.pos.y, if rocket { 4.0 } else { 2.5 }, WHITE);
    }
}

fn draw_players(world: &World, t: f32) {
    for (i, p) in world.players.iter().enumerate() {
        if !p.alive {
            continue;
        }
        // Blink while spawn-protected.
        if p.invuln > 0.0 && (t * 14.0) as i32 % 2 == 0 {
            continue;
        }
        let c = player_color(i);
        // The arena is grey, so players are the only saturated, filled shapes on
        // screen: a dark body (so the ring never merges with a wall behind it), a
        // bright solid core the eye finds in its periphery, and the ring as the
        // identity accent.
        draw_circle(p.pos.x, p.pos.y, PLAYER_RADIUS, Color::new(c.r * 0.2, c.g * 0.2, c.b * 0.2, 1.0));
        draw_circle_lines(p.pos.x, p.pos.y, PLAYER_RADIUS, 2.5, c);
        draw_circle(p.pos.x, p.pos.y, PLAYER_RADIUS * 0.6, c);
        draw_circle(p.pos.x, p.pos.y, PLAYER_RADIUS * 0.25, WHITE);
        // A filled wedge past the ring says which way you are facing.
        let dir = vec2(p.aim.cos(), p.aim.sin());
        let (base, tip, side) = (p.pos + dir * (PLAYER_RADIUS + 2.0), p.pos + dir * (PLAYER_RADIUS + 12.0), vec2(-dir.y, dir.x) * 5.0);
        draw_triangle(base + side, base - side, tip, c);

        let w = 26.0;
        let (x, y) = (p.pos.x - w / 2.0, p.pos.y - PLAYER_RADIUS - 9.0);
        draw_line(x, y, x + w, y, 3.0, Color::new(c.r, c.g, c.b, 0.2));
        draw_line(x, y, x + w * p.hp / MAX_HP, y, 3.0, c);
    }
}

pub fn draw_hud(world: &World, me: usize, can_restart: bool) {
    // Scoreboard along the top.
    let mut x = 520.0;
    for (i, p) in world.players.iter().enumerate() {
        draw_text(&format!("{}", p.kills), x, 26.0, 30.0, player_color(i));
        x += 60.0;
    }
    draw_text(&format!("first to {SCORE_LIMIT}"), x + 10.0, 24.0, 20.0, DARKGRAY);

    // Weapon slots for the local player, bottom left.
    let player = &world.players[me];
    for (slot, w) in player.weapons.iter().enumerate() {
        let def = &WEAPONS[w.id as usize];
        let selected = slot == player.current;
        let color = if selected { WHITE } else { GRAY };
        let status = if w.reload > 0.0 {
            format!("reloading {:.1}s", w.reload)
        } else {
            format!("{}/{}", w.ammo, def.mag)
        };
        draw_text(
            &format!("{} {}  {}", slot + 1, def.name, status),
            16.0,
            660.0 - (player.weapons.len() - slot) as f32 * 22.0 + 22.0,
            22.0,
            color,
        );
    }

    if !player.alive {
        let left = player.respawn_timer.max(0.0).min(RESPAWN_TIME);
        center_text(&format!("respawn in {:.1}", left), 340.0, 40.0, GRAY);
    }
    if let Some(w) = world.winner {
        center_text(&format!("PLAYER {} WINS", w + 1), 300.0, 70.0, player_color(w as usize));
        // Only the host owns the match, so only the host can start a new one.
        let hint = if can_restart { "press R for a new match" } else { "waiting for the host to start a new match" };
        center_text(hint, 350.0, 28.0, GRAY);
    }
}

/// Bytes per second the wire carries, so you can see what the netcode costs.
pub fn draw_net_stats(down: f32, up: f32) {
    draw_text(&format!("net  in {:.1} KB/s   out {:.1} KB/s", down / 1024.0, up / 1024.0), 930.0, 22.0, 18.0, GRAY);
}

pub fn center_text(text: &str, y: f32, size: f32, color: Color) {
    let dim = measure_text(text, None, size as u16, 1.0);
    draw_text(text, (sim::TILE * sim::MAP_W as f32 - dim.width) / 2.0, y, size, color);
}
