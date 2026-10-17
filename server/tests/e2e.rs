// End-to-end: a real relay on a real port, two real WebSocket clients.
// The unit tests in rooms.rs cover the logic; this proves the network shell
// around it (frames in, frames out, disconnect handling) works too.

use std::net::TcpStream;
use std::time::Duration;

use protocol::{decode_from_relay, encode_to_relay, FromRelay, ToRelay};
use tungstenite::{connect, stream::MaybeTlsStream, Message, WebSocket};

type Client = WebSocket<MaybeTlsStream<TcpStream>>;

fn start_relay() -> u16 {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = std_listener.local_addr().unwrap().port();
    std_listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            server::serve(listener).await;
        });
    });
    port
}

fn client(port: u16) -> Client {
    let (ws, _) = connect(format!("ws://127.0.0.1:{port}/ws")).expect("connect to relay");
    if let MaybeTlsStream::Plain(s) = ws.get_ref() {
        // Never hang the test forever if something is wrong.
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    }
    ws
}

fn say(ws: &mut Client, msg: ToRelay) {
    ws.send(Message::Binary(encode_to_relay(&msg))).unwrap();
}

fn hear(ws: &mut Client) -> FromRelay {
    loop {
        if let Message::Binary(bytes) = ws.read().expect("a message from the relay") {
            return decode_from_relay(&bytes).unwrap();
        }
    }
}

#[test]
fn two_clients_talk_through_a_real_relay() {
    let port = start_relay();
    let (mut host, mut friend) = (client(port), client(port));

    say(&mut host, ToRelay::CreateRoom);
    let FromRelay::RoomCreated { code } = hear(&mut host) else { panic!("no room") };

    say(&mut friend, ToRelay::JoinRoom { code });
    assert!(matches!(hear(&mut friend), FromRelay::RoomJoined));
    assert!(matches!(hear(&mut host), FromRelay::PeerJoined(1)));

    say(&mut friend, ToRelay::Data { to: None, bytes: b"hello host".to_vec() });
    match hear(&mut host) {
        FromRelay::Data { from: 1, bytes } => assert_eq!(bytes, b"hello host"),
        other => panic!("unexpected {other:?}"),
    }

    say(&mut host, ToRelay::Data { to: Some(1), bytes: b"hello friend".to_vec() });
    match hear(&mut friend) {
        FromRelay::Data { from: 0, bytes } => assert_eq!(bytes, b"hello friend"),
        other => panic!("unexpected {other:?}"),
    }

    // Closing the host's connection must close the room for the friend.
    drop(host);
    assert!(matches!(hear(&mut friend), FromRelay::RoomClosed));
}
