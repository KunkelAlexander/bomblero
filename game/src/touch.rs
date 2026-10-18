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

/// True after a touch, until a key is pressed again.
pub fn active() -> bool {
    ACTIVE.with(|a| a.get())
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
    /// `Glow::present`, so they are not blurred or zoomed).
    pub fn draw(&self) {
        let ui = view::layout().ui;
        for stick in [&self.left, &self.right].into_iter().flatten() {
            let (o, p) = (stick.origin * ui, (stick.origin + stick.push() * RADIUS) * ui);
            draw_circle(o.x, o.y, RADIUS * ui, Color::new(0.5, 0.9, 1.0, 0.08));
            draw_circle_lines(o.x, o.y, RADIUS * ui, 2.0, Color::new(0.5, 0.9, 1.0, 0.35));
            draw_circle(p.x, p.y, 22.0 * ui, Color::new(0.5, 0.9, 1.0, 0.35));
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
