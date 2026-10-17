// The game executable: glue between the platform (window, input devices,
// display) and the sim. It owns no game rules.
//
//   devices -> controls -> Input ─┐
//   bots ---------------> Input ──┼─> World::step (60 Hz) -> state + events
//                                 │                              │
//                                 │            draw / effects <──┘
//
// Everything left of `World::step` produces Commands; everything right of it
// only reads.

mod controls;
mod draw;
mod effects;
mod shaders;
mod view;

use macroquad::prelude::*;
use sim::bots::Bot;
use sim::tilemap::{Tilemap, MAP_GRID};
use sim::weapons::{BLASTER, RAILGUN, ROCKET};
use sim::{Input, World, DT};

const LOCAL_PLAYER: usize = 0;
const PLAYER_COUNT: usize = 4; // you + 3 bots

fn window_conf() -> Conf {
    Conf {
        window_title: "Neon Bomber".to_owned(),
        window_width: view::WIDTH as i32,
        window_height: view::HEIGHT as i32,
        window_resizable: false,
        ..Default::default()
    }
}

fn new_match() -> (World, Vec<Bot>) {
    let seed = (macroquad::miniquad::date::now() * 1000.0) as u32;
    let world = World::new(Tilemap::from_ascii(MAP_GRID), PLAYER_COUNT, &[BLASTER, ROCKET, RAILGUN], seed);
    // One brain per player slot; slot 0 is the human so its brain goes unused.
    let bots = (0..PLAYER_COUNT).map(|i| Bot::new(seed.wrapping_add(i as u32 * 7919))).collect();
    (world, bots)
}

#[macroquad::main(window_conf)]
async fn main() {
    let glow = view::Glow::new();
    let mut controls = controls::Controls::new();
    let mut effects = effects::Effects::default();
    let (mut world, mut bots) = new_match();

    // Pattern: Game Loop. macroquad owns the outer loop (`next_frame().await`).
    // Inside it we run the sim on a fixed 1/60 s step using an accumulator:
    // real elapsed time is added to a bucket, and every time the bucket holds a
    // full tick we spend it on one `step`. A slow frame runs several ticks to
    // catch up; a fast frame may run none. Drawing happens once per frame
    // regardless, at whatever rate the monitor runs.
    let mut bucket = 0.0;

    loop {
        if is_key_pressed(KeyCode::R) {
            (world, bots) = new_match();
        }

        let frame = get_frame_time();
        // Cap the catch-up: after a long stall (e.g. a background tab) we skip
        // time instead of freezing while we simulate seconds of ticks.
        bucket += frame.min(0.1);

        let human = controls.read(&world, LOCAL_PLAYER);
        while bucket >= DT {
            let inputs: Vec<Input> = (0..PLAYER_COUNT)
                .map(|i| if i == LOCAL_PLAYER { human } else { bots[i].think(&world, i) })
                .collect();
            world.step(&inputs);
            bucket -= DT;
        }

        for event in world.drain_events() {
            effects.on_event(&event);
        }
        effects.update(frame);

        glow.begin();
        draw::draw_world(&world, get_time() as f32);
        effects.draw();
        draw::draw_hud(&world, LOCAL_PLAYER);
        glow.present();

        next_frame().await;
    }
}
