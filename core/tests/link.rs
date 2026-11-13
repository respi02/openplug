//! The read API end to end over the in-memory pipe: a scripted dongle on the
//! server side of a real TLS-SRP session, `DongleLink` on the client side.

use std::thread;

use num_bigint::BigUint;
use openplug_core::dongle::{build_response, parse_request, state, DataElement, Deframer, Request};
use openplug_core::groups::RFC5054_1536;
use openplug_core::link::DongleLink;
use openplug_core::tls_client::{server_handshake, HsError, ServerConfig, Session};
use openplug_core::transport::testpipe::{self, Half};

fn serve(half: Half, pin: &'static [u8]) -> Result<Session<Half>, HsError> {
    let cfg = ServerConfig {
        group: &RFC5054_1536,
        password: pin,
        salt: vec![0x5A; 32],
        server_random: [0xC3u8; 32],
        b: BigUint::parse_bytes(b"0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF", 16).unwrap(),
        session_id: vec![],
        finished_iv: [0x66u8; 16],
    };
    server_handshake(half, &cfg)
}

fn el(url: u32, value: f32) -> DataElement {
    DataElement { url, state: state::OK, value, precision: Some(1) }
}

#[test]
fn subscribe_then_stream_across_record_boundaries() {
    let (client_half, server_half) = testpipe::pair();
    let server = thread::spawn(move || {
        let mut sess = serve(server_half, b"1234").expect("server handshake");
        let mut de = Deframer::new();
        de.push(&sess.recv_app().expect("request"));
        let req = parse_request(&de.next_message().expect("one message")).expect("parse");
        assert_eq!(req, Request::Subscribe { param_id: 14, rate_ms: 1000, txid: 1 });

        let r1 = build_response(1, 14, 1, &[el(0x0004_000e, 6.9)]);
        let r2 = build_response(2, 14, 1, &[el(0x0004_000e, 7.1), el(0x0005_0001, 14.9)]);
        let r3 = build_response(3, 14, 1, &[el(0x0004_000e, 8.0)]);
        // r1 and r2 share one record; r3 is split over two.
        sess.send_app(&[r1, r2].concat()).unwrap();
        let (a, b) = r3.split_at(r3.len() / 2);
        sess.send_app(a).unwrap();
        sess.send_app(b).unwrap();
        sess.close().unwrap();
    });

    let mut link = DongleLink::connect(client_half, "1234").expect("connect");
    assert_eq!(link.subscribe(14, 1000).unwrap(), 1);

    let r1 = link.next_response().unwrap();
    assert_eq!((r1.seq, r1.transaction_id), (Some(1), Some(1)));
    assert_eq!(r1.elements, vec![el(0x0004_000e, 6.9)]);
    let r2 = link.next_response().unwrap();
    assert_eq!(r2.elements, vec![el(0x0004_000e, 7.1), el(0x0005_0001, 14.9)]);
    let r3 = link.next_response().unwrap();
    assert_eq!(r3.elements, vec![el(0x0004_000e, 8.0)]);

    // the dongle's close_notify ends the stream cleanly
    assert_eq!(link.next_response().err(), Some(HsError::Closed));
    server.join().unwrap();
}

#[test]
fn wrong_pin_fails_the_handshake() {
    let (client_half, server_half) = testpipe::pair();
    let server = thread::spawn(move || {
        assert!(serve(server_half, b"1234").is_err());
    });
    assert!(DongleLink::connect(client_half, "9999").is_err());
    server.join().unwrap();
}

#[test]
fn getter_requests_carry_incrementing_txids() {
    let (client_half, server_half) = testpipe::pair();
    let server = thread::spawn(move || {
        let mut sess = serve(server_half, b"1234").expect("server handshake");
        let mut de = Deframer::new();
        let mut seen = Vec::new();
        while seen.len() < 2 {
            de.push(&sess.recv_app().unwrap());
            while let Some(m) = de.next_message() {
                seen.push(parse_request(&m).unwrap());
            }
        }
        assert_eq!(seen, vec![
            Request::Other { field: 4, txid: 1 },
            Request::Other { field: 22, txid: 2 },
        ]);
    });
    let mut link = DongleLink::connect(client_half, "1234").expect("connect");
    assert_eq!(link.get(4).unwrap(), 1);
    assert_eq!(link.get(22).unwrap(), 2);
    server.join().unwrap();
}
