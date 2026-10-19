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

/// The smallest a 32 px tile may look on the real screen (in CSS pixels on the
/// web). Below this we zoom in and follow the player instead of showing more of
/// the arena. Size on the eye is what counts: a monitor is ~60 cm away, a phone
/// ~30 cm, so 24 on a phone already looks like 32 on a monitor. But a phone
/// shooter needs more than "readable": small bullets must be spotted at a
/// glance next to your thumbs, so phones get bigger tiles and see less arena
/// (the look-ahead below wins some of it back). Tune these two to taste.
const MIN_TILE_PX_DESKTOP: f32 = 24.0;
const MIN_TILE_PX_PHONE: f32 = 32.0;

/// How far the camera leads towards where you aim, as a fraction of the visible
/// arena. Zooming in costs awareness, and the direction you shoot in is where
/// awareness matters, so the view shifts that way ("look-ahead camera").
const LOOK_AHEAD: f32 = 0.25;
/// How quickly the look-ahead follows a new aim, per second. Smoothed so
/// flicking the aim around does not whip the whole view.
const LOOK_SPEED: f32 = 4.0;

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
    /// Where the HUD strip starts, in picture coordinates.
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
    // The current (smoothed) look-ahead offset, in world pixels.
    static LOOK: Cell<Vec2> = Cell::new(Vec2::ZERO);
    // Whether the next `present` flips the picture (see `mirror`), and whether
    // the one shown last frame was flipped (for `screen_to_world`).
    static MIRROR: Cell<bool> = Cell::new(false);
    static SHOWN_MIRRORED: Cell<bool> = Cell::new(false);
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
/// tiles of at least the minimum size, it is shown whole and centred;
/// otherwise the camera zooms in, centres on the player (led a little towards
/// `aim`) and stops at the arena's edges.
pub fn follow(pos: Vec2, aim: f32) {
    place(pos, Some(aim));
}

/// Hold the camera on a fixed world point, without the look-ahead.
pub fn pin(at: Vec2) {
    place(at, None);
}

fn place(pos: Vec2, aim: Option<f32>) {
    let l = layout();
    let area = vec2(l.size.x, l.hud_top() - TOP_MARGIN);
    let fit = (area.x / MAP_SIZE.x).min(area.y / MAP_SIZE.y);
    let min_tile = if crate::touch::is_touch_device() { MIN_TILE_PX_PHONE } else { MIN_TILE_PX_DESKTOP };
    let zoom = fit.max(min_tile / (32.0 * l.ui));
    let seen = area / zoom;

    // Lead towards the aim, but only along an axis that scrolls: a map that
    // fits stays still.
    let scrolls = vec2((seen.x < MAP_SIZE.x) as u8 as f32, (seen.y < MAP_SIZE.y) as u8 as f32);
    let target = aim.map_or(Vec2::ZERO, |a| vec2(a.cos(), a.sin()) * seen * LOOK_AHEAD * scrolls);
    let blend = 1.0 - (-LOOK_SPEED * get_frame_time()).exp();
    let look = LOOK.with(|l| {
        let look = l.get() + (target - l.get()) * blend;
        l.set(look);
        look
    });
    let pos = pos + look;

    // Per axis: centre the map if it fits, else follow and clamp.
    let axis = |p: f32, seen: f32, map: f32| if seen >= map { (map - seen) / 2.0 } else { (p - seen / 2.0).clamp(0.0, map - seen) };
    let corner = vec2(axis(pos.x, seen.x, MAP_SIZE.x), axis(pos.y, seen.y, MAP_SIZE.y));
    let rect = Rect::new(corner.x, corner.y - TOP_MARGIN / zoom, l.size.x / zoom, l.size.y / zoom);
    VIEW.with(|v| v.set(Some(rect)));
}

/// Flip this frame's arena left-to-right (the Confuser). Call before
/// `Glow::present`. Only the arena flips: the HUD is drawn afterwards, so its
/// text stays readable. Keys and thumbsticks are not flipped, which is the whole
/// point: left on the keyboard now moves you right on the screen.
///
/// Why here and not in the sim: mirroring is a way of *showing* the world. The
/// host never learns what anyone sees, so the effect costs no network traffic
/// beyond the timer already in `Player::confused`.
pub fn mirror(on: bool) {
    MIRROR.with(|m| m.set(on));
}

/// Was the picture shown last frame flipped? Input code that turns a position
/// or direction on the screen into the world needs to know.
pub fn mirrored() -> bool {
    SHOWN_MIRRORED.with(|m| m.get())
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
    let mut at = screen / l.ui;
    if mirrored() {
        at.x = l.size.x - at.x; // so aiming follows the flipped picture
    }
    vec2(view.x, view.y) + at * (view.w / l.size.x)
}

/// How bright a colour must be before it glows (see `DOWN_FRAGMENT`).
const THRESHOLD: f32 = 0.65;

/// The glow post-process ("bloom"): draw the scene into an offscreen texture,
/// blur the bright parts of it at half and quarter size, and put the sum on
/// the window. (A third, eighth-size level made a wider halo, but it was more
/// glow than the game needs and cost three more passes.) See the shaders in shaders.rs.
///
/// A frame has two layers:
///   * the scene, between `begin` and `present`: the arena, and anything that
///     should glow like a neon sign (the title, big labels);
///   * the UI, after `present` and `ui`: drawn straight onto the finished
///     picture, so small text is never blurred by the glow.
pub struct Glow {
    /// Sized like the window (real pixels, so lines stay sharp on a big
    /// screen), so recreated when the window changes size.
    scene: RefCell<RenderTarget>,
    /// One pair per glow level, each half the size of the one before: the
    /// level itself and a scratch texture for the first half of its blur.
    levels: RefCell<Vec<[RenderTarget; 2]>>,
    down: Material,
    blur: Material,
    combine: Material,
}

impl Glow {
    pub fn new() -> Glow {
        let material = |fragment, uniforms: &[(&str, UniformType)], textures: &[&str]| {
            load_material(
                ShaderSource::Glsl { vertex: shaders::VERTEX, fragment },
                MaterialParams {
                    uniforms: uniforms.iter().map(|(name, kind)| UniformDesc::new(name, *kind)).collect(),
                    textures: textures.iter().map(|t| t.to_string()).collect(),
                    ..Default::default()
                },
            )
            .expect("glow shader failed to compile")
        };
        Glow {
            scene: RefCell::new(render_target(1, 1)),
            levels: RefCell::new(Vec::new()),
            down: material(shaders::DOWN_FRAGMENT, &[("texel", UniformType::Float2), ("threshold", UniformType::Float1)], &[]),
            blur: material(shaders::BLUR_FRAGMENT, &[("step", UniformType::Float2)], &[]),
            combine: material(shaders::COMBINE_FRAGMENT, &[], &["Near", "Far"]),
        }
    }

    /// Start drawing the scene. Afterwards, draw in world coordinates.
    pub fn begin(&self) {
        let (w, h) = (screen_width().round().max(8.0) as u32, screen_height().round().max(8.0) as u32);
        if (self.scene.borrow().texture.width() as u32, self.scene.borrow().texture.height() as u32) != (w, h) {
            let target = |w: u32, h: u32| {
                let t = render_target(w.max(1), h.max(1));
                t.texture.set_filter(FilterMode::Linear); // the blur's in-between taps need it
                t
            };
            *self.scene.borrow_mut() = target(w, h);
            let levels: Vec<[RenderTarget; 2]> = [2, 4].iter().map(|d| [target(w / d, h / d), target(w / d, h / d)]).collect();
            for (name, level) in ["Near", "Far"].iter().zip(&levels) {
                self.combine.set_texture(name, level[0].texture.clone());
            }
            *self.levels.borrow_mut() = levels;
        }
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
    /// of the arena, keep their size and should glow (the armory's title).
    pub fn overlay(&self) {
        let l = layout();
        self.camera(Rect::new(0.0, 0.0, l.size.x, l.size.y));
    }

    fn camera(&self, rect: Rect) {
        let mut cam = Camera2D::from_display_rect(rect);
        cam.render_target = Some(self.scene.borrow().clone());
        set_camera(&cam);
    }

    /// Finish the scene: put it on the window with the glow applied.
    pub fn present(&self) {
        // Remember what was shown (for the mouse) and drop the follow, so the
        // next screen starts with the default view unless it follows again.
        let shown = VIEW.with(|v| v.take()).unwrap_or_else(|| default_view(&layout()));
        SHOWN.with(|v| v.set(shown));
        let mirrored = MIRROR.with(|m| m.take());
        SHOWN_MIRRORED.with(|m| m.set(mirrored));

        // Each level shrinks the one above it (the first shrinks the scene
        // and keeps only its bright parts), then is blurred across and down.
        let scene = self.scene.borrow();
        let mut source = scene.texture.clone();
        for (i, [level, scratch]) in self.levels.borrow().iter().enumerate() {
            let texel = vec2(1.0 / source.width(), 1.0 / source.height());
            self.down.set_uniform("texel", texel);
            self.down.set_uniform("threshold", if i == 0 { THRESHOLD } else { 0.0 });
            pass(&self.down, &source, level);
            let (w, h) = (level.texture.width(), level.texture.height());
            self.blur.set_uniform("step", vec2(1.0 / w, 0.0));
            pass(&self.blur, &level.texture, scratch);
            self.blur.set_uniform("step", vec2(0.0, 1.0 / h));
            pass(&self.blur, &scratch.texture, level);
            source = level.texture.clone();
        }

        set_default_camera();
        clear_background(BLACK);
        gl_use_material(&self.combine);
        draw_texture_ex(
            &scene.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                // Render targets are stored upside down relative to the screen.
                flip_y: true,
                flip_x: mirrored,
                ..Default::default()
            },
        );
        gl_use_default_material();
    }

    /// After `present`: draw on the finished picture, in picture coordinates.
    /// This is the crisp layer for small text (the HUD, the armory's cards).
    pub fn ui(&self) {
        let l = layout();
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, l.size.x, l.size.y));
        // macroquad flips y for the window but not for render targets; undo it
        // so y grows downwards here too.
        cam.zoom.y = -cam.zoom.y;
        set_camera(&cam);
    }
}

/// Draw `source` over all of `target` through `material`.
fn pass(material: &Material, source: &Texture2D, target: &RenderTarget) {
    let (w, h) = (target.texture.width(), target.texture.height());
    let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, w, h));
    cam.render_target = Some(target.clone());
    set_camera(&cam);
    gl_use_material(material);
    // Copying one render target into another keeps it upside down (see `present`).
    draw_texture_ex(source, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), flip_y: true, ..Default::default() });
}
