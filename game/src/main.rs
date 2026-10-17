// The game executable: glue between the platform (window, input devices,
// display) and the match. It owns no game rules.
//
//   devices -> controls -> Input ──[wire]──> Host: bots + World::step (60 Hz)
//                                                      │
//   draw / effects <── Replica (the client's copy) <──[wire]── Snapshot
//
// Everything left of the host produces Commands; everything right of it only
// reads. For an offline game the host runs in this same process (see
// local.rs); online, the wire leads to another machine and nothing on this
// side changes.

mod controls;
mod draw;
mod effects;
mod lobby;
mod local;
mod menu;
mod net;
mod shaders;
mod socket;
mod view;

use controls::Controls;
use effects::Effects;
use lobby::Lobby;
use local::LocalGame;
use macroquad::prelude::*;
use menu::{Choice, Menu};
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

/// Which screen the game is on.
///
/// Pattern: State. Each screen owns exactly the data it needs and nothing
/// else, and the main loop just asks the current one to run a frame.
enum Screen {
    Menu(Menu),
    Lobby(Lobby),
    Offline(Offline),
}

/// An offline match plus the per-frame bookkeeping around it.
struct Offline {
    game: LocalGame,
    effects: Effects,
    /// Real time not yet spent on sim ticks (see the Game Loop note below).
    bucket: f32,
    up_rate: RateMeter,
    down_rate: RateMeter,
}

impl Offline {
    fn new() -> Offline {
        Offline {
            game: LocalGame::new((macroquad::miniquad::date::now() * 1000.0) as u32),
            effects: Effects::default(),
            bucket: 0.0,
            up_rate: RateMeter::default(),
            down_rate: RateMeter::default(),
        }
    }

    fn frame(&mut self, controls: &mut Controls, glow: &view::Glow) {
        let frame = get_frame_time();

        // Pattern: Game Loop. macroquad owns the outer loop (`next_frame().await`).
        // Inside it we run the sim on a fixed 1/60 s step using an accumulator:
        // real elapsed time is added to a bucket, and every time the bucket
        // holds a full tick we spend it on one host tick. A slow frame runs
        // several ticks to catch up; a fast frame may run none. Drawing
        // happens once per frame regardless, at whatever rate the monitor runs.
        //
        // Cap the catch-up: after a long stall (e.g. a background tab) we skip
        // time instead of freezing while we simulate seconds of ticks.
        self.bucket += frame.min(0.1);

        // Client side: read the devices and send the wishes to the host.
        let me = self.game.replica.me as usize;
        self.game.send_input(controls.read(&self.game.replica.world, me));

        // Host side: tick as many times as real time demands.
        while self.bucket >= DT {
            self.game.host_tick();
            self.bucket -= DT;
        }

        // Client side again: take in what the host said, then show it.
        self.game.receive();
        for event in self.game.replica.drain_events() {
            self.effects.on_event(&event);
        }
        self.effects.update(frame);
        self.up_rate.update(get_time(), self.game.bytes_up());
        self.down_rate.update(get_time(), self.game.bytes_down());

        glow.begin();
        draw::draw_world(&self.game.replica.world, get_time() as f32);
        self.effects.draw();
        draw::draw_hud(&self.game.replica.world, me);
        draw::draw_net_stats(self.down_rate.per_second, self.up_rate.per_second);
        glow.present();
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let glow = view::Glow::new();
    let mut controls = Controls::new();
    let mut screen = Screen::Menu(Menu::new());

    loop {
        // Esc leaves any screen for the menu (the menu itself uses Esc to
        // back out of typing a code). Dropping a `Lobby` closes its socket.
        if is_key_pressed(KeyCode::Escape) && !matches!(screen, Screen::Menu(_)) {
            screen = Screen::Menu(Menu::new());
        }

        // A screen can ask to switch to another one; we do it after its frame
        // so we never replace a screen while it is still borrowed.
        let mut next = None;
        match &mut screen {
            Screen::Menu(menu) => {
                next = menu.update().map(|choice| match choice {
                    Choice::Offline => Screen::Offline(Offline::new()),
                    Choice::Host => Screen::Lobby(Lobby::host()),
                    Choice::Join(code) => Screen::Lobby(Lobby::join(code)),
                });
                glow.begin();
                menu.draw();
                glow.present();
            }
            Screen::Lobby(lobby) => {
                lobby.update(get_time());
                glow.begin();
                lobby.draw();
                glow.present();
            }
            Screen::Offline(offline) => {
                if is_key_pressed(KeyCode::R) {
                    *offline = Offline::new();
                }
                offline.frame(&mut controls, &glow);
            }
        }
        if let Some(next) = next {
            screen = next;
        }

        next_frame().await;
    }
}
