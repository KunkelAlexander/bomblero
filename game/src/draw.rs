// Draws the world. Read-only: this module takes `&World` and never changes it.
//
// Neon look = thin, saturated outlines on black. The glow shader (see view.rs)
// supplies the halo, so here we just draw crisp lines and dim fills.

use macroquad::prelude::*;
use sim::world::{MAX_HP, RESPAWN_TIME, SCORE_LIMIT};
use sim::{Tile, World, MAP_H, MAP_W, PLAYER_RADIUS, TILE, WEAPONS};

pub fn player_color(i: usize) -> Color {
    match i % 4 {
        0 => Color::new(0.1, 1.0, 0.95, 1.0),  // cyan
        1 => Color::new(1.0, 0.2, 0.8, 1.0),   // magenta
        2 => Color::new(0.6, 1.0, 0.2, 1.0),   // lime
        _ => Color::new(1.0, 0.85, 0.2, 1.0),  // yellow
    }
}

pub fn draw_world(world: &World, t: f32) {
    draw_tiles(world);
    draw_projectiles(world);
    draw_players(world, t);
}

fn draw_tiles(world: &World) {
    let solid = Color::new(0.25, 0.35, 1.0, 1.0);
    let brick = Color::new(1.0, 0.5, 0.15, 1.0);
    for ty in 0..MAP_H as i32 {
        for tx in 0..MAP_W as i32 {
            let (x, y) = (tx as f32 * TILE, ty as f32 * TILE);
            match world.map.get(tx, ty) {
                Tile::Empty => {}
                Tile::Solid => {
                    draw_rectangle(x + 2.0, y + 2.0, TILE - 4.0, TILE - 4.0, Color::new(0.05, 0.07, 0.25, 1.0));
                    draw_rectangle_lines(x + 2.0, y + 2.0, TILE - 4.0, TILE - 4.0, 2.0, solid);
                }
                Tile::Brick(hp) => {
                    // More damage = dimmer fill and more cracks.
                    draw_rectangle(x + 2.0, y + 2.0, TILE - 4.0, TILE - 4.0, Color::new(0.35, 0.15, 0.03, 0.2 + 0.1 * hp as f32));
                    draw_rectangle_lines(x + 2.0, y + 2.0, TILE - 4.0, TILE - 4.0, 2.0, brick);
                    if hp <= 2 {
                        draw_line(x + 6.0, y + 8.0, x + 18.0, y + 20.0, 2.0, brick);
                    }
                    if hp <= 1 {
                        draw_line(x + 26.0, y + 6.0, x + 14.0, y + 24.0, 2.0, brick);
                    }
                }
            }
        }
    }
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
        draw_circle_lines(p.pos.x, p.pos.y, PLAYER_RADIUS, 2.5, c);
        let tip = p.pos + vec2(p.aim.cos(), p.aim.sin()) * (PLAYER_RADIUS + 9.0);
        draw_line(p.pos.x, p.pos.y, tip.x, tip.y, 2.5, c);

        let w = 26.0;
        let (x, y) = (p.pos.x - w / 2.0, p.pos.y - PLAYER_RADIUS - 9.0);
        draw_line(x, y, x + w, y, 3.0, Color::new(c.r, c.g, c.b, 0.2));
        draw_line(x, y, x + w * p.hp / MAX_HP, y, 3.0, c);
    }
}

pub fn draw_hud(world: &World, me: usize) {
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
        center_text("press R for a new match", 350.0, 28.0, GRAY);
    }
}

/// Bytes per second the wire carries, so you can see what the netcode costs.
pub fn draw_net_stats(down: f32, up: f32) {
    draw_text(&format!("net  in {:.1} KB/s   out {:.1} KB/s", down / 1024.0, up / 1024.0), 930.0, 22.0, 18.0, GRAY);
}

fn center_text(text: &str, y: f32, size: f32, color: Color) {
    let dim = measure_text(text, None, size as u16, 1.0);
    draw_text(text, (sim::TILE * sim::MAP_W as f32 - dim.width) / 2.0, y, size, color);
}
