// Touchscreen controls: two virtual thumbsticks.
//
//   * A finger that lands on the left half of the screen is the move stick,
//     one on the right half the aim stick. Pushing the aim stick aims and fires
//     (the classic twin-stick shooter layout).
//   * The sticks "float": their centre is wherever the thumb landed, so the
//     player never has to find a drawn button without looking.
//   * Each stick remembers which finger (touch id) owns it, so both thumbs work
//     at the same time and lifting one never disturbs the other.
//
// This file only turns fingers into numbers. `Controls` mixes them into the same
// `sim::Input` the keyboard produces, so the sim, the host and the network never
// know a phone was involved: Pattern: Command pays off here, as a new input
// device costs one file and zero changes to the game rules or the netcode.

use std::cell::Cell;

use macroquad::prelude::*;

use crate::view;

/// How far (in picture pixels) a thumb must move for a full push.
const RADIUS: f32 = 64.0;
/// The aim stick fires only past this fraction of a full push, so a resting
/// thumb can turn without wasting ammo.
const FIRE_AT: f32 = 0.3;

thread_local! {
    // Whether the player is using a touchscreen right now. Global so screens can
    // swap their hints ("press R" vs "tap") without having it handed down.
    static ACTIVE: Cell<bool> = Cell::new(false);
}

/// True on a phone or tablet from the start, and on any device after a touch,
/// until a key is pressed again.
pub fn active() -> bool {
    ACTIVE.with(|a| a.get())
}

/// Is the main pointer a finger (phone, tablet)? Asked of the browser (see
/// web/device.js); a native build is always a desktop.
#[cfg(target_arch = "wasm32")]
pub fn is_touch_device() -> bool {
    #[link(wasm_import_module = "env")]
    extern "C" {
        fn device_is_touch() -> i32;
    }
    // SAFETY: a plain JS function without arguments.
    unsafe { device_is_touch() != 0 }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_touch_device() -> bool {
    false
}

/// Twin sticks need both thumbs side by side, so phones play in landscape.
/// In portrait we show a hint instead of a cramped game.
pub fn wants_landscape() -> bool {
    is_touch_device() && screen_height() > screen_width()
}

/// The "turn your device" screen, drawn over everything in screen pixels.
pub fn draw_rotate_hint() {
    let (w, h) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.02, 0.02, 0.03, 1.0));
    let c = vec2(w / 2.0, h * 0.4);
    let neon = Color::new(0.1, 1.0, 0.95, 1.0);
    let dim = Color::new(0.45, 0.47, 0.52, 1.0);
    // A phone upright (dim) and the same phone turned (bright).
    draw_rectangle_lines(c.x - 70.0, c.y - 45.0, 50.0, 90.0, 3.0, dim);
    draw_rectangle_lines(c.x - 5.0, c.y - 25.0, 90.0, 50.0, 3.0, neon);
    draw_circle(c.x + 75.0, c.y, 4.0, neon);
    for (text, y, size, color) in [("turn your device sideways", h * 0.4 + 100.0, 28.0, WHITE), ("left thumb moves, right thumb aims and fires", h * 0.4 + 136.0, 18.0, dim)] {
        let width = measure_text(text, None, size as u16, 1.0).width;
        draw_text(text, (w - width) / 2.0, y, size, color);
    }
}

struct Stick {
    /// The finger that owns this stick.
    id: u64,
    /// Where the finger landed, and where it is now (picture coordinates).
    origin: Vec2,
    pos: Vec2,
}

impl Stick {
    /// The push as a vector of length 0..=1.
    fn push(&self) -> Vec2 {
        ((self.pos - self.origin) / RADIUS).clamp_length_max(1.0)
    }
}

#[derive(Default)]
pub struct Touch {
    /// Whether the first `update` has run (it sets the starting mode).
    started: bool,
    left: Option<Stick>,
    right: Option<Stick>,
    /// Fingers that landed on the HUD this frame (picture coordinates): taps
    /// on the weapon cards, handled by `Controls`.
    pub taps: Vec<Vec2>,
}

impl Touch {
    /// Read the fingers. Call once per frame, on every screen, so a finger
    /// lifted while a menu was up is never left "held".
    pub fn update(&mut self) {
        if !self.started {
            self.started = true;
            ACTIVE.with(|a| a.set(is_touch_device()));
        }
        if wants_landscape() {
            // The rotate hint covers the game; fingers on it are not input.
            (self.left, self.right) = (None, None);
            self.taps.clear();
            return;
        }
        let touches = touches();
        if !touches.is_empty() {
            ACTIVE.with(|a| a.set(true));
        } else if get_last_key_pressed().is_some() {
            ACTIVE.with(|a| a.set(false));
        }

        // Drop sticks whose finger is gone (macroquad forgets a touch the frame
        // after it ends).
        let alive = |s: &Option<Stick>| s.as_ref().is_some_and(|s| touches.iter().any(|t| t.id == s.id && t.phase != TouchPhase::Ended && t.phase != TouchPhase::Cancelled));
        if !alive(&self.left) {
            self.left = None;
        }
        if !alive(&self.right) {
            self.right = None;
        }

        self.taps.clear();
        let l = view::layout();
        for t in &touches {
            let pos = t.position / l.ui;
            match t.phase {
                TouchPhase::Started => {
                    if pos.y >= l.hud_top() {
                        self.taps.push(pos);
                    } else if back_button().contains(t.position) {
                        // The back button handles itself (via the mouse).
                    } else if pos.x < l.size.x / 2.0 {
                        self.left.get_or_insert(Stick { id: t.id, origin: pos, pos });
                    } else {
                        self.right.get_or_insert(Stick { id: t.id, origin: pos, pos });
                    }
                }
                _ => {
                    for stick in [&mut self.left, &mut self.right].into_iter().flatten() {
                        if stick.id == t.id {
                            stick.pos = pos;
                        }
                    }
                }
            }
        }
    }

    /// The move stick's push, length 0..=1.
    pub fn move_dir(&self) -> Vec2 {
        self.left.as_ref().map_or(Vec2::ZERO, Stick::push)
    }

    /// The aim stick: `Some((angle, fire))` while it is pushed at all.
    pub fn aim(&self) -> Option<(f32, bool)> {
        let push = self.right.as_ref()?.push();
        if push.length() < 0.05 {
            return None;
        }
        Some((push.y.atan2(push.x), push.length() >= FIRE_AT))
    }

    /// Draw the sticks over the finished picture, in screen pixels (call after
    /// `Glow::present`, so they are not blurred or zoomed). With `hints`, a
    /// stick nobody holds is drawn as a faint ghost with a label in its corner,
    /// so a new player sees that the screen is the controller.
    pub fn draw(&self, hints: bool) {
        let l = view::layout();
        let ui = l.ui;
        let low = l.hud_top() - RADIUS - 28.0;
        let homes = [(vec2(RADIUS + 40.0, low), "MOVE"), (vec2(l.size.x - RADIUS - 40.0, low), "AIM + FIRE")];
        for (stick, (home, label)) in [&self.left, &self.right].into_iter().zip(homes) {
            match stick {
                Some(stick) => {
                    let (o, p) = (stick.origin * ui, (stick.origin + stick.push() * RADIUS) * ui);
                    draw_circle(o.x, o.y, RADIUS * ui, Color::new(0.5, 0.9, 1.0, 0.08));
                    draw_circle_lines(o.x, o.y, RADIUS * ui, 2.0, Color::new(0.5, 0.9, 1.0, 0.35));
                    draw_circle(p.x, p.y, 22.0 * ui, Color::new(0.5, 0.9, 1.0, 0.35));
                }
                None if hints => {
                    let o = home * ui;
                    draw_circle_lines(o.x, o.y, RADIUS * ui, 2.0, Color::new(0.5, 0.9, 1.0, 0.18));
                    draw_circle_lines(o.x, o.y, 22.0 * ui, 2.0, Color::new(0.5, 0.9, 1.0, 0.18));
                    let size = 16.0 * ui;
                    let width = measure_text(label, None, size as u16, 1.0).width;
                    draw_text(label, o.x - width / 2.0, o.y + (RADIUS + 18.0) * ui, size, Color::new(0.5, 0.9, 1.0, 0.45));
                }
                None => {}
            }
        }
    }
}

/// The "back to the menu" button in the top right corner, in screen pixels.
/// It stands in for Esc on screens without a keyboard.
pub fn back_button() -> Rect {
    let ui = view::layout().ui;
    Rect::new(screen_width() - 56.0 * ui, 12.0 * ui, 44.0 * ui, 44.0 * ui)
}

pub fn draw_back_button() {
    let r = back_button();
    let c = Color::new(0.7, 0.75, 0.85, 0.6);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.03, 0.03, 0.04, 0.6));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, c);
    let (m, k) = (r.center(), r.w * 0.22);
    draw_line(m.x - k, m.y - k, m.x + k, m.y + k, 3.0, c);
    draw_line(m.x - k, m.y + k, m.x + k, m.y - k, 3.0, c);
}

/// Was the back button tapped (or clicked) this frame?
pub fn back_pressed() -> bool {
    active() && is_mouse_button_pressed(MouseButton::Left) && back_button().contains(Vec2::from(mouse_position()))
}
