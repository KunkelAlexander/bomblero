// A WebSocket the game can poll once per frame: `connect`, `state`, `send`, `recv`.
//
// A game loop must never wait on the network, so nothing here blocks:
//   * `connect` returns at once and the state reads `Connecting` until the
//     handshake finishes (a sleeping free-tier server can take a minute);
//   * `recv` returns `None` when nothing has arrived yet;
//   * a failed or dropped connection is just the state `Closed`. It never panics.
//
// Two implementations behind the same four calls, chosen at compile time:
//   * browser: the browser's own WebSocket, driven by web/net.js;
//   * native:  a background thread running `tungstenite`, talking to the main
//              thread through channels.
// (We wrote this instead of using the quad-net crate because quad-net panics
// on a failed connection and cannot tell you a connection has closed.)

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SocketState {
    Connecting,
    Open,
    Closed,
}

#[cfg(target_arch = "wasm32")]
pub use browser::Socket;
#[cfg(not(target_arch = "wasm32"))]
pub use native::Socket;

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::SocketState;

    // Implemented in JavaScript (web/net.js). The browser keeps one socket at
    // a time, which is all the game needs.
    extern "C" {
        fn net_connect(url: *const u8, len: usize);
        fn net_state() -> i32;
        fn net_send(bytes: *const u8, len: usize);
        /// Copies the oldest unread message into `buf`; returns its length,
        /// -1 if there is none, -2 if it did not fit.
        fn net_recv(buf: *mut u8, cap: usize) -> i32;
        fn net_close();
    }

    /// Matches `MAX_MESSAGE_BYTES` in the protocol crate, plus room for the
    /// relay's own wrapper around a message.
    const RECV_BUFFER: usize = 80 * 1024;

    pub struct Socket {
        buf: Vec<u8>,
    }

    impl Socket {
        pub fn connect(url: &str) -> Socket {
            unsafe { net_connect(url.as_ptr(), url.len()) };
            Socket { buf: vec![0; RECV_BUFFER] }
        }

        pub fn state(&self) -> SocketState {
            match unsafe { net_state() } {
                0 => SocketState::Connecting,
                1 => SocketState::Open,
                _ => SocketState::Closed,
            }
        }

        pub fn send(&mut self, bytes: &[u8]) {
            unsafe { net_send(bytes.as_ptr(), bytes.len()) };
        }

        pub fn recv(&mut self) -> Option<Vec<u8>> {
            loop {
                let n = unsafe { net_recv(self.buf.as_mut_ptr(), self.buf.len()) };
                match n {
                    -1 => return None,
                    -2 => continue, // oversized message: it was dropped, look at the next
                    n => return Some(self.buf[..n as usize].to_vec()),
                }
            }
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            unsafe { net_close() };
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::Arc;
    use std::time::Duration;

    use tungstenite::stream::MaybeTlsStream;
    use tungstenite::{Error, Message};

    use super::SocketState;

    const CONNECTING: u8 = 0;
    const OPEN: u8 = 1;
    const CLOSED: u8 = 2;

    pub struct Socket {
        state: Arc<AtomicU8>,
        stop: Arc<AtomicBool>,
        to_net: Sender<Vec<u8>>,
        from_net: Receiver<Vec<u8>>,
    }

    impl Socket {
        pub fn connect(url: &str) -> Socket {
            let state = Arc::new(AtomicU8::new(CONNECTING));
            let stop = Arc::new(AtomicBool::new(false));
            let (to_net, outgoing) = channel();
            let (incoming, from_net) = channel();
            let (url, state2, stop2) = (url.to_string(), state.clone(), stop.clone());
            std::thread::spawn(move || {
                run(&url, &outgoing, &incoming, &stop2, &state2);
                state2.store(CLOSED, Ordering::SeqCst);
            });
            Socket { state, stop, to_net, from_net }
        }

        pub fn state(&self) -> SocketState {
            match self.state.load(Ordering::SeqCst) {
                CONNECTING => SocketState::Connecting,
                OPEN => SocketState::Open,
                _ => SocketState::Closed,
            }
        }

        pub fn send(&mut self, bytes: &[u8]) {
            let _ = self.to_net.send(bytes.to_vec());
        }

        pub fn recv(&mut self) -> Option<Vec<u8>> {
            self.from_net.try_recv().ok()
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            // Ask the network thread to say goodbye and exit.
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    /// The network thread: shuttles bytes between the channels and the socket.
    fn run(url: &str, outgoing: &Receiver<Vec<u8>>, incoming: &Sender<Vec<u8>>, stop: &AtomicBool, state: &AtomicU8) {
        let Ok((mut ws, _)) = tungstenite::connect(url) else { return };
        if let MaybeTlsStream::Plain(tcp) = ws.get_mut() {
            // Send small messages immediately instead of waiting to batch them
            // (Nagle's algorithm): in a game, latency matters more than
            // packing efficiency.
            let _ = tcp.set_nodelay(true);
            // Make `read` give up after 5 ms so this one thread can also
            // send and watch for `stop`.
            let _ = tcp.set_read_timeout(Some(Duration::from_millis(5)));
        }
        state.store(OPEN, Ordering::SeqCst);

        loop {
            if stop.load(Ordering::SeqCst) {
                let _ = ws.close(None);
                let _ = ws.flush();
                return;
            }
            while let Ok(bytes) = outgoing.try_recv() {
                if ws.send(Message::Binary(bytes)).is_err() {
                    return;
                }
            }
            match ws.read() {
                Ok(Message::Binary(bytes)) => {
                    if incoming.send(bytes).is_err() {
                        return; // the Socket was dropped
                    }
                }
                Ok(Message::Close(_)) => return,
                Ok(_) => {}
                // No data within the 5 ms: not an error, go round again.
                Err(Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => return,
            }
        }
    }
}
