// The "camera": how the game is laid out on whatever screen it runs on.
// Nothing in here knows about game rules.
//
// Three coordinate spaces:
//   * screen:  real window pixels (the mouse, touches).
//   * picture: screen pixels divided by the UI scale. The HUD, plates and the
//              armory are laid out here, so they keep a readable size on any
//              screen. Its shape is the screen's shape: no black bars.
//   * world:   the arena's own pixels (1280x704). A camera maps the visible
//              part of it onto the picture area above the HUD.
//
// Different screens see different amounts of the arena. That is fine for the
// netcode: the camera is pure presentation, and the host, the sim and the
// other players never know what anyone is looking at.

use std::cell::{Cell, RefCell};

use macroquad::prelude::*;

use crate::shaders;

/// The classic desktop picture. On a screen at least this big the UI scales
/// up uniformly (as the game always did); on smaller ones the picture simply
/// gets fewer pixels and the arena camera zooms in instead.
pub const WIDTH: f32 = 1280.0;
pub const HEIGHT: f32 = 768.0;

/// The map sits this far below the top of the picture.
const TOP_MARGIN: f32 = 8.0;

/// The smallest a 32 px tile may look on the real screen. Below this we zoom
/// in and follow the player instead of showing more of the arena.
const MIN_TILE_PX: f32 = 24.0;

/// The map's size in world pixels (40x22 tiles of 32 px).
const MAP_SIZE: Vec2 = vec2(1280.0, 704.0);

/// How this frame's screen is divided up. Recomputed every time it is asked
/// for: it is a handful of arithmetic and the window can be resized any time.
pub struct Layout {
    /// The picture's size (see the top of the file).
    pub size: Vec2,
    /// Screen pixels per picture pixel.
    pub ui: f32,
    /// How many rows the HUD needs at this width (see hud.rs).
    pub hud_rows: u32,
}

impl Layout {
    /// Where the HUD strip starts, in picture coordinates. Below this line the
    /// glow shader is switched off, so text stays crisp.
    pub fn hud_top(&self) -> f32 {
        self.size.y - 54.0 - (self.hud_rows - 1) as f32 * 50.0
    }
}

pub fn layout() -> Layout {
    let ui = (screen_width() / WIDTH).min(screen_height() / HEIGHT).max(1.0);
    let size = vec2(screen_width(), screen_height()) / ui;
    // One row fits the weapon cards and the scores side by side (from 820 px
    // the status line moves to the top left; see hud.rs). A landscape phone
    // must stay at one row: its height is too precious. Narrower screens stack.
    let hud_rows = if size.x >= 820.0 { 1 } else if size.x >= 640.0 { 2 } else { 3 };
    Layout { size, ui, hud_rows }
}

thread_local! {
    // The world rectangle the scene camera shows this frame (`None`: the
    // default view, see `default_view`), and the one shown last frame. Input
    // is read before drawing, so `screen_to_world` must use what the player
    // *saw*. Thread-locals instead of `Glow` fields so input code can reach them.
    static VIEW: Cell<Option<Rect>> = Cell::new(None);
    static SHOWN: Cell<Rect> = Cell::new(Rect::new(0.0, -TOP_MARGIN, WIDTH, HEIGHT));
}

/// Screens that do not follow a player (connecting) draw a classic 1280x768
/// picture, fitted into the real one and centred.
fn default_view(l: &Layout) -> Rect {
    let fit = (l.size.x / WIDTH).min(l.size.y / HEIGHT);
    let size = l.size / fit;
    Rect::new(-(size.x - WIDTH) / 2.0, -TOP_MARGIN - (size.y - HEIGHT) / 2.0, size.x, size.y)
}

/// Point the arena camera at `pos` (the local player) for this frame. Call
/// before `Glow::begin`.
///
/// The arena gets the picture above the HUD. If the whole map fits there with
/// tiles of at least `MIN_TILE_PX`, it is shown whole and centred; otherwise
/// the camera zooms in, centres on the player and stops at the arena's edges.
pub fn follow(pos: Vec2) {
    let l = layout();
    let area = vec2(l.size.x, l.hud_top() - TOP_MARGIN);
    let fit = (area.x / MAP_SIZE.x).min(area.y / MAP_SIZE.y);
    let zoom = fit.max(MIN_TILE_PX / (32.0 * l.ui));
    let seen = area / zoom;
    // Per axis: centre the map if it fits, else follow and clamp.
    let axis = |p: f32, seen: f32, map: f32| if seen >= map { (map - seen) / 2.0 } else { (p - seen / 2.0).clamp(0.0, map - seen) };
    let corner = vec2(axis(pos.x, seen.x, MAP_SIZE.x), axis(pos.y, seen.y, MAP_SIZE.y));
    let rect = Rect::new(corner.x, corner.y - TOP_MARGIN / zoom, l.size.x / zoom, l.size.y / zoom);
    VIEW.with(|v| v.set(Some(rect)));
}

/// Window pixel (e.g. the mouse) -> picture coordinates.
pub fn screen_to_picture(screen: Vec2) -> Vec2 {
    screen / layout().ui
}

/// Window pixel (e.g. the mouse) -> game-world coordinates. The inverse of
/// what the scene camera did last frame, so the cursor lines up with what is drawn.
pub fn screen_to_world(screen: Vec2) -> Vec2 {
    let l = layout();
    let view = SHOWN.with(|v| v.get());
    vec2(view.x, view.y) + screen / l.ui * (view.w / l.size.x)
}

/// The glow post-process: draw the scene into an offscreen texture, then draw
/// that texture to the window through a blur shader.
pub struct Glow {
    /// Sized like the picture, so recreated when the window changes size.
    scene: RefCell<RenderTarget>,
    material: Material,
}

impl Glow {
    pub fn new() -> Glow {
        let material = load_material(
            ShaderSource::Glsl { vertex: shaders::VERTEX, fragment: shaders::GLOW_FRAGMENT },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("texel", UniformType::Float2),
                    UniformDesc::new("hud_edge", UniformType::Float1),
                ],
                ..Default::default()
            },
        )
        .expect("glow shader failed to compile");
        Glow { scene: RefCell::new(render_target(1, 1)), material }
    }

    /// Start drawing the scene. Afterwards, draw in world coordinates.
    pub fn begin(&self) {
        let l = layout();
        let (w, h) = (l.size.x.round() as u32, l.size.y.round() as u32);
        if (self.scene.borrow().texture.width() as u32, self.scene.borrow().texture.height() as u32) != (w, h) {
            let scene = render_target(w, h);
            scene.texture.set_filter(FilterMode::Linear);
            *self.scene.borrow_mut() = scene;
            self.material.set_uniform("texel", vec2(1.0 / w as f32, 1.0 / h as f32));
        }
        // As a fraction of the picture height measured from the bottom, which
        // is how the shader sees `uv.y` (render targets are stored upside down).
        self.material.set_uniform("hud_edge", (l.size.y - l.hud_top()) / l.size.y);
        self.resume();
        clear_background(BLACK);
    }

    /// Draw into the scene again (without clearing it) after drawing somewhere
    /// else, e.g. the lightmap.
    pub fn resume(&self) {
        let rect = VIEW.with(|v| v.get()).unwrap_or_else(|| default_view(&layout()));
        self.camera(rect);
    }

    /// Draw into the scene in picture coordinates, for things that sit on top
    /// of the arena and must keep their size: the HUD, plates with text, the
    /// armory.
    pub fn overlay(&self) {
        let l = layout();
        self.camera(Rect::new(0.0, 0.0, l.size.x, l.size.y));
    }

    fn camera(&self, rect: Rect) {
        let mut cam = Camera2D::from_display_rect(rect);
        cam.render_target = Some(self.scene.borrow().clone());
        set_camera(&cam);
    }

    /// Finish: put the scene on the window with the glow applied.
    pub fn present(&self) {
        // Remember what was shown (for the mouse) and drop the follow, so the
        // next screen starts with the default view unless it follows again.
        let shown = VIEW.with(|v| v.take()).unwrap_or_else(|| default_view(&layout()));
        SHOWN.with(|v| v.set(shown));
        set_default_camera();
        clear_background(BLACK);
        gl_use_material(&self.material);
        draw_texture_ex(
            &self.scene.borrow().texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                // Render targets are stored upside down relative to the screen.
                flip_y: true,
                ..Default::default()
            },
        );
        gl_use_default_material();
    }
}
