// Draws the world. Read-only: this module takes `&World` and never changes it.
//
// The look, in one line: a soft grey stone world (terrain.rs) with crisp,
// angular, coloured actors on top. Colour means "who" (player colours),
// orange-white means "danger" (rockets, explosions). The full set of rules is
// at the top of effects.rs.

use macroquad::prelude::*;
use sim::weapons::Special;
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
    draw_deployables(world, t);
    draw_projectiles(world);
    draw_players(world, t);
}

/// Oil puddles (flat on the floor, so first) and mirror panes.
fn draw_deployables(world: &World, t: f32) {
    for d in &world.deployables {
        let c = player_color(d.owner as usize);
        match WEAPONS[d.weapon as usize].special {
            Special::Slick(radius, seconds) => {
                // It spreads out in a quarter second, and fades away in its last one.
                let spread = ((seconds - d.life) / 0.25).min(1.0);
                let fade = d.life.min(1.0);
                let r = radius * (1.0 - (1.0 - spread) * (1.0 - spread));
                // A blob, not a circle: five overlapping discs whose offsets
                // come from the puddle's position, so each one has its own shape.
                let seed = d.pos.x * 0.37 + d.pos.y * 0.61;
                let oil = Color::new(0.02, 0.02, 0.05, 0.8 * fade);
                draw_circle(d.pos.x, d.pos.y, r * 0.8, oil);
                for k in 0..5 {
                    let a = seed + k as f32 * 1.26;
                    draw_circle(d.pos.x + a.cos() * r * 0.3, d.pos.y + a.sin() * r * 0.3, r * 0.6, oil);
                }
                // The rainbow sheen of oil: thin rings that slowly turn.
                for (k, color) in [(0.0, (0.1, 1.0, 0.95)), (2.1, (1.0, 0.2, 0.8)), (4.2, (0.6, 1.0, 0.2))] {
                    let a = t * 0.5 + k;
                    draw_arc(d.pos.x, d.pos.y, 24, r * (0.45 + 0.1 * k), a.to_degrees(), 2.0, 70.0, Color::new(color.0, color.1, color.2, 0.5 * fade));
                }
            }
            Special::Mirror(half, seconds) => {
                // Dim and blinking when about to vanish.
                let dying = d.life < 1.0 && (t * 14.0) as i32 % 2 == 0;
                let alpha = if dying { 0.35 } else { 1.0 };
                let (normal, along) = (vec2(d.angle.cos(), d.angle.sin()), vec2(-d.angle.sin(), d.angle.cos()));
                let (a, b) = (d.pos + along * half, d.pos - along * half);
                // A pane of glass: a faint body, a pale core, the owner's colour
                // as the edge, and a glint that sweeps along it.
                draw_line(a.x, a.y, b.x, b.y, 7.0, Color::new(c.r, c.g, c.b, 0.3 * alpha));
                draw_line(a.x, a.y, b.x, b.y, 2.5, Color::new(0.85 + 0.15 * c.r, 0.9 + 0.1 * c.g, 1.0, alpha));
                let glint = d.pos + along * half * (((t * 1.5 + seconds - d.life).rem_euclid(1.0)) * 2.0 - 1.0);
                draw_line(glint.x - normal.x * 5.0, glint.y - normal.y * 5.0, glint.x + normal.x * 5.0, glint.y + normal.y * 5.0, 2.0, Color::new(1.0, 1.0, 1.0, 0.9 * alpha));
                draw_circle(a.x, a.y, 3.0, Color::new(c.r, c.g, c.b, alpha));
                draw_circle(b.x, b.y, 3.0, Color::new(c.r, c.g, c.b, alpha));
            }
            _ => {}
        }
    }
}

fn draw_projectiles(world: &World) {
    for p in &world.projectiles {
        let dir = p.vel.normalize_or_zero();
        let side = vec2(-dir.y, dir.x);
        let c = player_color(p.owner as usize);
        match p.weapon {
            sim::weapons::ROCKET | sim::weapons::NOVA => {
                // A sharp white warhead on a tapering flame. The Nova is the same
                // rocket twice the size, with a pulsing halo: you see it coming.
                let s = if p.weapon == sim::weapons::NOVA { 2.2 } else { 1.0 };
                if s > 1.0 {
                    let pulse = 0.5 + 0.5 * (world.tick as f32 * 0.4).sin();
                    draw_poly_lines(p.pos.x, p.pos.y, 6, 16.0 + 6.0 * pulse, dir.y.atan2(dir.x).to_degrees(), 2.0, Color::new(DANGER.r, DANGER.g, DANGER.b, 0.8 - 0.4 * pulse));
                }
                let flame = Color::new(DANGER.r, DANGER.g, DANGER.b, 0.85);
                draw_triangle(p.pos + side * 4.5 * s, p.pos - side * 4.5 * s, p.pos - dir * 24.0 * s, flame);
                draw_triangle(p.pos + side * 2.0 * s, p.pos - side * 2.0 * s, p.pos - dir * 13.0 * s, Color::new(1.0, 0.95, 0.6, 1.0));
                draw_triangle(p.pos + dir * 9.0 * s, p.pos - dir * 4.0 * s + side * 4.0 * s, p.pos - dir * 4.0 * s - side * 4.0 * s, WHITE);
            }
            sim::weapons::CLUSTER => {
                // A bomb-like shell that tumbles, with three bomblets showing
                // through: it looks like something that is about to split.
                let spin = world.tick as f32 * 5.0;
                draw_poly(p.pos.x, p.pos.y, 6, 8.0, spin, Color::new(0.12, 0.05, 0.02, 1.0));
                draw_poly_lines(p.pos.x, p.pos.y, 6, 8.0, spin, 2.0, DANGER);
                for k in 0..3 {
                    let a = spin.to_radians() + k as f32 * std::f32::consts::TAU / 3.0;
                    draw_circle(p.pos.x + a.cos() * 3.5, p.pos.y + a.sin() * 3.5, 1.6, WHITE);
                }
            }
            sim::weapons::BOMBLET => {
                draw_poly(p.pos.x, p.pos.y, 4, 3.5, 45.0, DANGER);
                draw_circle(p.pos.x, p.pos.y, 1.4, WHITE);
            }
            sim::weapons::SWARM => {
                // Six of these fly at once, so each is a small needle in its
                // owner's colour with a short orange tail.
                draw_triangle(p.pos + side * 2.0, p.pos - side * 2.0, p.pos - dir * 12.0, Color::new(DANGER.r, DANGER.g, DANGER.b, 0.7));
                draw_triangle(p.pos + dir * 6.0, p.pos - dir * 3.0 + side * 2.5, p.pos - dir * 3.0 - side * 2.5, c);
                draw_circle(p.pos.x, p.pos.y, 1.2, WHITE);
            }
            sim::weapons::TELEPORT => {
                // A hollow hexagon that spins and breathes, in its owner's colour:
                // not a weapon of danger, so not orange.
                let pulse = 0.5 + 0.5 * (world.tick as f32 * 0.5).sin();
                let spin = world.tick as f32 * 8.0;
                draw_poly(p.pos.x, p.pos.y, 6, 7.0, spin, Color::new(c.r * 0.2, c.g * 0.2, c.b * 0.2, 1.0));
                draw_poly_lines(p.pos.x, p.pos.y, 6, 7.0 + 3.0 * pulse, spin, 2.0, c);
                draw_circle(p.pos.x, p.pos.y, 2.0, WHITE);
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
            sim::weapons::SHEEP => {
                // A puff of wool with a pink heart.
                for k in 0..5 {
                    let a = k as f32 * 1.2566 + world.tick as f32 * 0.2;
                    draw_circle(p.pos.x + a.cos() * 3.0, p.pos.y + a.sin() * 3.0, 3.2, WHITE);
                }
                draw_circle(p.pos.x, p.pos.y, 2.4, Color::new(1.0, 0.55, 0.7, 1.0));
            }
            sim::weapons::CONFUSER => {
                // Three dots chasing each other round a core: it looks dizzy.
                let spin = world.tick as f32 * 0.5;
                for k in 0..3 {
                    let a = spin + k as f32 * std::f32::consts::TAU / 3.0;
                    draw_circle(p.pos.x + a.cos() * 5.5, p.pos.y + a.sin() * 5.5, 2.2, c);
                }
                draw_circle(p.pos.x, p.pos.y, 2.0, WHITE);
            }
            sim::weapons::OIL => {
                // A dark can with an oily rainbow rim.
                draw_circle(p.pos.x, p.pos.y, 5.5, Color::new(0.02, 0.02, 0.05, 1.0));
                draw_circle_lines(p.pos.x, p.pos.y, 5.5, 1.5, Color::new(0.1, 1.0, 0.95, 1.0));
                draw_circle_lines(p.pos.x, p.pos.y, 3.5, 1.0, Color::new(1.0, 0.2, 0.8, 1.0));
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
        if p.sheep > 0.0 {
            draw_sheep(p, c, t);
        } else {
            draw_poly(p.pos.x, p.pos.y, 6, PLAYER_RADIUS + 1.0, turn, Color::new(c.r * 0.2, c.g * 0.2, c.b * 0.2, 1.0));
            draw_poly_lines(p.pos.x, p.pos.y, 6, PLAYER_RADIUS + 1.0, turn, 2.5, c);
            // A paler line down the middle of the outline makes it a lit neon
            // tube, like the title: the glow around it stays in the player's colour.
            draw_poly_lines(p.pos.x, p.pos.y, 6, PLAYER_RADIUS + 1.0, turn, 1.0, Color::new(0.4 * c.r + 0.6, 0.4 * c.g + 0.6, 0.4 * c.b + 0.6, 1.0));
            draw_poly(p.pos.x, p.pos.y, 6, PLAYER_RADIUS * 0.6, turn, c);
            draw_circle(p.pos.x, p.pos.y, PLAYER_RADIUS * 0.25, WHITE);
            // A filled wedge past the body says which way you are facing.
            let dir = vec2(p.aim.cos(), p.aim.sin());
            let (base, tip, side) = (p.pos + dir * (PLAYER_RADIUS + 3.0), p.pos + dir * (PLAYER_RADIUS + 13.0), vec2(-dir.y, dir.x) * 5.0);
            draw_triangle(base + side, base - side, tip, c);
        }
        if p.confused > 0.0 {
            // Stars circling the head, the cartoon sign for "dizzy".
            for k in 0..3 {
                let a = t * 6.0 + k as f32 * std::f32::consts::TAU / 3.0;
                let at = p.pos + vec2(a.cos() * 13.0, -11.0 + a.sin() * 4.0);
                draw_poly(at.x, at.y, 4, 3.0, 45.0, Color::new(1.0, 0.9, 0.3, 1.0));
            }
        }

        let w = 26.0;
        let (x, y) = (p.pos.x - w / 2.0, p.pos.y - p.hit_radius() - 9.0);
        draw_line(x, y, x + w, y, 3.0, Color::new(c.r, c.g, c.b, 0.2));
        draw_line(x, y, x + w * p.hp / MAX_HP, y, 3.0, c);
    }
}

/// A player turned into a sheep: a cloud of wool, a dark face where they look,
/// and a ring in their own colour so you still know whose it is.
fn draw_sheep(p: &sim::Player, c: Color, t: f32) {
    let dir = vec2(p.aim.cos(), p.aim.sin());
    // The wool bobs a little, faster when it runs.
    let bob = (t * 14.0).sin() * (p.vel.length() / 330.0).min(1.0);
    for k in 0..6 {
        let a = k as f32 * std::f32::consts::TAU / 6.0;
        draw_circle(p.pos.x + a.cos() * 4.0, p.pos.y + a.sin() * 4.0 + bob, 4.6, WHITE);
    }
    draw_circle(p.pos.x, p.pos.y + bob, 4.6, WHITE);
    let head = p.pos + dir * 8.0;
    draw_circle(head.x, head.y, 3.0, Color::new(0.12, 0.1, 0.12, 1.0));
    draw_circle_lines(p.pos.x, p.pos.y + bob, 9.5, 1.5, c);
}

pub fn center_text(text: &str, y: f32, size: f32, color: Color) {
    let dim = measure_text(text, None, size as u16, 1.0);
    draw_text(text, (sim::TILE * sim::MAP_W as f32 - dim.width) / 2.0, y, size, color);
}

/// `center_text` for picture coordinates (HUD, plates, armory): centred on the
/// picture's real width, and shrunk if it would not fit on a narrow screen.
pub fn center_wide(text: &str, y: f32, size: f32, color: Color) {
    let width = crate::view::layout().size.x;
    let measured = measure_text(text, None, size as u16, 1.0).width;
    let size = if measured > width - 24.0 { size * (width - 24.0) / measured } else { size };
    let dim = measure_text(text, None, size as u16, 1.0);
    draw_text(text, (width - dim.width) / 2.0, y, size, color);
}

/// A button in picture coordinates; returns true on the frame it is clicked or
/// tapped (macroquad turns a tap into a mouse click). Drawing and handling in
/// one call is "immediate-mode UI": no button objects to keep in sync with the
/// screen, the code that draws a button is the code that reacts to it.
pub fn button(r: Rect, label: &str, enabled: bool, accent: Color) -> bool {
    let mouse = crate::view::screen_to_picture(Vec2::from(mouse_position()));
    let hover = enabled && r.contains(mouse);
    let bg = if hover { Color::new(0.11, 0.12, 0.145, 1.0) } else { Color::new(0.075, 0.08, 0.095, 1.0) };
    let fg = if enabled { accent } else { Color::new(0.3, 0.31, 0.35, 1.0) };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, fg);
    let size = (r.h * 0.5).min(26.0);
    let dim = measure_text(label, None, size as u16, 1.0);
    draw_text(label, r.x + (r.w - dim.width) / 2.0, r.y + (r.h + dim.offset_y) / 2.0, size, fg);
    enabled && crate::touch::clicked_in(r)
}
