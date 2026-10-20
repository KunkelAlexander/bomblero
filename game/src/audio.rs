// What the player hears: sound effects from the sim's events, the music loop,
// and the mute switch.
//
// Pattern: Event Queue (consumer side), the same events `effects.rs` turns into
// sparks. The sim never knows sound exists; a bot match, a replay or a unit test
// runs without any of this. Like `touch.rs`, the state is a thread-local, so any
// screen can ask for a sound without the audio object being handed through
// every function in between.
//
// Sounds come from synth.rs (effects) and music.rs (the tune): code, not files.
//
// Networking note: every player's machine makes its *own* sounds from the
// events it sees. Nothing about sound crosses the wire, only the same events
// that already draw the explosions. The price: a joiner hears things when it
// *sees* them, about 100 ms after the host does (the snapshot delay).

use std::cell::{Cell, RefCell};

use macroquad::audio::{load_sound_from_bytes, play_sound, stop_sound, PlaySoundParams, Sound};
use macroquad::experimental::coroutines::start_coroutine;
use macroquad::prelude::*;
use sim::weapons as w;
use sim::Event;

use crate::music;
use crate::synth::{render, wav, Sfx, SFX_COUNT, SOUNDS};
use crate::touch;
use crate::view;

/// The tune sits under the effects: shots and explosions must stay readable.
const MUSIC_VOLUME: f32 = 0.35;
/// A sound this many px away (or further) is silent. Nearer ones fall off
/// smoothly: a firefight across the map is a murmur, next to you it is loud.
const HEARING: f32 = 800.0;

struct Audio {
    /// Indexed by `Sfx as usize`, like `SOUNDS`.
    sfx: Vec<Sound>,
    music: Sound,
    /// `get_time` of the last play of each effect, for `MIN_GAP`.
    last: [f64; SFX_COUNT],
}

thread_local! {
    /// `None` until the sounds are made (a moment after start-up; see `start`).
    static AUDIO: RefCell<Option<Audio>> = RefCell::new(None);
    /// Lives apart from `AUDIO` so a press of M during start-up still counts.
    static MUTED: Cell<bool> = Cell::new(false);
}

/// Make the sounds in the background. Building them is a little work (the tune
/// is 22 seconds of samples), and in the browser each has to be decoded too, so
/// this runs as a coroutine: a task macroquad advances a bit every frame while
/// the menu is already on screen. Until it finishes the game is simply silent.
pub fn start() {
    start_coroutine(async {
        let mut sfx = Vec::new();
        for s in &SOUNDS {
            // If a sound cannot be loaded, the game goes on without sound
            // rather than refusing to start.
            let Ok(sound) = load_sound_from_bytes(&wav(&render(s.layers))).await else { return };
            sfx.push(sound);
            // One sound per frame, so no frame is long.
            next_frame().await;
        }
        let Ok(music) = load_sound_from_bytes(&wav(&music::tune())).await else { return };
        if !is_muted() {
            play_music(&music);
        }
        AUDIO.with(|a| *a.borrow_mut() = Some(Audio { sfx, music, last: [f64::NEG_INFINITY; SFX_COUNT] }));
    });
}

fn play_music(music: &Sound) {
    play_sound(music, PlaySoundParams { looped: true, volume: MUSIC_VOLUME });
}

pub fn is_muted() -> bool {
    MUTED.with(|m| m.get())
}

pub fn toggle_mute() {
    let muted = !is_muted();
    MUTED.with(|m| m.set(muted));
    AUDIO.with(|a| {
        if let Some(audio) = &*a.borrow() {
            // Stopping beats volume 0: a muted phone does no mixing at all.
            if muted {
                stop_sound(&audio.music);
            } else {
                play_music(&audio.music);
            }
        }
    });
}

/// Call once a frame, on every screen. `typing` is true while the player types
/// a room code: M is one of its letters then.
pub fn frame(typing: bool) {
    if (is_key_pressed(KeyCode::M) && !typing) || touch::clicked_in(button()) {
        toggle_mute();
    }
}

/// Play `sfx`, `loudness` from 0 to 1.
fn play(sfx: Sfx, loudness: f32) {
    if is_muted() || loudness < 0.03 {
        return;
    }
    AUDIO.with(|a| {
        let Some(audio) = &mut *a.borrow_mut() else { return };
        // Never restart the same sound within a few hundredths of a second:
        // 20 laser pulses a second would stack up into
        // a wall of noise (and the mixer has only so many voices).
        let now = get_time();
        if now - audio.last[sfx as usize] < min_gap(sfx) {
            return;
        }
        audio.last[sfx as usize] = now;
        play_sound(&audio.sfx[sfx as usize], PlaySoundParams { looped: false, volume: loudness });
    });
}

fn min_gap(sfx: Sfx) -> f64 {
    match sfx {
        Sfx::Laser => 0.06,
        Sfx::BoomSmall | Sfx::BrickBreak => 0.05,
        _ => 0.03,
    }
}

/// How loud a sound is `distance` px from the listener, 0 to 1.
fn nearness(distance: f32) -> f32 {
    let k = (1.0 - distance / HEARING).clamp(0.0, 1.0);
    k * k
}

/// Which sound a weapon makes when it is fired. Pattern: Type Object again: a
/// lookup from a weapon id, with no weapon-specific code anywhere else.
fn shot_sound(weapon: u8) -> Option<Sfx> {
    Some(match weapon {
        w::BLASTER => Sfx::Blaster,
        w::ROCKET => Sfx::Rocket,
        w::RAILGUN => Sfx::Railgun,
        w::BUILD => Sfx::Build,
        w::BOUNCER => Sfx::Bouncer,
        w::BOMB => Sfx::Bomb,
        w::CLUSTER => Sfx::Cluster,
        w::SWARM => Sfx::Swarm,
        w::TELEPORT => Sfx::Teleport,
        w::NOVA => Sfx::Nova,
        w::SHEEP => Sfx::Sheep,
        w::CONFUSER => Sfx::Confuser,
        w::MIRROR => Sfx::Mirror,
        w::OIL => Sfx::Oil,
        w::LASER => Sfx::Laser,
        w::BLACKHOLE => Sfx::BlackHole,
        w::FLASH => Sfx::Flash,
        _ => return None,
    })
}

/// React to one event. `me` is the local player and `listener` where they are:
/// things far from them are quieter.
pub fn on_event(event: &Event, me: u8, listener: Vec2) {
    let near = |pos: Vec2| nearness(pos.distance(listener));
    match *event {
        Event::Shot { player, weapon, pos } => {
            if let Some(sfx) = shot_sound(weapon) {
                // Your own gun is always at full volume, however the camera lags.
                play(sfx, if player == me { 1.0 } else { near(pos) });
            }
        }
        Event::Explosion { pos, radius } => {
            let sfx = if radius < 50.0 {
                Sfx::BoomSmall
            } else if radius < 150.0 {
                Sfx::BoomMedium
            } else {
                Sfx::BoomBig
            };
            play(sfx, near(pos));
        }
        Event::Hit { victim, pos, .. } => {
            if victim == me {
                play(Sfx::Hurt, 1.0);
            } else {
                play(Sfx::Hit, near(pos));
            }
        }
        Event::Died { pos, .. } => play(Sfx::Death, near(pos).max(0.3)),
        Event::Respawned { player } if player == me => play(Sfx::Respawn, 0.8),
        // A brick that is gone. Bricks that are only damaged (or placed) stay quiet.
        Event::TileChanged { x, y, tile: sim::Tile::Empty } => {
            play(Sfx::BrickBreak, near(sim::tilemap::tile_center(x, y)));
        }
        Event::Dashed { pos, .. } => play(Sfx::Dash, near(pos)),
        Event::Reflected { pos } => play(Sfx::Reflect, near(pos)),
        Event::Teleported { to, .. } => play(Sfx::Teleport, near(to)),
        // The stun of a flashgun is already the flash sound.
        Event::Status { weapon: w::SHEEP, pos, .. } => play(Sfx::Sheep, near(pos)),
        Event::Status { weapon: w::CONFUSER, pos, .. } => play(Sfx::Confuser, near(pos)),
        Event::MatchWon { .. } => play(Sfx::Victory, 1.0),
        _ => {}
    }
}

/// The speaker button's place (picture pixels): top right, left of the touch
/// back button when that is showing.
fn button() -> Rect {
    let slot = if touch::active() { 108.0 } else { 56.0 };
    Rect::new(view::layout().size.x - slot, 12.0, 44.0, 44.0)
}

/// Draw the speaker button over the finished picture, in screen pixels (like
/// `touch::draw_back_button`).
pub fn draw_button() {
    let ui = view::layout().ui;
    let r = button();
    let (r, c) = (Rect::new(r.x * ui, r.y * ui, r.w * ui, r.h * ui), Color::new(0.7, 0.75, 0.85, 0.6));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.03, 0.03, 0.04, 0.6));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, c);
    let (m, u) = (r.center(), r.w / 44.0);
    // The speaker: a small box and a cone opening to the right.
    draw_rectangle(m.x - 12.0 * u, m.y - 4.0 * u, 6.0 * u, 8.0 * u, c);
    draw_triangle(vec2(m.x - 6.0 * u, m.y - 4.0 * u), vec2(m.x - 6.0 * u, m.y + 4.0 * u), vec2(m.x + 2.0 * u, m.y + 11.0 * u), c);
    draw_triangle(vec2(m.x - 6.0 * u, m.y - 4.0 * u), vec2(m.x + 2.0 * u, m.y - 11.0 * u), vec2(m.x + 2.0 * u, m.y + 11.0 * u), c);
    if is_muted() {
        // A slash across it.
        draw_line(m.x - 14.0 * u, m.y + 14.0 * u, m.x + 14.0 * u, m.y - 14.0 * u, 3.0, Color::new(1.0, 0.35, 0.4, 0.9));
    } else {
        // Two sound waves.
        for (dx, h) in [(7.0, 5.0), (12.0, 9.0)] {
            draw_line(m.x + dx * u, m.y - h * u, m.x + dx * u, m.y + h * u, 2.0, c);
        }
    }
}
