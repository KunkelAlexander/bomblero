// Short-lived visual flourishes: beams, shockwaves, sparks, flying brick shards.
//
// This state is purely cosmetic. It is NOT part of the `World`, it is never
// networked, and the sim does not know it exists. It is built entirely from
// the sim's `Event`s.
//
// Pattern: Event Queue (consumer side). The sim writes events; this module
// reads them and decides what they look like. M3 adds screen shake and sound
// as further consumers of the exact same events.
//
// The game's visual language (draw.rs and terrain.rs follow it too):
//   * The WORLD is soft, grey and rounded: stone you can read by shape.
//   * ACTORS are crisp and angular: diamonds, wedges, needles, hexagons.
//   * COLOUR means "who": everything a player does (bolts, muzzle flash, the
//     sparks when they are hit, their death burst) is in their colour.
//   * ORANGE-WHITE means "danger": rockets and explosions, for everybody.
//   * Light is the only thing that glows, and it falls on the stone: coloured
//     light around players, projectiles and explosions (lights.rs).

use std::collections::HashMap;

use macroquad::prelude::*;
use sim::{Event, Tile};

use crate::draw::{player_color, DANGER};
use crate::lights::Light;
use crate::terrain;

struct Beam {
    from: Vec2,
    to: Vec2,
    age: f32,
    color: Color,
    /// Thickness at the start; it fades with age.
    width: f32,
}

/// A hexagonal shockwave (the game's "energy" shape), optionally lighting the
/// floor around it while it expands.
struct Ring {
    pos: Vec2,
    max_radius: f32,
    age: f32,
    life: f32,
    color: Color,
    light: f32,
}

/// A needle-shaped spark.
struct Spark {
    pos: Vec2,
    vel: Vec2,
    age: f32,
    life: f32,
    color: Color,
}

/// A wedge of brick flying away. See `terrain::shards`.
struct Shard {
    piece: terrain::Shard,
    pos: Vec2,
    vel: Vec2,
    angle: f32,
    spin: f32,
    age: f32,
}

#[derive(Default)]
pub struct Effects {
    beams: Vec<Beam>,
    rings: Vec<Ring>,
    sparks: Vec<Spark>,
    shards: Vec<Shard>,
    /// Who fired last. A `Beam` event carries no owner, but it always follows
    /// the `Shot` that made it, so this is whose colour the beam gets.
    last_shooter: u8,
    /// Hit points of bricks we have seen damaged. A tile change only says what
    /// the brick is *now*; remembering what it was tells us which wedges just
    /// came off. Bricks nobody has hurt yet are not in here (they have full hp).
    brick_hp: HashMap<(i32, i32), u8>,
}

const BEAM_LIFE: f32 = 0.25;
const SHARD_LIFE: f32 = 0.6;

impl Effects {
    pub fn on_event(&mut self, event: &Event) {
        match *event {
            Event::Shot { player, pos, .. } => {
                self.last_shooter = player;
                self.ring(pos, 12.0, 0.12, player_color(player as usize), 0.0);
            }
            Event::Beam { from, to } => {
                let color = player_color(self.last_shooter as usize);
                self.beams.push(Beam { from, to, age: 0.0, color, width: 5.0 });
            }
            Event::Explosion { pos, radius } => {
                self.ring(pos, radius, 0.4, DANGER, 0.6);
                // More sparks for a bigger blast: 14 for a rocket, 46 for a Nova.
                self.burst(pos, (radius * 0.2) as usize, 120.0..300.0, DANGER);
            }
            Event::Teleported { player, from, to } => {
                // A ring and a puff of sparks at both ends, in the jumper's colour.
                let color = player_color(player as usize);
                for at in [from, to] {
                    self.ring(at, 36.0, 0.35, color, 0.5);
                    self.burst(at, 10, 60.0..180.0, color);
                }
            }
            Event::Cross { pos, arms } => {
                // The plus is four fat beams, one per arm, so it is exactly as
                // long as the walls let the blast be.
                for (arm, dir) in [vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(-1.0, 0.0), vec2(0.0, -1.0)].into_iter().enumerate() {
                    self.beams.push(Beam { from: pos, to: pos + dir * arms[arm], age: 0.0, color: DANGER, width: 22.0 });
                    self.burst(pos + dir * arms[arm] * 0.5, 4, 40.0..140.0, DANGER);
                }
                self.ring(pos, 40.0, 0.4, DANGER, 0.9);
                self.burst(pos, 10, 100.0..260.0, DANGER);
            }
            Event::Hit { victim, pos, .. } => {
                self.ring(pos, 20.0, 0.2, WHITE, 0.0);
                self.burst(pos, 6, 60.0..160.0, player_color(victim as usize));
            }
            Event::Died { victim, pos, .. } => {
                let color = player_color(victim as usize);
                self.ring(pos, 60.0, 0.6, color, 0.5);
                self.burst(pos, 22, 80.0..260.0, color);
            }
            Event::TileChanged { x, y, tile } => {
                let now = match tile {
                    Tile::Brick(hp) => hp,
                    _ => 0,
                };
                let before = self.brick_hp.get(&(x, y)).copied().unwrap_or(sim::tilemap::BRICK_HP);
                if now == 0 {
                    self.brick_hp.remove(&(x, y));
                } else {
                    self.brick_hp.insert((x, y), now);
                }
                for piece in terrain::shards(x, y, before, now) {
                    // Each wedge flies the way it faces, with a bit of spread.
                    let speed = rand::gen_range(70.0, 150.0);
                    self.shards.push(Shard {
                        pos: piece.centre,
                        vel: piece.dir * speed,
                        angle: 0.0,
                        spin: rand::gen_range(-9.0, 9.0),
                        age: 0.0,
                        piece,
                    });
                }
            }
            _ => {}
        }
    }

    fn ring(&mut self, pos: Vec2, max_radius: f32, life: f32, color: Color, light: f32) {
        self.rings.push(Ring { pos, max_radius, age: 0.0, life, color, light });
    }

    /// `count` sparks flying out in all directions.
    fn burst(&mut self, pos: Vec2, count: usize, speed: std::ops::Range<f32>, color: Color) {
        for _ in 0..count {
            let a = rand::gen_range(0.0, std::f32::consts::TAU);
            let v = vec2(a.cos(), a.sin()) * rand::gen_range(speed.start, speed.end);
            self.sparks.push(Spark { pos, vel: v, age: 0.0, life: rand::gen_range(0.25, 0.55), color });
        }
    }

    pub fn update(&mut self, dt: f32) {
        for b in &mut self.beams {
            b.age += dt;
        }
        for r in &mut self.rings {
            r.age += dt;
        }
        for s in &mut self.sparks {
            s.age += dt;
            s.pos += s.vel * dt;
            s.vel *= 1.0 - 3.0 * dt;
        }
        for sh in &mut self.shards {
            sh.age += dt;
            sh.pos += sh.vel * dt;
            sh.vel *= 1.0 - 4.0 * dt; // air drag: shards burst out, then settle
            sh.angle += sh.spin * dt;
        }
        self.shards.retain(|sh| sh.age < SHARD_LIFE);
        self.sparks.retain(|s| s.age < s.life);
        self.beams.retain(|b| b.age < BEAM_LIFE);
        self.rings.retain(|r| r.age < r.life);
    }

    /// Flashes of light cast by explosions and deaths (see lights.rs).
    pub fn lights(&self, out: &mut Vec<Light>) {
        for r in self.rings.iter().filter(|r| r.light > 0.0) {
            let k = r.age / r.life;
            out.push(Light { pos: r.pos, radius: r.max_radius * 2.2, color: r.color, strength: r.light * 1.5 * (1.0 - k) });
        }
    }

    pub fn draw(&self) {
        for sh in &self.shards {
            let k = sh.age / SHARD_LIFE;
            let color = Color::new(sh.piece.color.r + 0.2, sh.piece.color.g + 0.2, sh.piece.color.b + 0.2, 1.0 - k);
            let (sin, cos) = sh.angle.sin_cos();
            let at = |p: Vec2| sh.pos + vec2(p.x * cos - p.y * sin, p.x * sin + p.y * cos) * (1.0 - 0.5 * k);
            for pair in sh.piece.corners[1..].windows(2) {
                let (a, b, c) = (at(sh.piece.corners[0]), at(pair[0]), at(pair[1]));
                draw_triangle(a, b, c, color);
            }
        }
        for b in &self.beams {
            let fade = 1.0 - b.age / BEAM_LIFE;
            let c = Color::new(b.color.r, b.color.g, b.color.b, fade);
            draw_line(b.from.x, b.from.y, b.to.x, b.to.y, 1.0 + b.width * fade, c);
            draw_line(b.from.x, b.from.y, b.to.x, b.to.y, 1.5, Color::new(1.0, 1.0, 1.0, fade));
        }
        for r in &self.rings {
            let k = r.age / r.life;
            // Ease-out so the ring snaps outward and then slows.
            let radius = r.max_radius * (1.0 - (1.0 - k) * (1.0 - k));
            draw_poly_lines(r.pos.x, r.pos.y, 6, radius, 0.0, 3.0, Color::new(r.color.r, r.color.g, r.color.b, 1.0 - k));
        }
        for s in &self.sparks {
            let k = s.age / s.life;
            let dir = s.vel.normalize_or_zero();
            let side = vec2(-dir.y, dir.x) * 1.4;
            let c = Color::new(s.color.r, s.color.g, s.color.b, 1.0 - k);
            draw_triangle(s.pos + dir * 6.0, s.pos + side, s.pos - side, c);
            draw_triangle(s.pos - dir * 3.0, s.pos + side, s.pos - side, c);
        }
    }
}
