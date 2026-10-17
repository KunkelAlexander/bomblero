// Short-lived visual flourishes: beam flashes, explosion rings, hit pulses.
//
// This state is purely cosmetic. It is NOT part of the `World`, it is never
// networked, and the sim does not know it exists. It is built entirely from
// the sim's `Event`s.
//
// Pattern: Event Queue (consumer side). The sim writes events; this module
// reads them and decides what they look like. M3 adds particles, screen shake
// and sound as further consumers of the exact same events.

use macroquad::prelude::*;
use sim::Event;

use crate::draw::player_color;

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

#[derive(Default)]
pub struct Effects {
    beams: Vec<Beam>,
    rings: Vec<Ring>,
}

const BEAM_LIFE: f32 = 0.25;

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
        self.beams.retain(|b| b.age < BEAM_LIFE);
        self.rings.retain(|r| r.age < r.life);
    }

    pub fn draw(&self) {
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
