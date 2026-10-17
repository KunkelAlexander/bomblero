// Draws the world. Read-only: this module takes `&World` and never changes it.
//
// The look, in one line: a soft grey stone world (terrain.rs) with crisp,
// angular, coloured actors on top. Colour means "who" (player colours),
// orange-white means "danger" (rockets, explosions). The full set of rules is
// at the top of effects.rs.

use macroquad::prelude::*;
use sim::world::MAX_HP;
use sim::{World, PLAYER_RADIUS};

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

/// A soft pool of coloured light lying on the floor, brightest in the middle.
/// Stacked translucent discs fake a gradient cheaply; it is how colour spills
/// onto the grey stone without making the stone itself glow.
pub fn light_pool(pos: Vec2, radius: f32, color: Color, strength: f32) {
    const LAYERS: usize = 8;
    for i in 0..LAYERS {
        let r = radius * (1.0 - i as f32 / LAYERS as f32);
        draw_circle(pos.x, pos.y, r, Color::new(color.r, color.g, color.b, strength * 1.6 / LAYERS as f32));
    }
}

/// Draws everything except the terrain (see terrain.rs), which the presenter
/// draws first so it sits underneath.
pub fn draw_world(world: &World, t: f32) {
    // Light first, so it lies under the things casting it.
    for p in world.players.iter().enumerate().filter(|(_, p)| p.alive) {
        light_pool(p.1.pos, 70.0, player_color(p.0), 0.22);
    }
    for p in &world.projectiles {
        let (radius, color) = if p.weapon == sim::weapons::ROCKET { (60.0, DANGER) } else { (28.0, player_color(p.owner as usize)) };
        light_pool(p.pos, radius, color, 0.2);
    }
    draw_projectiles(world);
    draw_players(world, t);
}

fn draw_projectiles(world: &World) {
    for p in &world.projectiles {
        let dir = p.vel.normalize_or_zero();
        let side = vec2(-dir.y, dir.x);
        if p.weapon == sim::weapons::ROCKET {
            // A sharp white warhead on a tapering flame.
            let flame = Color::new(DANGER.r, DANGER.g, DANGER.b, 0.85);
            draw_triangle(p.pos + side * 4.5, p.pos - side * 4.5, p.pos - dir * 24.0, flame);
            draw_triangle(p.pos + side * 2.0, p.pos - side * 2.0, p.pos - dir * 13.0, Color::new(1.0, 0.95, 0.6, 1.0));
            draw_triangle(p.pos + dir * 9.0, p.pos - dir * 4.0 + side * 4.0, p.pos - dir * 4.0 - side * 4.0, WHITE);
        } else {
            // A bolt is a diamond with a white core and a fading tail.
            let c = player_color(p.owner as usize);
            draw_triangle(p.pos + side * 2.5, p.pos - side * 2.5, p.pos - dir * 16.0, Color::new(c.r, c.g, c.b, 0.45));
            let (front, back) = (p.pos + dir * 8.0, p.pos - dir * 5.0);
            draw_triangle(front, p.pos + side * 3.5, p.pos - side * 3.5, c);
            draw_triangle(back, p.pos + side * 3.5, p.pos - side * 3.5, c);
            draw_triangle(p.pos + dir * 5.0, p.pos + side * 1.6, p.pos - side * 1.6, WHITE);
            draw_triangle(p.pos - dir * 2.5, p.pos + side * 1.6, p.pos - side * 1.6, WHITE);
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
