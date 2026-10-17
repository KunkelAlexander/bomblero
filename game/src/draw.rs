// Draws the world. Read-only: this module takes `&World` and never changes it.
//
// The look, in one line: a soft grey stone world (terrain.rs) with crisp,
// angular, coloured actors on top. Colour means "who" (player colours),
// orange-white means "danger" (rockets, explosions). The full set of rules is
// at the top of effects.rs.

use macroquad::prelude::*;
use sim::world::MAX_HP;
use sim::{World, PLAYER_RADIUS, WEAPONS};

pub fn player_color(i: usize) -> Color {
    match i % 4 {
        0 => Color::new(0.1, 1.0, 0.95, 1.0),  // cyan
        1 => Color::new(1.0, 0.2, 0.8, 1.0),   // magenta
        2 => Color::new(0.6, 1.0, 0.2, 1.0),   // lime
        _ => Color::new(1.0, 0.85, 0.2, 1.0),  // yellow
    }
}

/// The colour of danger: rockets and explosions, whoever fired them.
pub const DANGER: Color = Color::new(1.0, 0.55, 0.1, 1.0);

/// Draws the actors. The presenter draws the terrain first (terrain.rs) and
/// lights it (lights.rs) before this, so the actors are never darkened.
pub fn draw_world(world: &World, t: f32) {
    draw_projectiles(world);
    draw_players(world, t);
}

fn draw_projectiles(world: &World) {
    for p in &world.projectiles {
        let dir = p.vel.normalize_or_zero();
        let side = vec2(-dir.y, dir.x);
        let c = player_color(p.owner as usize);
        match p.weapon {
            sim::weapons::ROCKET => {
                // A sharp white warhead on a tapering flame.
                let flame = Color::new(DANGER.r, DANGER.g, DANGER.b, 0.85);
                draw_triangle(p.pos + side * 4.5, p.pos - side * 4.5, p.pos - dir * 24.0, flame);
                draw_triangle(p.pos + side * 2.0, p.pos - side * 2.0, p.pos - dir * 13.0, Color::new(1.0, 0.95, 0.6, 1.0));
                draw_triangle(p.pos + dir * 9.0, p.pos - dir * 4.0 + side * 4.0, p.pos - dir * 4.0 - side * 4.0, WHITE);
            }
            sim::weapons::BOMB => {
                // Orange means danger. The core blinks faster as the fuse burns
                // down, so you can read when to be somewhere else.
                let burnt = 1.0 - p.life / WEAPONS[sim::weapons::BOMB as usize].lifetime;
                let lit = (burnt * (8.0 + 30.0 * burnt) * std::f32::consts::TAU).sin() > 0.0;
                draw_poly(p.pos.x, p.pos.y, 6, 9.0, 0.0, Color::new(0.12, 0.05, 0.02, 1.0));
                draw_poly_lines(p.pos.x, p.pos.y, 6, 9.0, 0.0, 2.0, DANGER);
                draw_poly(p.pos.x, p.pos.y, 6, 4.0, 0.0, if lit { WHITE } else { DANGER });
            }
            sim::weapons::SCATTER => {
                // Eight of these fly at once, so each is small.
                draw_poly(p.pos.x, p.pos.y, 4, 3.5, dir.y.atan2(dir.x).to_degrees(), c);
                draw_poly(p.pos.x, p.pos.y, 4, 1.5, dir.y.atan2(dir.x).to_degrees(), WHITE);
            }
            sim::weapons::BOUNCER => {
                // A spinning hollow diamond: it looks like something that bounces.
                let spin = world.tick as f32 * 6.0;
                draw_poly(p.pos.x, p.pos.y, 4, 6.0, spin, Color::new(c.r * 0.25, c.g * 0.25, c.b * 0.25, 1.0));
                draw_poly_lines(p.pos.x, p.pos.y, 4, 6.5, spin, 2.0, c);
                draw_circle(p.pos.x, p.pos.y, 1.8, WHITE);
            }
            _ => {
                // A bolt is a diamond with a white core and a fading tail.
                draw_triangle(p.pos + side * 2.5, p.pos - side * 2.5, p.pos - dir * 16.0, Color::new(c.r, c.g, c.b, 0.45));
                let (front, back) = (p.pos + dir * 8.0, p.pos - dir * 5.0);
                draw_triangle(front, p.pos + side * 3.5, p.pos - side * 3.5, c);
                draw_triangle(back, p.pos + side * 3.5, p.pos - side * 3.5, c);
                draw_triangle(p.pos + dir * 5.0, p.pos + side * 1.6, p.pos - side * 1.6, WHITE);
                draw_triangle(p.pos - dir * 2.5, p.pos + side * 1.6, p.pos - side * 1.6, WHITE);
            }
        }
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
        // screen: a dark hexagonal body (so the outline never merges with a wall
        // behind it), a bright solid core the eye finds in its periphery, and the
        // outline as the identity accent. The hexagon is the game's "energy"
        // shape and one corner always points where you aim, so facing reads
        // even before you look at the wedge.
        let turn = p.aim.to_degrees();
        draw_poly(p.pos.x, p.pos.y, 6, PLAYER_RADIUS + 1.0, turn, Color::new(c.r * 0.2, c.g * 0.2, c.b * 0.2, 1.0));
        draw_poly_lines(p.pos.x, p.pos.y, 6, PLAYER_RADIUS + 1.0, turn, 2.5, c);
        draw_poly(p.pos.x, p.pos.y, 6, PLAYER_RADIUS * 0.6, turn, c);
        draw_circle(p.pos.x, p.pos.y, PLAYER_RADIUS * 0.25, WHITE);
        // A filled wedge past the body says which way you are facing.
        let dir = vec2(p.aim.cos(), p.aim.sin());
        let (base, tip, side) = (p.pos + dir * (PLAYER_RADIUS + 3.0), p.pos + dir * (PLAYER_RADIUS + 13.0), vec2(-dir.y, dir.x) * 5.0);
        draw_triangle(base + side, base - side, tip, c);

        let w = 26.0;
        let (x, y) = (p.pos.x - w / 2.0, p.pos.y - PLAYER_RADIUS - 9.0);
        draw_line(x, y, x + w, y, 3.0, Color::new(c.r, c.g, c.b, 0.2));
        draw_line(x, y, x + w * p.hp / MAX_HP, y, 3.0, c);
    }
}

pub fn center_text(text: &str, y: f32, size: f32, color: Color) {
    let dim = measure_text(text, None, size as u16, 1.0);
    draw_text(text, (sim::TILE * sim::MAP_W as f32 - dim.width) / 2.0, y, size, color);
}
