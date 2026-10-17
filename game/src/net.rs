// The wire between a client and a host.
//
// `Loopback` is a fake network that lives inside one process: two queues of
// bytes, one per direction. It behaves like a perfect connection (nothing is
// lost, reordered or delayed), which is also roughly what a WebSocket gives
// you: WebSocket runs over TCP, which is *reliable* (every message arrives)
// and *ordered* (in the order sent). The price of that comfort is that one
// delayed packet holds up everything behind it ("head-of-line blocking").
//
//   client_send ─ up ──> host_recv
//   client_recv <─ down ─ host_send
//
// Step 2 adds a second kind of wire (a real WebSocket through the relay) with
// the same four operations, so the code on either side will not care which one
// it is talking over.

use std::collections::VecDeque;

use protocol::{decode_client, decode_host, encode_client, encode_host, ClientMsg, HostMsg};

#[derive(Default)]
pub struct Loopback {
    up: VecDeque<Vec<u8>>,
    down: VecDeque<Vec<u8>>,
    /// Running totals, so the HUD can show what the game costs in bandwidth.
    pub bytes_up: u64,
    pub bytes_down: u64,
}

impl Loopback {
    // ---- the client's end ----
    pub fn client_send(&mut self, msg: &ClientMsg) {
        let bytes = encode_client(msg);
        self.bytes_up += bytes.len() as u64;
        self.up.push_back(bytes);
    }

    pub fn client_recv(&mut self) -> Option<HostMsg> {
        // Garbage is skipped, never fatal (see `protocol::decode_host`).
        while let Some(bytes) = self.down.pop_front() {
            if let Some(msg) = decode_host(&bytes) {
                return Some(msg);
            }
        }
        None
    }

    // ---- the host's end ----
    pub fn host_send(&mut self, msg: &HostMsg) {
        let bytes = encode_host(msg);
        self.bytes_down += bytes.len() as u64;
        self.down.push_back(bytes);
    }

    pub fn host_recv(&mut self) -> Option<ClientMsg> {
        while let Some(bytes) = self.up.pop_front() {
            if let Some(msg) = decode_client(&bytes) {
                return Some(msg);
            }
        }
        None
    }
}

/// Turns a growing byte total into "bytes per second", refreshed once a second.
#[derive(Default)]
pub struct RateMeter {
    last_time: f64,
    last_total: u64,
    pub per_second: f32,
}

impl RateMeter {
    pub fn update(&mut self, now: f64, total: u64) {
        if now - self.last_time >= 1.0 {
            self.per_second = (total - self.last_total) as f32 / (now - self.last_time) as f32;
            self.last_time = now;
            self.last_total = total;
        }
    }
}
