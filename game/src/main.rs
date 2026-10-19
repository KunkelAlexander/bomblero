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

mod armory;
mod controls;
mod draw;
mod effects;
mod hud;
mod ink;
mod join;
mod lights;
mod local;
mod menu;
mod net;
mod present;
mod setup;
mod share;
mod shaders;
mod socket;
mod terrain;
mod touch;
mod view;

use armory::Armory;
use controls::Controls;
use join::JoinGame;
use local::LocalGame;
use macroquad::prelude::*;
use menu::{Choice, Menu};
use present::Presenter;
use setup::{Settings, Setup};
use sim::{Input, DT};

fn window_conf() -> Conf {
    Conf {
        window_title: "Bomblero".to_owned(),
        window_width: view::WIDTH as i32,
        window_height: view::HEIGHT as i32,
        window_resizable: true,
        ..Default::default()
    }
}

/// Which screen the game is on.
///
/// Pattern: State. Each screen owns exactly the data it needs and nothing
/// else, and the main loop just asks the current one to run a frame.
enum Screen {
    Menu(Menu),
    /// Choosing the map and rules of a match you host (not one you join).
    Setup(Setup),
    /// Choosing weapons, on the way from the menu to a match.
    Armory(Armory),
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
    fn new(open_room: bool, loadout: &[u8], settings: Settings) -> HostGame {
        // A room for friends always has every seat; offline, you and your bots.
        let slots = if open_room { local::PLAYER_COUNT } else { 1 + settings.bots };
        let mut game = LocalGame::new(new_seed(), loadout, slots, settings.rules);
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
        // On a touchscreen, a tap after the match is over stands in for R.
        let tapped = self.game.replica.world.winner.is_some() && touch::pressed();
        if is_key_pressed(KeyCode::R) || tapped {
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
    // What the player carried last time: the picker starts from it.
    let mut loadout = sim::weapons::DEFAULT_LOADOUT.to_vec();
    // The same for the match setup.
    let mut settings = Settings::default();

    loop {
        // Esc leaves any screen for the menu (the menu itself uses Esc to
        // back out of typing a code). Dropping a screen closes its socket.
        controls.touch.update();
        if (is_key_pressed(KeyCode::Escape) || touch::back_pressed()) && !matches!(screen, Screen::Menu(_)) {
            screen = Screen::Menu(Menu::new());
        }

        // A screen can ask to switch to another one; we do it after its frame
        // so we never replace a screen while it is still borrowed.
        let mut next = None;
        match &mut screen {
            Screen::Menu(menu) => {
                // Every way into a match goes through the weapon picker; a match
                // you host first through the setup (a joiner gets the host's).
                next = menu.frame(&mut controls, &glow).map(|choice| match choice {
                    Choice::Join(_) => Screen::Armory(Armory::new(choice, loadout.clone())),
                    _ => Screen::Setup(Setup::new(choice, settings)),
                });
            }
            Screen::Setup(setup) => {
                if let Some((choice, chosen)) = setup.frame(&glow) {
                    settings = chosen;
                    next = Some(Screen::Armory(Armory::new(choice, loadout.clone())));
                }
            }
            Screen::Armory(armory) => {
                if let Some((choice, picked)) = armory.frame(&glow) {
                    next = Some(match choice {
                        Choice::Offline => Screen::Hosting(HostGame::new(false, &picked, settings)),
                        Choice::Host => Screen::Hosting(HostGame::new(true, &picked, settings)),
                        Choice::Join(code) => Screen::Joined(JoinGame::new(code, &picked)),
                    });
                    loadout = picked;
                }
            }
            Screen::Hosting(host_game) => host_game.frame(&mut controls, &glow),
            Screen::Joined(join_game) => join_game.frame(&mut controls, &glow),
        }
        if let Some(next) = next {
            screen = next;
        }

        // Touch controls go on top of the finished picture, in screen pixels.
        set_default_camera();
        if touch::active() {
            // Stick hints only where the sticks do something.
            let playing = match &screen {
                Screen::Menu(menu) => menu.walkable(),
                Screen::Setup(_) | Screen::Armory(_) => false,
                Screen::Hosting(_) | Screen::Joined(_) => true,
            };
            controls.touch.draw(playing);
            if !matches!(screen, Screen::Menu(_)) {
                touch::draw_back_button();
            }
        }
        // A phone held upright gets a hint to turn it, over everything else.
        if touch::wants_landscape() {
            touch::draw_rotate_hint();
        }

        next_frame().await;
    }
}
