// End-to-end: a real relay on a real port, two real WebSocket clients.
// The unit tests in rooms.rs cover the logic; this proves the network shell
// around it (frames in, frames out, disconnect handling) works too.

use std::net::TcpStream;
use std::time::Duration;

use protocol::{
    decode_from_relay, decode_host, encode_client, encode_to_relay, ClientMsg, FromRelay, HostMsg, Reject, Rules, ToRelay,
};
use sim::Input;
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

/// Wait for the next game message from the server's match.
fn hear_host(ws: &mut Client) -> HostMsg {
    loop {
        if let FromRelay::Data { from: 0, bytes } = hear(ws) {
            return decode_host(&bytes).expect("a host message");
        }
    }
}

#[test]
fn the_server_hosts_a_match_and_ends_it_when_everyone_has_left() {
    let port = start_relay();
    let (mut creator, mut friend) = (client(port), client(port));

    // The creator asks for a server-hosted room (with a nonsense map number,
    // which the server must tame rather than crash on) and is seated in it.
    say(&mut creator, ToRelay::CreateHostedRoom { rules: Rules { map: 200, score_limit: 5 } });
    let FromRelay::RoomCreated { code } = hear(&mut creator) else { panic!("no room") };
    assert!(matches!(hear(&mut creator), FromRelay::RoomJoined));
    let HostMsg::Welcome(welcome) = hear_host(&mut creator) else { panic!("no Welcome") };
    assert_eq!(welcome.you, 0);
    assert!((welcome.rules.map as usize) < sim::tilemap::MAPS.len());

    // Snapshots follow, and the creator's input reaches the server's host.
    say(&mut creator, ToRelay::Data { to: None, bytes: encode_client(&ClientMsg::Input(Input::default())) });
    loop {
        if let HostMsg::Snapshot(s) = hear_host(&mut creator) {
            if s.tick > 3 {
                break;
            }
        }
    }

    // A friend joins by code like in any room and gets the next free seat.
    say(&mut friend, ToRelay::JoinRoom { code: code.clone() });
    assert!(matches!(hear(&mut friend), FromRelay::RoomJoined));
    let HostMsg::Welcome(w) = hear_host(&mut friend) else { panic!("no Welcome for the friend") };
    assert_eq!(w.you, 1);

    // The creator leaves: the match goes on for the friend...
    drop(creator);
    assert!(matches!(hear_host(&mut friend), HostMsg::Snapshot(_)));

    // ...and when the friend leaves too, the server closes the room.
    drop(friend);
    let mut late = client(port);
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(50));
        say(&mut late, ToRelay::JoinRoom { code: code.clone() });
        if matches!(hear(&mut late), FromRelay::Rejected(Reject::NoSuchRoom)) {
            return;
        }
    }
    panic!("the room is still open");
}
