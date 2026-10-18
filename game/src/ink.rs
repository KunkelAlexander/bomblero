// Ink: the splatter a hit leaves on the floor, in the victim's colour, which
// stays for the rest of the match (Liero's blood, in neon). The floor becomes a
// map of where the fights were.
//
// Cost: splats are painted ONCE, when they happen, into a texture the size of
// the arena. Every frame after that, all the ink there has ever been is a
// single textured rectangle, so a thousand splats cost the same as one.
//
// Networking note: this is cosmetic, so it lives only here in the client. It is
// not in the sim and nothing about it is sent. Each machine paints its own
// splats from the `Hit` and `Died` events it already receives, with its own
// random droplets, so two players see slightly different stains. Nobody can
// tell, and nobody needs to agree: only things that change the game must be
// the same everywhere. (Pattern: Event Queue, one more consumer.)

use macroquad::prelude::*;
use sim::{Event, Tile, World, MAP_H, MAP_W, TILE};

use crate::draw::player_color;

const MAP_PX: Vec2 = vec2(MAP_W as f32 * TILE, MAP_H as f32 * TILE);

/// How strongly the player colour shows. The floor is lit by the lightmap like
/// the stone, so this is paint, not light: dark in the dark, vivid near a lamp.
/// Kept low enough that ink in normal light stays under the glow threshold
/// (view.rs): it should read as paint, not as another light.
const SHADE: f32 = 0.6;
const OPACITY: f32 = 0.95;

struct Splat {
    pos: Vec2,
    /// Overall size in px: the main blob's radius, droplets fly up to 3x out.
    size: f32,
    droplets: usize,
    color: Color,
}

#[derive(Default)]
pub struct Ink {
    /// Created on the first frame: it needs the graphics context.
    canvas: Option<RenderTarget>,
    /// Splats seen since the last frame, waiting to be painted.
    pending: Vec<Splat>,
    /// The sim tick at the last paint. A new match (R) starts again at tick 0,
    /// which is how we notice that the old match's ink must go: the same test
    /// works for the host and for joiners, with no message for it.
    tick: u32,
}

impl Ink {
    pub fn on_event(&mut self, event: &Event) {
        let (pos, victim, size, droplets) = match *event {
            // A bigger hit leaves a bigger mark: a blaster bolt (12) a dab, a
            // railgun (70) a real splash.
            Event::Hit { victim, pos, damage } => (pos, victim, 3.0 + damage * 0.06, 3 + (damage / 8.0) as usize),
            Event::Died { victim, pos, .. } => (pos, victim, 11.0, 16),
            _ => return,
        };
        let c = player_color(victim as usize);
        let color = Color::new(c.r * SHADE, c.g * SHADE, c.b * SHADE, 1.0);
        self.pending.push(Splat { pos, size, droplets, color });
    }

    /// Paint what happened since last frame into the canvas. Call before
    /// `Glow::begin`, which takes the camera back to the scene.
    pub fn paint(&mut self, world: &World) {
        let fresh = self.canvas.is_none() || world.tick < self.tick;
        self.tick = world.tick;
        if !fresh && self.pending.is_empty() {
            return;
        }
        let canvas = self.canvas.get_or_insert_with(|| {
            let target = render_target(MAP_PX.x as u32, MAP_PX.y as u32);
            target.texture.set_filter(FilterMode::Linear);
            target
        });
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, MAP_PX.x, MAP_PX.y));
        cam.render_target = Some(canvas.clone());
        set_camera(&cam);
        if fresh {
            clear_background(Color::new(0.0, 0.0, 0.0, 0.0)); // no ink yet
        }
        let map = &world.map;
        // Every blob is opaque in the canvas, and the canvas as a whole is
        // drawn see-through (`OPACITY`). Overlapping splats then do not pile up
        // into solid paint, and no alpha arithmetic is needed in here.
        for s in self.pending.drain(..) {
            let on_floor = |p: Vec2| map.tile_at(p) != Tile::Solid; // no paint on the black walls
            if on_floor(s.pos) {
                draw_circle(s.pos.x, s.pos.y, s.size, s.color);
            }
            for _ in 0..s.droplets {
                let a = rand::gen_range(0.0, std::f32::consts::TAU);
                // Most droplets land close, a few fly far: squaring a random
                // number bunches it towards zero.
                let r = s.size * (0.6 + 2.4 * rand::gen_range(0.0f32, 1.0).powi(2));
                let p = s.pos + vec2(a.cos(), a.sin()) * r;
                if on_floor(p) {
                    draw_circle(p.x, p.y, rand::gen_range(0.15, 0.45) * s.size, s.color);
                }
            }
        }
    }

    /// Lay the ink on the ground. Call after the ground and before the bricks,
    /// so a brick hides the ink beneath it and the lightmap lights both.
    pub fn draw(&self) {
        if let Some(canvas) = &self.canvas {
            draw_texture_ex(
                &canvas.texture,
                0.0,
                0.0,
                Color::new(1.0, 1.0, 1.0, OPACITY),
                DrawTextureParams {
                    dest_size: Some(MAP_PX),
                    // Render targets are stored upside down (see `Glow::present`).
                    flip_y: true,
                    ..Default::default()
                },
            );
        }
    }
}
