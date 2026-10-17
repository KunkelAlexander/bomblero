// The game executable: glue between the platform (window, input devices,
// display) and the match. It owns no game rules.
//
//   devices -> controls -> Input ──[wire]──> Host: bots + World::step (60 Hz)
//                                                      │
//   draw / effects <── Replica (the client's copy) <──[wire]── Snapshot
//
// Everything left of the host produces Commands; everything right of it only
// reads. The host runs either in this very process (offline, or hosting for
// friends: local.rs + share.rs) or on a friend's machine (join.rs); the
// wire is a loopback in the first case and the relay in the second, and
// nothing else changes.

mod controls;
mod draw;
mod effects;
mod hud;
mod join;
mod lights;
mod local;
mod menu;
mod net;
mod present;
mod share;
mod shaders;
mod socket;
mod terrain;
mod view;

use controls::Controls;
use join::JoinGame;
use local::LocalGame;
use macroquad::prelude::*;
use menu::{Choice, Menu};
use present::Presenter;
use sim::{Input, DT};

fn window_conf() -> Conf {
    Conf {
        window_title: "Bomblero".to_owned(),
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
    /// A match whose host runs in this process: offline, or with a room open.
    Hosting(HostGame),
    /// A match hosted by a friend.
    Joined(JoinGame),
}

/// A hosted match plus the per-frame bookkeeping around it.
struct HostGame {
    game: LocalGame,
    presenter: Presenter,
    /// Real time not yet spent on sim ticks (see the Game Loop note below).
    bucket: f32,
}

impl HostGame {
    fn new(open_room: bool) -> HostGame {
        let mut game = LocalGame::new(new_seed());
        if open_room {
            game.open_room();
        }
        HostGame { game, presenter: Presenter::default(), bucket: 0.0 }
    }

    /// The menu's hub: a tiny match with one player and no bots (see `menu.rs`).
    fn menu() -> HostGame {
        HostGame { game: LocalGame::menu(new_seed()), presenter: Presenter::default(), bucket: 0.0 }
    }

    /// Everything except drawing: send the player's wishes to the host, let the
    /// host tick for as much real time as has passed, take in its answer.
    fn advance(&mut self, input: Input) {
        // Pattern: Game Loop. macroquad owns the outer loop (`next_frame().await`).
        // Inside it we run the sim on a fixed 1/60 s step using an accumulator:
        // real elapsed time is added to a bucket, and every time the bucket
        // holds a full tick we spend it on one host tick. A slow frame runs
        // several ticks to catch up; a fast frame may run none. Drawing
        // happens once per frame regardless, at whatever rate the monitor runs.
        //
        // Cap the catch-up: after a long stall (e.g. a background tab) we skip
        // time instead of freezing while we simulate seconds of ticks. Note
        // what that means for a hosting tab that is in the background: the
        // match runs in slow motion for everybody, because the host's clock
        // is the match's clock.
        self.bucket += get_frame_time().min(0.1);

        // Client side: send the wishes to the host.
        self.game.send_input(input);

        // Host side: tick as many times as real time demands.
        while self.bucket >= DT {
            self.game.host_tick();
            self.bucket -= DT;
        }

        // Client side again: take in what the host said.
        self.game.receive();
    }

    fn frame(&mut self, controls: &mut Controls, glow: &view::Glow) {
        if is_key_pressed(KeyCode::R) {
            self.game.restart(new_seed());
        }

        // Read the devices, run the match, then show it.
        let me = self.game.replica.me as usize;
        let input = controls.read(&self.game.replica.world, me);
        self.advance(input);
        let banner = self.game.room_status().unwrap_or_default();
        let (up, down) = (self.game.bytes_up(), self.game.bytes_down());
        self.presenter.show(&mut self.game.replica, up, down, &banner, true, glow);
    }
}

fn new_seed() -> u32 {
    (macroquad::miniquad::date::now() * 1000.0) as u32
}

#[macroquad::main(window_conf)]
async fn main() {
    let glow = view::Glow::new();
    let mut controls = Controls::new();
    let mut screen = Screen::Menu(Menu::new());

    loop {
        // Esc leaves any screen for the menu (the menu itself uses Esc to
        // back out of typing a code). Dropping a screen closes its socket.
        if is_key_pressed(KeyCode::Escape) && !matches!(screen, Screen::Menu(_)) {
            screen = Screen::Menu(Menu::new());
        }

        // A screen can ask to switch to another one; we do it after its frame
        // so we never replace a screen while it is still borrowed.
        let mut next = None;
        match &mut screen {
            Screen::Menu(menu) => {
                next = menu.frame(&mut controls, &glow).map(|choice| match choice {
                    Choice::Offline => Screen::Hosting(HostGame::new(false)),
                    Choice::Host => Screen::Hosting(HostGame::new(true)),
                    Choice::Join(code) => Screen::Joined(JoinGame::new(code)),
                });
            }
            Screen::Hosting(host_game) => host_game.frame(&mut controls, &glow),
            Screen::Joined(join_game) => join_game.frame(&mut controls, &glow),
        }
        if let Some(next) = next {
            screen = next;
        }

        next_frame().await;
    }
}
