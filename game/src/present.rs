// Turns a `Replica` into pixels. Both ways of playing (hosting/offline and
// joining) end here: they differ in where the Replica's data comes from, never
// in how it is shown. That is the payoff of "clients only draw a Replica".

use macroquad::prelude::*;
use protocol::replica::Replica;

use crate::draw;
use crate::effects::Effects;
use crate::net::RateMeter;
use crate::view::Glow;

#[derive(Default)]
pub struct Presenter {
    effects: Effects,
    up_rate: RateMeter,
    down_rate: RateMeter,
}

impl Presenter {
    /// Draw one frame. `up`/`down` are running byte totals, `banner` a line of
    /// status for the top left, `can_restart` whether this player may press R.
    pub fn show(&mut self, replica: &mut Replica, up: u64, down: u64, banner: &str, can_restart: bool, glow: &Glow) {
        let frame = get_frame_time();
        for event in replica.drain_events() {
            self.effects.on_event(&event);
        }
        self.effects.update(frame);
        self.up_rate.update(get_time(), up);
        self.down_rate.update(get_time(), down);

        glow.begin();
        draw::draw_world(&replica.world, get_time() as f32);
        self.effects.draw();
        draw::draw_hud(&replica.world, replica.me as usize, can_restart);
        draw::draw_net_stats(self.down_rate.per_second, self.up_rate.per_second);
        draw_text(banner, 16.0, 26.0, 24.0, Color::new(0.1, 1.0, 0.95, 1.0));
        glow.present();
    }
}
