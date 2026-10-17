// How the arena looks: a greyscale world where shape, not colour, tells you what
// a rocket will do.
//
//   * Walls (indestructible) are dark, soft, *fused* masses: neighbouring wall
//     tiles melt into one continuous shape with rounded corners.
//   * Bricks (destructible) are lighter, loose pebbles, one per tile, cut into
//     three facets. The facets are the break lines: each hit knocks one off.
//
// Colour is saved for players and weapons, so they stand out against grey.
//
// Pattern: Dirty Flag. Walls never change during a match, so the fused look is
// baked once into a texture and redrawn only if the wall layout differs from
// the one we baked. Bricks change all the time, but each is a few triangles.
//
// Like everything in the client this only *reads* the map. The sim still
// collides against the square tile grid; the rounded look is cosmetic, which is
// why the rounding is kept small (a bolt should never hit "empty" air by much).

use macroquad::prelude::*;
use sim::tilemap::Tilemap;
use sim::{Tile, MAP_H, MAP_W, TILE};

const W: usize = MAP_W * TILE as usize;
const H: usize = MAP_H * TILE as usize;

/// Blur radius in pixels for the fused walls. This is also the corner rounding.
const BLUR: usize = 5;

#[derive(Default)]
pub struct Terrain {
    /// The wall layout the texture was baked from (the Dirty Flag's "last seen").
    solid: Vec<bool>,
    walls: Option<Texture2D>,
}

impl Terrain {
    pub fn update(&mut self, map: &Tilemap) {
        let solid: Vec<bool> = map.tiles().iter().map(|t| *t == Tile::Solid).collect();
        if self.walls.is_none() || solid != self.solid {
            self.walls = Some(bake_walls(&solid));
            self.solid = solid;
        }
    }

    pub fn draw(&self, map: &Tilemap) {
        if let Some(walls) = &self.walls {
            draw_texture(walls, 0.0, 0.0, WHITE);
        }
        for ty in 0..MAP_H as i32 {
            for tx in 0..MAP_W as i32 {
                if let Tile::Brick(hp) = map.get(tx, ty) {
                    draw_pebble(tx, ty, hp);
                }
            }
        }
    }
}

// ---- walls: blur the wall mask, then cut and light it -----------------------

fn bake_walls(solid: &[bool]) -> Texture2D {
    // 1. A mask the size of the screen: 1.0 inside a wall tile, 0.0 elsewhere.
    let mut height = vec![0.0f32; W * H];
    for y in 0..H {
        for x in 0..W {
            let (tx, ty) = (x / TILE as usize, y / TILE as usize);
            height[y * W + x] = if solid[ty * MAP_W + tx] { 1.0 } else { 0.0 };
        }
    }

    // 2. Blur it twice. A straight edge stays exactly where it was (the blur
    //    is 0.5 right on it), but a convex corner loses a little and a concave
    //    one gains a little: that is what rounds and fuses the shapes.
    box_blur(&mut height);
    box_blur(&mut height);

    // 3. Read the blurred mask two ways: "is this pixel in the wall?" (above
    //    one half) and "which way does the surface slope?" (its gradient), which
    //    gives a soft bevel lit from the top left.
    let mut bytes = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let at = |dx: i32, dy: i32| {
                let (px, py) = ((x as i32 + dx).clamp(0, W as i32 - 1), (y as i32 + dy).clamp(0, H as i32 - 1));
                height[py as usize * W + px as usize]
            };
            let h = at(0, 0);
            let slope = (at(1, 0) - at(-1, 0)) + (at(0, 1) - at(0, -1));
            let inside = ((h - 0.5) * 7.0 + 0.5).clamp(0.0, 1.0); // ~1 px soft edge
            let rim = if h > 0.5 && h < 0.62 { 0.04 } else { 0.0 }; // thin lit edge
            let grey = (0.07 + slope * 0.9 + rim).clamp(0.02, 0.26);

            let i = (y * W + x) * 4;
            if inside > 0.0 {
                let v = (grey * 255.0) as u8;
                bytes[i..i + 4].copy_from_slice(&[v, v, (v as f32 * 1.06) as u8, (inside * 255.0) as u8]);
            } else if x % TILE as usize == 16 && y % TILE as usize == 16 {
                // A faint dot at the centre of each floor tile, so you can still
                // count tiles for Bomberman-style trapping without a noisy grid.
                bytes[i..i + 4].copy_from_slice(&[70, 70, 78, 110]);
            }
        }
    }
    let texture = Texture2D::from_image(&Image { width: W as u16, height: H as u16, bytes });
    texture.set_filter(FilterMode::Linear);
    texture
}

/// Average each pixel with its neighbours within `BLUR` pixels: first along
/// rows, then along columns (two cheap passes instead of one big square).
/// Pixels past the edge repeat the edge pixel, so the border wall stays solid.
fn box_blur(v: &mut [f32]) {
    let n = (2 * BLUR + 1) as f32;
    let mut tmp = vec![0.0f32; v.len()];
    for y in 0..H {
        for x in 0..W {
            let mut sum = 0.0;
            for d in -(BLUR as i32)..=BLUR as i32 {
                sum += v[y * W + (x as i32 + d).clamp(0, W as i32 - 1) as usize];
            }
            tmp[y * W + x] = sum / n;
        }
    }
    for y in 0..H {
        for x in 0..W {
            let mut sum = 0.0;
            for d in -(BLUR as i32)..=BLUR as i32 {
                sum += tmp[(y as i32 + d).clamp(0, H as i32 - 1) as usize * W + x];
            }
            v[y * W + x] = sum / n;
        }
    }
}

// ---- bricks: faceted pebbles ------------------------------------------------

const OUTLINE: usize = 12; // points around a pebble
const FACETS: usize = 3; // wedges; one per brick hit point
const PER_FACET: usize = OUTLINE / FACETS;

/// A pebble's outline, relative to its tile centre, plus how light it is.
/// Pure function of the tile position: every client draws the same pebble
/// without being told anything (Pattern: Flyweight, the shape is derived, not
/// stored or sent).
struct Pebble {
    /// `OUTLINE` points, then the first again so a wedge can read past the end.
    points: [Vec2; OUTLINE + 1],
    grey: f32,
    /// Where wedge 0 starts, so the seams differ from tile to tile.
    turn: f32,
}

fn hash(tx: i32, ty: i32, salt: i32) -> f32 {
    let mut h = (tx as u32).wrapping_mul(73856093) ^ (ty as u32).wrapping_mul(19349663) ^ (salt as u32).wrapping_mul(83492791);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1e995);
    h ^= h >> 15;
    (h % 10_000) as f32 / 10_000.0
}

fn pebble(tx: i32, ty: i32) -> Pebble {
    let turn = hash(tx, ty, 0) * std::f32::consts::TAU / FACETS as f32;
    let mut points = [Vec2::ZERO; OUTLINE + 1];
    for (i, point) in points.iter_mut().enumerate().take(OUTLINE) {
        let a = turn + i as f32 * std::f32::consts::TAU / OUTLINE as f32;
        // A rounded square (a "squircle") fills the tile better than a circle,
        // and a little per-point wobble keeps the pebbles from looking stamped.
        let n = 3.5;
        let r = 14.0 / (a.cos().abs().powf(n) + a.sin().abs().powf(n)).powf(1.0 / n);
        *point = vec2(a.cos(), a.sin()) * r * (0.9 + 0.1 * hash(tx, ty, 1 + i as i32));
    }
    points[OUTLINE] = points[0];
    Pebble { points, grey: 0.34 + 0.06 * hash(tx, ty, 99), turn }
}

/// Angle at the middle of wedge `k`.
fn wedge_angle(p: &Pebble, k: usize) -> f32 {
    p.turn + (k * PER_FACET + PER_FACET / 2) as f32 * std::f32::consts::TAU / OUTLINE as f32
}

/// Shade of wedge `k`: wedges that face the light (top left) are lighter, so
/// the pebble reads as cut stone, not a flat blob.
fn wedge_color(p: &Pebble, k: usize) -> Color {
    let g = p.grey + 0.1 * (wedge_angle(p, k) - (-2.36f32)).cos();
    Color::new(g, g, g * 1.04, 1.0)
}

/// A brick with `hp` hit points left has `hp` wedges. The wedges knocked off
/// first are the highest numbered, and `Effects` flies them away as shards.
fn draw_pebble(tx: i32, ty: i32, hp: u8) {
    let p = pebble(tx, ty);
    let c = vec2(tx as f32 + 0.5, ty as f32 + 0.5) * TILE;
    for k in 0..(hp as usize).min(FACETS) {
        let color = wedge_color(&p, k);
        for i in k * PER_FACET..(k + 1) * PER_FACET {
            draw_triangle(c, c + p.points[i], c + p.points[i + 1], color);
        }
        // The seam: where the next hit will split it.
        let s = c + p.points[k * PER_FACET];
        draw_line(c.x, c.y, s.x, s.y, 1.5, Color::new(0.06, 0.06, 0.07, 1.0));
    }
}

/// One wedge of a pebble knocked loose: its corners relative to its own
/// centre, the colour it had, and the direction it flies.
pub struct Shard {
    pub corners: Vec<Vec2>,
    pub centre: Vec2,
    pub color: Color,
    pub dir: Vec2,
}

/// The pieces that fall off a brick when it goes from `from` to `to` hit points.
pub fn shards(tx: i32, ty: i32, from: u8, to: u8) -> Vec<Shard> {
    let p = pebble(tx, ty);
    let tile_centre = vec2(tx as f32 + 0.5, ty as f32 + 0.5) * TILE;
    (to as usize..(from as usize).min(FACETS))
        .map(|k| {
            let mut pts = vec![Vec2::ZERO];
            pts.extend(&p.points[k * PER_FACET..=(k + 1) * PER_FACET]);
            let mid = pts.iter().copied().sum::<Vec2>() / pts.len() as f32;
            Shard {
                corners: pts.iter().map(|q| *q - mid).collect(),
                centre: tile_centre + mid,
                color: wedge_color(&p, k),
                dir: vec2(wedge_angle(&p, k).cos(), wedge_angle(&p, k).sin()),
            }
        })
        .collect()
}
