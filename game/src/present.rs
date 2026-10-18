// Turns a `Replica` into pixels. Both ways of playing (hosting/offline and
// joining) end here: they differ in where the Replica's data comes from, never
// in how it is shown. That is the payoff of "clients only draw a Replica".
//
// A frame is drawn in layers, back to front:
//   floor():   ground, bricks, lights      (lit: the lightmap darkens them)
//   ...        anything the caller wants on the floor (the menu's pads and title)
//   actors():  projectiles, players, effects (unlit: they make their own light)
//   hud
// `show` does all of it for a match; the menu calls the pieces itself.

use macroquad::prelude::*;
use protocol::replica::Replica;
use sim::World;

use crate::draw;
use crate::effects::Effects;
use crate::hud;
use crate::lights::{self, Light, Lights};
use crate::net::RateMeter;
use crate::terrain::Terrain;
use crate::view::{self, Glow};

#[derive(Default)]
pub struct Presenter {
    effects: Effects,
    terrain: Terrain,
    lights: Option<Lights>,
    /// Smoothed time between frames, shown in the HUD so a slow GPU is visible.
    frame_ms: f32,
    up_rate: RateMeter,
    down_rate: RateMeter,
}

impl Presenter {
    /// Draw one frame. `up`/`down` are running byte totals, `banner` a line of
    /// status for the top left, `can_restart` whether this player may press R.
    pub fn show(&mut self, replica: &mut Replica, up: u64, down: u64, banner: &str, can_restart: bool, glow: &Glow) {
        self.up_rate.update(get_time(), up);
        self.down_rate.update(get_time(), down);

        self.floor(replica, &[], glow);
        self.actors(&replica.world);
        glow.overlay();
        hud::draw(&replica.world, replica.me as usize, can_restart, banner, self.down_rate.per_second, self.up_rate.per_second, self.frame_ms);
        glow.present();
    }

    /// Start the frame and draw the lit ground. `extra` are lights that do not
    /// come from the world (the menu's pads).
    pub fn floor(&mut self, replica: &mut Replica, extra: &[Light], glow: &Glow) {
        let frame = get_frame_time();
        // Move the replica's playhead on by one frame of real time (see
        // `Replica`), then show what is due.
        replica.advance(frame);
        for event in replica.drain_events() {
            self.effects.on_event(&event);
        }
        self.effects.update(frame);

        self.terrain.update(&replica.world.map);
        self.frame_ms += (frame * 1000.0 - self.frame_ms) * 0.05;

        let mut lights = Vec::new();
        lights::from_world(&replica.world, &mut lights);
        self.effects.lights(&mut lights);
        lights.extend_from_slice(extra);

        let me = &replica.world.players[replica.me as usize];
        view::follow(me.pos, me.aim);
        glow.begin();
        self.terrain.draw(&replica.world.map, &lights);
        // Created on the first frame: it needs the graphics context.
        self.lights.get_or_insert_with(Lights::new).apply(glow, &replica.world.map, &lights, self.terrain.shine());
    }

    /// The things that make their own light: drawn over the lit ground.
    pub fn actors(&mut self, world: &World) {
        draw::draw_world(world, get_time() as f32);
        self.effects.draw();
    }
}
