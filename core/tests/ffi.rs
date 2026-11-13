//! The UniFFI facade driven from Rust, with the in-memory pipe standing in for the
//! app's Bluetooth transport.
#![cfg(feature = "ffi")]

use std::sync::{Arc, Mutex};
use std::thread;

use num_bigint::BigUint;
use openplug_core::dongle::{build_response, state, DataElement};
use openplug_core::ffi::{ByteTransport, Dongle, DongleError, TransportError};
use openplug_core::groups::RFC5054_1536;
use openplug_core::tls_client::{server_handshake, ServerConfig};
use openplug_core::transport::testpipe::{self, Half};
use openplug_core::transport::Transport;

struct Pipe(Mutex<Half>);

impl ByteTransport for Pipe {
    fn send(&self, data: Vec<u8>) -> Result<(), TransportError> {
        self.0.lock().unwrap().send(&data).map_err(|e| TransportError::Io(e.to_string()))
    }
    fn recv(&self, max_len: u32) -> Result<Vec<u8>, TransportError> {
        let mut buf = vec![0u8; max_len as usize];
        let n = self.0.lock().unwrap().recv(&mut buf).map_err(|e| TransportError::Io(e.to_string()))?;
        buf.truncate(n);
        Ok(buf)
    }
}

fn server_cfg() -> ServerConfig<'static> {
    ServerConfig {
        group: &RFC5054_1536,
        password: b"1234",
        salt: vec![0x5A; 32],
        server_random: [0xD4u8; 32],
        b: BigUint::parse_bytes(b"0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF", 16).unwrap(),
        session_id: vec![],
        finished_iv: [0x77u8; 16],
    }
}

#[test]
fn connect_subscribe_and_read_labelled_readings() {
    let (client_half, server_half) = testpipe::pair();
    let server = thread::spawn(move || {
        let mut sess = server_handshake(server_half, &server_cfg()).expect("server handshake");
        for _ in 0..17 {
            sess.recv_app().expect("subscribe");
        }
        let els = [
            DataElement { url: 0x0004_0000, state: state::OK, value: 768.0, precision: Some(0) },
            DataElement { url: 0x0010_0001, state: state::NODATA, value: 0.0, precision: Some(0) },
        ];
        sess.send_app(&build_response(1, 14, 1, &els)).unwrap();
        sess.close().unwrap();
    });

    let dongle = Dongle::connect(Arc::new(Pipe(Mutex::new(client_half))), "1234".into()).expect("connect");
    dongle.subscribe_defaults().unwrap();
    let r = dongle.next_readings().unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].name.as_deref(), r[0].unit.as_deref()), (Some("engine speed"), Some("rpm")));
    assert!(r[0].value == 768.0 && r[0].available && r[0].precision == 0);
    assert!(!r[1].available && r[1].name.is_none());
    assert_eq!(dongle.next_readings().err(), Some(DongleError::Closed));
    server.join().unwrap();
}

#[test]
fn wrong_pin_is_an_error() {
    let (client_half, server_half) = testpipe::pair();
    let server = thread::spawn(move || {
        assert!(server_handshake(server_half, &server_cfg()).is_err());
    });
    assert!(Dongle::connect(Arc::new(Pipe(Mutex::new(client_half))), "0000".into()).is_err());
    server.join().unwrap();
}
