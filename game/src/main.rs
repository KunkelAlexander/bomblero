// The game executable: glue between the platform (window, input devices,
// display) and the match. It owns no game rules.
//
//   devices -> controls -> Input ──[wire]──> Host: bots + World::step (60 Hz)
//                                                      │
//   draw / effects <── Replica (the client's copy) <──[wire]── Snapshot
//
// Everything left of the host produces Commands; everything right of it only
// reads. Right now the host runs in this same process (see local.rs); online,
// the wire will lead to another machine and nothing on this side changes.

mod controls;
mod draw;
mod effects;
mod local;
mod net;
mod shaders;
mod view;

use local::LocalGame;
use macroquad::prelude::*;
use net::RateMeter;
use sim::DT;

fn window_conf() -> Conf {
    Conf {
        window_title: "Neon Bomber".to_owned(),
        window_width: view::WIDTH as i32,
        window_height: view::HEIGHT as i32,
        window_resizable: false,
        ..Default::default()
    }
}

fn new_match() -> LocalGame {
    LocalGame::new((macroquad::miniquad::date::now() * 1000.0) as u32)
}

#[macroquad::main(window_conf)]
async fn main() {
    let glow = view::Glow::new();
    let mut controls = controls::Controls::new();
    let mut effects = effects::Effects::default();
    let mut game = new_match();
    let (mut up_rate, mut down_rate) = (RateMeter::default(), RateMeter::default());

    // Pattern: Game Loop. macroquad owns the outer loop (`next_frame().await`).
    // Inside it we run the sim on a fixed 1/60 s step using an accumulator:
    // real elapsed time is added to a bucket, and every time the bucket holds a
    // full tick we spend it on one host tick. A slow frame runs several ticks
    // to catch up; a fast frame may run none. Drawing happens once per frame
    // regardless, at whatever rate the monitor runs.
    let mut bucket = 0.0;

    loop {
        if is_key_pressed(KeyCode::R) {
            game = new_match();
        }

        let frame = get_frame_time();
        // Cap the catch-up: after a long stall (e.g. a background tab) we skip
        // time instead of freezing while we simulate seconds of ticks.
        bucket += frame.min(0.1);

        // Client side: read the devices and send the wishes to the host.
        let me = game.replica.me as usize;
        let input = controls.read(&game.replica.world, me);
        game.send_input(input);

        // Host side: tick as many times as real time demands.
        while bucket >= DT {
            game.host_tick();
            bucket -= DT;
        }

        // Client side again: take in what the host said, then show it.
        game.receive();
        for event in game.replica.drain_events() {
            effects.on_event(&event);
        }
        effects.update(frame);
        up_rate.update(get_time(), game.bytes_up());
        down_rate.update(get_time(), game.bytes_down());

        glow.begin();
        draw::draw_world(&game.replica.world, get_time() as f32);
        effects.draw();
        draw::draw_hud(&game.replica.world, me);
        draw::draw_net_stats(down_rate.per_second, up_rate.per_second);
        glow.present();

        next_frame().await;
    }
}
