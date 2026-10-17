// Lighting: the arena is dark and every light source paints into a "lightmap",
// which is then multiplied over the stone.
//
//   1. clear a small texture to the AMBIENT level (the unlit arena);
//   2. add every light to it: a smooth coloured gradient that stops at walls;
//   3. multiply the lightmap over the scene (stone only: actors are drawn after
//      this, so they stay bright, like things that make their own light);
//   4. add the glints the lights make on the polished black walls.
//
// Why a lightmap and not drawing glows on the stone: light *adds up* (two
// bolts crossing make a brighter spot), it can brighten the stone and not just
// tint it, and the same texture can later carry shadows from anything.
//
// Why it is cheap: the lightmap is a quarter of the screen size in each
// direction (16x fewer pixels). Bilinear filtering when it is stretched back up
// gives smooth gradients for free, and each light only touches the pixels
// inside its own radius.
//
// Like all client code this only *reads* the world: lights are derived from
// players, projectiles and effects each frame. The sim does not know about them.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use sim::tilemap::Tilemap;
use sim::{World, MAP_H, MAP_W, TILE};

use crate::draw::{player_color, DANGER};
use crate::shaders;
use crate::view::Glow;

/// Stone is painted `LIFT` times brighter than the look it was designed for,
/// and the lightmap's ambient level darkens it again. That way the arena can be
/// made darker or lighter by changing `AMBIENT` alone.
pub const LIFT: f32 = 0.5 / 0.3;

/// Lightmap level of the unlit arena. The multiply is 2x (see `apply`): at 0.3
/// the stone looks as designed, lower is darker, higher is lighter.
pub const AMBIENT: f32 = 0.14;

const MAP_PX: Vec2 = vec2(MAP_W as f32 * TILE, MAP_H as f32 * TILE);
const DOWNSCALE: f32 = 4.0;

#[derive(Clone, Copy)]
pub struct Light {
    pub pos: Vec2,
    pub radius: f32,
    pub color: Color,
    pub strength: f32,
}

/// The lights of the living things in the world: a pool around every player
/// and every projectile. Explosion flashes come from `Effects::lights`.
pub fn from_world(world: &World, out: &mut Vec<Light>) {
    for (i, p) in world.players.iter().enumerate().filter(|(_, p)| p.alive) {
        out.push(Light { pos: p.pos, radius: 120.0, color: player_color(i), strength: 0.75 });
    }
    for p in &world.projectiles {
        let (radius, color, strength) = match p.weapon {
            sim::weapons::ROCKET => (130.0, DANGER, 0.8),
            sim::weapons::BOMB => (70.0, DANGER, 0.5),
            sim::weapons::NOVA => (240.0, DANGER, 1.0),
            sim::weapons::CLUSTER => (80.0, DANGER, 0.6),
            // Eight pellets per shot would be eight lights: not worth it. The
            // same goes for the swarm's six missiles and the cluster's seven bomblets.
            sim::weapons::SCATTER | sim::weapons::SWARM | sim::weapons::BOMBLET => continue,
            _ => (85.0, player_color(p.owner as usize), 0.75),
        };
        out.push(Light { pos: p.pos, radius, color, strength });
    }
}

pub struct Lights {
    map: RenderTarget,
    /// One texel per tile: 1 where light cannot pass. Bricks block it too, so
    /// shooting a brick away lets the light through.
    mask: Texture2D,
    /// What `mask` currently holds (Pattern: Dirty Flag, upload only on change).
    mask_bytes: Vec<u8>,
    light: Material,
    /// The highlights on the black walls (see `GLOSS_FRAGMENT`).
    gloss: Material,
    multiply: Material,
}

impl Lights {
    pub fn new() -> Lights {
        let map = render_target((MAP_PX.x / DOWNSCALE) as u32, (MAP_PX.y / DOWNSCALE) as u32);
        map.texture.set_filter(FilterMode::Linear);

        let mask = Texture2D::from_image(&Image { width: MAP_W as u16, height: MAP_H as u16, bytes: vec![0; MAP_W * MAP_H * 4] });
        mask.set_filter(FilterMode::Nearest); // a tile is solid or it is not: no blending

        // Lights add to each other: result = light + what is already there.
        let add = BlendState::new(Equation::Add, BlendFactor::One, BlendFactor::One);
        let light = load_material(
            ShaderSource::Glsl { vertex: shaders::LIGHT_VERTEX, fragment: shaders::LIGHT_FRAGMENT },
            MaterialParams {
                pipeline_params: PipelineParams { color_blend: Some(add), ..Default::default() },
                uniforms: vec![
                    UniformDesc::new("light", UniformType::Float2),
                    UniformDesc::new("radius", UniformType::Float1),
                    UniformDesc::new("tint", UniformType::Float3),
                    UniformDesc::new("map_px", UniformType::Float2),
                ],
                textures: vec!["Mask".to_string()],
            },
        )
        .expect("light shader failed to compile");
        light.set_uniform("map_px", MAP_PX);
        light.set_texture("Mask", mask.clone());

        // The gloss pass adds like the lights do, but at full resolution.
        let gloss = load_material(
            ShaderSource::Glsl { vertex: shaders::LIGHT_VERTEX, fragment: shaders::GLOSS_FRAGMENT },
            MaterialParams {
                pipeline_params: PipelineParams { color_blend: Some(add), ..Default::default() },
                uniforms: vec![
                    UniformDesc::new("light", UniformType::Float2),
                    UniformDesc::new("radius", UniformType::Float1),
                    UniformDesc::new("tint", UniformType::Float3),
                    UniformDesc::new("map_px", UniformType::Float2),
                ],
                textures: vec!["Mask".to_string(), "Normals".to_string()],
            },
        )
        .expect("gloss shader failed to compile");
        gloss.set_uniform("map_px", MAP_PX);
        gloss.set_texture("Mask", mask.clone());

        // Scene * lightmap + lightmap * scene = 2 * scene * lightmap. That is
        // the "2x multiply": a lightmap of 0.5 leaves the scene as it is, more
        // than that brightens it, less darkens it.
        let double_multiply =
            BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::DestinationColor), BlendFactor::Value(BlendValue::SourceColor));
        let multiply = load_material(
            ShaderSource::Glsl { vertex: shaders::VERTEX, fragment: shaders::COPY_FRAGMENT },
            MaterialParams {
                pipeline_params: PipelineParams { color_blend: Some(double_multiply), ..Default::default() },
                ..Default::default()
            },
        )
        .expect("lightmap shader failed to compile");

        Lights { map, mask, mask_bytes: Vec::new(), light, gloss, multiply }
    }

    fn update_mask(&mut self, tilemap: &Tilemap) {
        let mut bytes = Vec::with_capacity(MAP_W * MAP_H * 4);
        for ty in 0..MAP_H as i32 {
            for tx in 0..MAP_W as i32 {
                let v = if tilemap.get(tx, ty).blocks() { 255 } else { 0 };
                bytes.extend_from_slice(&[v, v, v, 255]);
            }
        }
        if bytes != self.mask_bytes {
            self.mask.update(&Image { width: MAP_W as u16, height: MAP_H as u16, bytes: bytes.clone() });
            self.mask_bytes = bytes;
        }
    }

    /// Light the stone drawn so far. Call after the terrain, before the actors.
    pub fn apply(&mut self, glow: &Glow, tilemap: &Tilemap, lights: &[Light], shine: Option<&Texture2D>) {
        self.update_mask(tilemap);

        // Step 1 and 2: the lightmap, in world coordinates like everything else.
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, MAP_PX.x, MAP_PX.y));
        cam.render_target = Some(self.map.clone());
        set_camera(&cam);
        clear_background(Color::new(AMBIENT, AMBIENT, AMBIENT * 1.1, 1.0));
        gl_use_material(&self.light);
        for l in lights {
            self.light.set_uniform("light", l.pos);
            self.light.set_uniform("radius", l.radius);
            self.light.set_uniform("tint", vec3(l.color.r, l.color.g, l.color.b) * l.strength);
            // A square around the light: the shader only draws inside the circle.
            draw_rectangle(l.pos.x - l.radius, l.pos.y - l.radius, l.radius * 2.0, l.radius * 2.0, WHITE);
        }

        // Step 3: back to the scene, and multiply the lightmap over it.
        glow.resume();
        gl_use_material(&self.multiply);
        draw_texture_ex(
            &self.map.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(MAP_PX),
                // Render targets are stored upside down (see `Glow::present`).
                flip_y: true,
                ..Default::default()
            },
        );

        // Step 4: the glints on the walls, light by light.
        if let Some(shine) = shine {
            gl_use_material(&self.gloss);
            self.gloss.set_texture("Normals", shine.clone());
            for l in lights {
                self.gloss.set_uniform("light", l.pos);
                self.gloss.set_uniform("radius", l.radius);
                self.gloss.set_uniform("tint", vec3(l.color.r, l.color.g, l.color.b) * l.strength);
                draw_rectangle(l.pos.x - l.radius, l.pos.y - l.radius, l.radius * 2.0, l.radius * 2.0, WHITE);
            }
        }
        gl_use_default_material();
    }
}
