// How the arena looks: a dark cave. Shape, not colour, tells you what a rocket
// will do.
//
//   * Walls (indestructible) are polished black stone: fused masses with soft,
//     slightly irregular outlines, like water-worn rock. They are nearly black
//     on their own; what you see of them is the *reflections* of lights moving
//     past (see `lights.rs`, which uses the `shine` texture baked here).
//   * The floor is mottled stone, darker where it meets a wall (a contact
//     shadow), so the walls look like they stand on it.
//   * Bricks (destructible) are loose pebbles of the same polished stone, one
//     per tile, a shade paler than the walls and cut into three facets. They
//     rest on the floor in a soft shadow and glint towards the nearest light
//     just like the walls do. The facets are the break lines: each hit knocks
//     one off.
//
// Colour is saved for players and weapons, so they stand out against grey.
//
// Pattern: Dirty Flag. Walls never change during a match, so the whole ground
// is baked once into textures and redrawn only if the wall layout differs from
// the one we baked. Bricks change all the time, but each is a few triangles.
//
// Like everything in the client this only *reads* the map. The sim still
// collides against the square tile grid; the rounded, wobbly look is cosmetic,
// which is why the wobble is kept to a few pixels (a bolt should never hit
// "empty" air by much).

use macroquad::prelude::*;
use sim::tilemap::Tilemap;
use sim::{Tile, MAP_H, MAP_W, TILE};

use crate::ink::Ink;
use crate::lights::{Light, LIFT};

const W: usize = MAP_W * TILE as usize;
const H: usize = MAP_H * TILE as usize;

/// Everything here is painted `LIFT` times brighter than it should look unlit,
/// because the lightmap multiplies it back down (see lights.rs).
const FLOOR: Color = Color::new(0.14, 0.14, 0.16, 1.0);

/// Blur radius in pixels for the fused walls. This is also the corner rounding.
const BLUR: usize = 5;

/// How far the wall outline may wander, as a fraction of the blurred edge. The
/// edge ramps over ~22 px, so 0.5 here moves the outline by about +-3 px.
const WOBBLE: f32 = 0.55;

#[derive(Default)]
pub struct Terrain {
    /// The wall layout the textures were baked from (the Dirty Flag's "last seen").
    solid: Vec<bool>,
    /// Floor and walls, opaque.
    ground: Option<Texture2D>,
    /// Which way each wall pixel faces (see `bake`), for the gloss pass.
    shine: Option<Texture2D>,
}

impl Terrain {
    pub fn update(&mut self, map: &Tilemap) {
        let solid: Vec<bool> = map.tiles().iter().map(|t| *t == Tile::Solid).collect();
        if self.ground.is_none() || solid != self.solid {
            let (ground, shine) = bake(&solid);
            self.ground = Some(ground);
            self.shine = Some(shine);
            self.solid = solid;
        }
    }

    pub fn shine(&self) -> Option<&Texture2D> {
        self.shine.as_ref()
    }

    pub fn draw(&self, map: &Tilemap, lights: &[Light], ink: &Ink) {
        // The ground is painted a lit grey and the lightmap darkens it (lights.rs):
        // a black floor would stay black under any light.
        if let Some(ground) = &self.ground {
            draw_texture(ground, 0.0, 0.0, WHITE);
        }
        // Ink stains the floor, so it goes on before the bricks rest on it.
        ink.draw();
        // Shadows first, for all bricks, so a brick's shadow never lands on top
        // of its neighbour.
        for_each_brick(map, |tx, ty, _| draw_shadow(tx, ty));
        for_each_brick(map, |tx, ty, hp| draw_pebble(tx, ty, hp, lights));
    }
}

// ---- ground: blur the wall mask, wobble it, then paint floor and walls ------

/// Smooth random values: random numbers on a grid of `1` unit, blended between
/// grid points. Scale the coordinates down for big soft patches, up for fine
/// detail. (Value noise: no library needed.)
fn smooth_noise(x: f32, y: f32, salt: i32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let at = |dx: i32, dy: i32| hash(ix as i32 + dx, iy as i32 + dy, salt);
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * u;
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * u;
    top + (bottom - top) * v
}

/// Returns (ground, shine).
///   * ground: the painted floor and walls.
///   * shine: for each wall pixel, which way its surface leans (red/green, 0.5 =
///     flat) and how much of the pixel is wall (alpha). The gloss pass turns that
///     into highlights: it is a "normal map", the usual trick for making a flat
///     picture react to light.
fn bake(solid: &[bool]) -> (Texture2D, Texture2D) {
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
    box_blur(&mut height, BLUR);
    box_blur(&mut height, BLUR);

    // 3. Blur it further, into a wide soft shadow for the floor to darken by.
    let mut shadow = height.clone();
    box_blur(&mut shadow, 7);
    box_blur(&mut shadow, 7);

    // 4. Wobble the outline with noise, so the walls are not ruler-straight.
    //    Only pixels near an edge can cross the one-half line, so the interior
    //    of a wall and open floor are not affected.
    for y in 0..H {
        for x in 0..W {
            let (fx, fy) = (x as f32, y as f32);
            let n = smooth_noise(fx / 26.0, fy / 26.0, 3) * 0.7 + smooth_noise(fx / 12.0, fy / 12.0, 4) * 0.3;
            height[y * W + x] += (n - 0.5) * WOBBLE;
        }
    }

    let mut ground = vec![0u8; W * H * 4];
    let mut shine = vec![0u8; W * H * 4];
    for y in 0..H {
        for x in 0..W {
            let at = |dx: i32, dy: i32| {
                let (px, py) = ((x as i32 + dx).clamp(0, W as i32 - 1), (y as i32 + dy).clamp(0, H as i32 - 1));
                height[py as usize * W + px as usize]
            };
            let h = at(0, 0);
            let (gx, gy) = (at(1, 0) - at(-1, 0), at(0, 1) - at(0, -1));
            let slope = gx + gy; // > 0: the surface faces up and to the left
            let inside = ((h - 0.5) * 7.0 + 0.5).clamp(0.0, 1.0); // ~1 px soft edge

            let i = (y * W + x) * 4;
            let (fx, fy) = (x as f32, y as f32);

            // The floor: big soft patches, finer grain, a slow drift between
            // cool and warm, a slight green where the patches are strongest, a
            // dark corner vignette, and the contact shadow next to walls.
            let patch = smooth_noise(fx / 90.0, fy / 90.0, 11) * 0.6 + smooth_noise(fx / 30.0, fy / 30.0, 12) * 0.4;
            let grain = hash(x as i32, y as i32, 13) - 0.5;
            let warm = smooth_noise(fx / 140.0, fy / 140.0, 14);
            let moss = (patch - 0.6).max(0.0) * 0.5;
            let edge = ((fx / W as f32 - 0.5).abs().max((fy / H as f32 - 0.5).abs()) * 2.0).powi(3);
            let contact = (shadow[y * W + x] * 1.6).clamp(0.0, 1.0);
            let shade = (0.45 + 1.1 * patch) * (1.0 + grain * 0.14) * (1.0 - 0.4 * edge) * (1.0 - 0.7 * contact);
            let floor = [
                FLOOR.r * shade * (0.9 + 0.2 * warm),
                (FLOOR.g + moss * 0.1) * shade,
                FLOOR.b * shade * (1.1 - 0.2 * warm) * (1.0 - moss),
            ];

            // The wall: nearly black, with a cold bevel facing the top-left
            // and a thin pale rim all round, so it has a shape before any
            // light reaches it. The rim does the work on a dim screen: two
            // dark greys cannot differ much, so what tells wall from floor is
            // a clearly lighter edge between them (the same idea as WCAG's
            // rule that the outline of anything you must see has a 3:1
            // contrast with what is next to it).
            let rim = if h > 0.5 && h < 0.62 { 0.2 } else { 0.0 };
            let wall_grey = ((0.03 + slope.max(0.0) * 0.6 + rim) * LIFT).clamp(0.02, 0.5);
            let wall = [wall_grey, wall_grey * 1.05, wall_grey * 1.25];

            // A faint dot at the centre of each floor tile, so you can still
            // count tiles for Bomberman-style trapping without a noisy grid.
            let dot = if inside == 0.0 && x % TILE as usize == 16 && y % TILE as usize == 16 { 0.35 } else { 0.0 };
            for c in 0..3 {
                let floor_c = floor[c] + (0.3 - floor[c]) * dot;
                let v = floor_c + (wall[c] - floor_c) * inside;
                ground[i + c] = (v.clamp(0.0, 1.0) * 255.0) as u8;
            }
            ground[i + 3] = 255;

            // Surface direction: it points away from the wall, i.e. against the
            // way the height grows. Scaled so the usual range fits one byte.
            shine[i] = ((0.5 - gx * 2.5).clamp(0.0, 1.0) * 255.0) as u8;
            shine[i + 1] = ((0.5 - gy * 2.5).clamp(0.0, 1.0) * 255.0) as u8;
            shine[i + 2] = 128;
            shine[i + 3] = (inside * 255.0) as u8;
        }
    }
    let upload = |bytes| {
        let texture = Texture2D::from_image(&Image { width: W as u16, height: H as u16, bytes });
        texture.set_filter(FilterMode::Linear);
        texture
    };
    (upload(ground), upload(shine))
}

/// Average each pixel with its neighbours within `radius` pixels: first along
/// rows, then along columns (two cheap passes instead of one big square).
/// Pixels past the edge repeat the edge pixel, so the border wall stays solid.
///
/// A running sum: moving one pixel along adds the pixel that enters the window
/// and subtracts the one that leaves, so the cost per pixel does not grow with
/// the radius. (The baking runs once per map, but in the browser the naive
/// version froze the game for about half a second.)
fn box_blur(v: &mut [f32], radius: usize) {
    let n = (2 * radius + 1) as f32;
    let r = radius as i32;
    let mut tmp = vec![0.0f32; v.len()];
    for y in 0..H {
        let row = &v[y * W..(y + 1) * W];
        let at = |x: i32| row[x.clamp(0, W as i32 - 1) as usize];
        let mut sum: f32 = (-r..=r).map(at).sum();
        for x in 0..W as i32 {
            tmp[y * W + x as usize] = sum / n;
            sum += at(x + r + 1) - at(x - r);
        }
    }
    // Down the columns, one whole row at a time (friendlier to the cache than
    // walking a column): `sums[x]` is the running sum of column x.
    let row_at = |y: i32| y.clamp(0, H as i32 - 1) as usize * W;
    let mut sums = vec![0.0f32; W];
    for d in -r..=r {
        let from = row_at(d);
        for x in 0..W {
            sums[x] += tmp[from + x];
        }
    }
    for y in 0..H as i32 {
        for x in 0..W {
            v[y as usize * W + x] = sums[x] / n;
        }
        let (enter, leave) = (row_at(y + r + 1), row_at(y - r));
        for x in 0..W {
            sums[x] += tmp[enter + x] - tmp[leave + x];
        }
    }
}

// ---- bricks: faceted pebbles ------------------------------------------------

fn for_each_brick(map: &Tilemap, mut f: impl FnMut(i32, i32, u8)) {
    for ty in 0..MAP_H as i32 {
        for tx in 0..MAP_W as i32 {
            if let Tile::Brick(hp) = map.get(tx, ty) {
                f(tx, ty, hp);
            }
        }
    }
}

const OUTLINE: usize = 18; // points around a pebble
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
        let n = 3.0;
        let r = 14.0 / (a.cos().abs().powf(n) + a.sin().abs().powf(n)).powf(1.0 / n);
        *point = vec2(a.cos(), a.sin()) * r * (0.93 + 0.07 * hash(tx, ty, 1 + i as i32));
    }
    points[OUTLINE] = points[0];
    Pebble { points, grey: 0.29 + 0.06 * hash(tx, ty, 99), turn }
}

/// Angle at the middle of wedge `k`.
fn wedge_angle(p: &Pebble, k: usize) -> f32 {
    p.turn + (k * PER_FACET + PER_FACET / 2) as f32 * std::f32::consts::TAU / OUTLINE as f32
}

/// Shade of wedge `k`: wedges that face the (imagined) top left are a little
/// lighter, so the pebble reads as cut stone, not a flat blob. Dark slate with a
/// blue cast: the same polished stone as the walls, one step paler.
fn wedge_color(p: &Pebble, k: usize) -> Color {
    let g = p.grey + 0.05 * (wedge_angle(p, k) - (-2.36f32)).cos();
    Color::new(g, g * 1.04, g * 1.14, 1.0)
}

/// A soft contact shadow, so the pebble rests on the floor like the walls
/// stand on it. A few stacked translucent discs fake the blur; the offset says
/// the (imagined) light is top left.
fn draw_shadow(tx: i32, ty: i32) {
    let c = vec2(tx as f32 + 0.5, ty as f32 + 0.5) * TILE + vec2(2.0, 3.0);
    for (radius, alpha) in [(22.0, 0.10), (19.0, 0.12), (16.0, 0.16)] {
        draw_poly(c.x, c.y, 16, radius, 0.0, Color::new(0.0, 0.0, 0.0, alpha));
    }
}

/// The light that matters most to this pebble: strongest after distance
/// falloff. Returns it with how much of its reach is left at the pebble.
fn strongest_light(c: Vec2, lights: &[Light]) -> Option<(&Light, f32)> {
    lights
        .iter()
        .map(|l| (l, l.strength * (1.0 - c.distance(l.pos) / l.radius)))
        .filter(|(_, reach)| *reach > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// A brick with `hp` hit points left has `hp` wedges. The wedges knocked off
/// first are the highest numbered, and `Effects` flies them away as shards.
fn draw_pebble(tx: i32, ty: i32, hp: u8, lights: &[Light]) {
    let p = pebble(tx, ty);
    let c = vec2(tx as f32 + 0.5, ty as f32 + 0.5) * TILE;
    let glint = strongest_light(c, lights);
    for k in 0..(hp as usize).min(FACETS) {
        // Domed: each facet is brightest in the middle and falls off towards the
        // rim, which is what turns a flat grey shape into a polished pebble and
        // gives it the same soft edge the walls have. The gradient is free: the
        // GPU blends between vertex colours (one mesh per facet, a triangle fan).
        let w = wedge_color(&p, k);
        let shade = |m: f32| Color::new(w.r * LIFT * m, w.g * LIFT * m, w.b * LIFT * m, 1.0);
        let mut vertices = vec![Vertex::new(c.x, c.y, 0.0, 0.0, 0.0, shade(1.35))];
        for point in &p.points[k * PER_FACET..=(k + 1) * PER_FACET] {
            vertices.push(Vertex::new(c.x + point.x, c.y + point.y, 0.0, 0.0, 0.0, shade(0.55)));
        }
        let indices = (0..PER_FACET as u16).flat_map(|i| [0, i + 1, i + 2]).collect();
        draw_mesh(&Mesh { vertices, indices, texture: None });
        // The seam: where the next hit will split it.
        let s = c + p.points[k * PER_FACET];
        draw_line(c.x, c.y, s.x, s.y, 1.2, Color::new(0.02, 0.02, 0.03, 0.75));

        // Polish: a facet turned towards a light catches a bright rim in that
        // light's colour, plus a tiny hot spot. Facing away, it stays dark.
        // (Painted into the lit layer, so the lightmap also hides it where a
        // wall shadows the light.)
        if let Some((light, reach)) = glint {
            let to_light = light.pos - c;
            let facing = (wedge_angle(&p, k) - to_light.y.atan2(to_light.x)).cos().max(0.0).powi(3);
            let strength = facing * reach.min(1.0);
            if strength > 0.02 {
                let edge = &p.points[k * PER_FACET..=(k + 1) * PER_FACET];
                let rim = Color::new(light.color.r, light.color.g, light.color.b, (strength * 1.5).min(1.0));
                for pair in edge.windows(2) {
                    draw_line(c.x + pair[0].x, c.y + pair[0].y, c.x + pair[1].x, c.y + pair[1].y, 2.0, rim);
                }
                let spot = c + edge[PER_FACET / 2] * 0.8;
                draw_circle(spot.x, spot.y, 2.0, Color::new(1.0, 1.0, 1.0, (strength * 1.2).min(1.0)));
            }
        }
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
