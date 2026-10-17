// The "camera": how the fixed logical game space maps onto the real window.
// Nothing in here knows about game rules.

use macroquad::prelude::*;

use crate::shaders;

// The game always renders at a fixed logical resolution and the window just
// shows it. That way every player sees the same map size, which matters
// for fairness in a shooter and keeps the later netcode free of screen-size
// concerns.
pub const WIDTH: f32 = 1280.0;
pub const HEIGHT: f32 = 720.0;

/// The map is 1280x704 px, so it sits 8 px below the top to be centred.
pub const ORIGIN: Vec2 = vec2(0.0, 8.0);

/// Letterbox: scale uniformly to fit the window and centre, so a window that is
/// not 16:9 gets black bars instead of a stretched picture.
/// Returns (top-left corner of the picture in window pixels, scale).
fn viewport() -> (Vec2, f32) {
    let scale = (screen_width() / WIDTH).min(screen_height() / HEIGHT);
    let size = vec2(WIDTH, HEIGHT) * scale;
    ((vec2(screen_width(), screen_height()) - size) / 2.0, scale)
}

/// Window pixel (e.g. the mouse) -> game-world coordinates. The inverse of what
/// `Glow::present` does, so the cursor lines up with what is drawn.
pub fn screen_to_world(screen: Vec2) -> Vec2 {
    let (offset, scale) = viewport();
    (screen - offset) / scale - ORIGIN
}

/// The glow post-process: draw the scene into an offscreen texture, then draw
/// that texture to the window through a blur shader.
pub struct Glow {
    scene: RenderTarget,
    material: Material,
}

impl Glow {
    pub fn new() -> Glow {
        let scene = render_target(WIDTH as u32, HEIGHT as u32);
        scene.texture.set_filter(FilterMode::Linear);
        let material = load_material(
            ShaderSource::Glsl { vertex: shaders::VERTEX, fragment: shaders::GLOW_FRAGMENT },
            MaterialParams {
                uniforms: vec![UniformDesc::new("texel", UniformType::Float2)],
                ..Default::default()
            },
        )
        .expect("glow shader failed to compile");
        material.set_uniform("texel", vec2(1.0 / WIDTH, 1.0 / HEIGHT));
        Glow { scene, material }
    }

    /// Start drawing the scene. Afterwards, draw in world coordinates.
    pub fn begin(&self) {
        let mut cam = Camera2D::from_display_rect(Rect::new(-ORIGIN.x, -ORIGIN.y, WIDTH, HEIGHT));
        cam.render_target = Some(self.scene.clone());
        set_camera(&cam);
        clear_background(BLACK);
    }

    /// Finish: put the scene on the window with the glow applied.
    pub fn present(&self) {
        set_default_camera();
        clear_background(BLACK);
        let (offset, scale) = viewport();
        gl_use_material(&self.material);
        draw_texture_ex(
            &self.scene.texture,
            offset.x,
            offset.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(WIDTH, HEIGHT) * scale),
                // Render targets are stored upside down relative to the screen.
                flip_y: true,
                ..Default::default()
            },
        );
        gl_use_default_material();
    }
}
