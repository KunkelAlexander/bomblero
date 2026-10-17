// Short-lived visual flourishes: beam flashes, explosion rings, hit pulses.
//
// This state is purely cosmetic. It is NOT part of the `World`, it is never
// networked, and the sim does not know it exists. It is built entirely from
// the sim's `Event`s.
//
// Pattern: Event Queue (consumer side). The sim writes events; this module
// reads them and decides what they look like. M3 adds particles, screen shake
// and sound as further consumers of the exact same events.

use std::collections::HashMap;

use macroquad::prelude::*;
use sim::{Event, Tile};

use crate::draw::player_color;
use crate::terrain;

struct Beam {
    from: Vec2,
    to: Vec2,
    age: f32,
}

struct Ring {
    pos: Vec2,
    max_radius: f32,
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
    shards: Vec<Shard>,
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
            Event::Beam { from, to } => self.beams.push(Beam { from, to, age: 0.0 }),
            Event::Explosion { pos, radius } => {
                self.rings.push(Ring { pos, max_radius: radius, age: 0.0, life: 0.4, color: ORANGE })
            }
            Event::Hit { pos, .. } => {
                self.rings.push(Ring { pos, max_radius: 20.0, age: 0.0, life: 0.2, color: WHITE })
            }
            Event::Died { victim, pos, .. } => self.rings.push(Ring {
                pos,
                max_radius: 60.0,
                age: 0.0,
                life: 0.6,
                color: player_color(victim as usize),
            }),
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

    pub fn update(&mut self, dt: f32) {
        for b in &mut self.beams {
            b.age += dt;
        }
        for r in &mut self.rings {
            r.age += dt;
        }
        for sh in &mut self.shards {
            sh.age += dt;
            sh.pos += sh.vel * dt;
            sh.vel *= 1.0 - 4.0 * dt; // air drag: shards burst out, then settle
            sh.angle += sh.spin * dt;
        }
        self.shards.retain(|sh| sh.age < SHARD_LIFE);
        self.beams.retain(|b| b.age < BEAM_LIFE);
        self.rings.retain(|r| r.age < r.life);
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
            draw_line(b.from.x, b.from.y, b.to.x, b.to.y, 1.0 + 5.0 * fade, Color::new(0.6, 0.9, 1.0, fade));
            draw_line(b.from.x, b.from.y, b.to.x, b.to.y, 1.5, Color::new(1.0, 1.0, 1.0, fade));
        }
        for r in &self.rings {
            let k = r.age / r.life;
            let c = Color::new(r.color.r, r.color.g, r.color.b, 1.0 - k);
            // Ease-out so the ring snaps outward and then slows.
            draw_circle_lines(r.pos.x, r.pos.y, r.max_radius * (1.0 - (1.0 - k) * (1.0 - k)), 3.0, c);
        }
    }
}
