// M0: prove the toolchain. A neon triangle rendered through a glow shader,
// running both natively and in the browser. Nothing here is game logic yet.

mod shaders;

use macroquad::prelude::*;

// The game always renders at a fixed logical resolution and the window just
// shows it. That way every player sees the same map size, which matters
// for fairness in a shooter and keeps the later netcode free of screen-size
// concerns.
const WIDTH: i32 = 1280;
const HEIGHT: i32 = 720;

fn window_conf() -> Conf {
    Conf {
        window_title: "Neon Bomber".to_owned(),
        window_width: WIDTH,
        window_height: HEIGHT,
        window_resizable: false,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Pattern: Game Loop. macroquad owns the outer loop (`next_frame().await`
    // below). Later, the sim will run inside it at a fixed 60 Hz while drawing
    // happens at whatever rate the display runs.

    // Scene is drawn into this texture first; the glow shader then reads it.
    let scene = render_target(WIDTH as u32, HEIGHT as u32);
    scene.texture.set_filter(FilterMode::Linear);

    let glow_material = load_material(
        ShaderSource::Glsl {
            vertex: shaders::VERTEX,
            fragment: shaders::GLOW_FRAGMENT,
        },
        MaterialParams {
            uniforms: vec![UniformDesc::new("texel", UniformType::Float2)],
            ..Default::default()
        },
    )
    .expect("glow shader failed to compile");
    glow_material.set_uniform("texel", vec2(1.0 / WIDTH as f32, 1.0 / HEIGHT as f32));

    loop {
        let t = get_time() as f32;

        // 1) Draw the sharp scene into the offscreen texture.
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32));
        cam.render_target = Some(scene.clone());
        set_camera(&cam);
        clear_background(BLACK);
        draw_neon_triangle(vec2(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0), 180.0, t);

        // 2) Draw that texture to the screen through the glow shader.
        set_default_camera();
        clear_background(BLACK);
        // Letterbox: scale uniformly to fit and centre, so a window that is not
        // 16:9 gets black bars instead of a stretched picture.
        let scale = (screen_width() / WIDTH as f32).min(screen_height() / HEIGHT as f32);
        let (w, h) = (WIDTH as f32 * scale, HEIGHT as f32 * scale);
        gl_use_material(&glow_material);
        draw_texture_ex(
            &scene.texture,
            (screen_width() - w) / 2.0,
            (screen_height() - h) / 2.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(w, h)),
                // Render targets are stored upside down relative to the screen.
                flip_y: true,
                ..Default::default()
            },
        );
        gl_use_default_material();

        next_frame().await;
    }
}

// Neon look = thin, saturated outlines on black. The shader supplies the halo.
fn draw_neon_triangle(center: Vec2, radius: f32, t: f32) {
    let color = Color::new(0.1, 1.0, 0.9, 1.0);
    let corner = |i: i32| {
        let a = t * 0.8 + i as f32 * std::f32::consts::TAU / 3.0;
        center + vec2(a.cos(), a.sin()) * radius
    };
    for i in 0..3 {
        let (a, b) = (corner(i), corner((i + 1) % 3));
        draw_line(a.x, a.y, b.x, b.y, 3.0, color);
    }
}
