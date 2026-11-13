//! Full TLS-SRP handshake loopback: client vs server over an in-memory pipe,
//! session establishment and application-data exchange in both directions.
//! Offline, no dongle.

use std::thread;

use num_bigint::BigUint;
use openplug_core::groups::RFC5054_1536;
use openplug_core::tls_client::{client_handshake, server_handshake, ClientConfig, HsError, ServerConfig};
use openplug_core::transport::{testpipe, Transport};

fn bn(h: &str) -> BigUint { BigUint::parse_bytes(h.as_bytes(), 16).unwrap() }

#[test]
fn full_tls_srp_handshake_loopback() {
    let (client_half, server_half) = testpipe::pair();

    let salt = hex::decode("1DDB115A83D11D50171DC1BB4C39F15E643C0FB869B953CC2F28D5DFA7C4F2F0").unwrap();
    let password = b"1234".to_vec();
    let salt_s = salt.clone();
    let pw_s = password.clone();

    // server on a thread (= dongle)
    let server = thread::spawn(move || {
        let cfg = ServerConfig {
            group: &RFC5054_1536,
            password: &pw_s,
            salt: salt_s,
            server_random: [0xB0u8; 32],
            b: bn("0099887766554433221100FFEEDDCCBBAA998877665544332211009988776655"),
            session_id: vec![0x11; 32],
            finished_iv: [0x77u8; 16],
        };
        let mut sess = server_handshake(server_half, &cfg).expect("server handshake");
        // read the client's request, respond (simulates reading data from the dongle)
        let req = sess.recv_app().expect("server recv");
        assert_eq!(req, b"subscribe: rpm, battery_voltage, coolant_temp");
        sess.send_app(b"rpm=0 battery=12.4V coolant=20C").expect("server send");
        // one more round
        let req2 = sess.recv_app().expect("server recv2");
        assert_eq!(req2, b"get: vin");
        sess.send_app(b"vin=TMBXXXXXXXXXXXXXX").expect("server send2");
    });

    // client on the main thread (= OpenPlug)
    let cfg = ClientConfig {
        group: &RFC5054_1536,
        identity: b"PA",
        password: &password,
        client_random: [0xA0u8; 32],
        a: bn("A1B2C3D4E5F600112233445566778899AABBCCDDEEFF00112233445566778899"),
        finished_iv: [0x66u8; 16],
    };
    let mut sess = client_handshake(client_half, &cfg).expect("client handshake");

    sess.send_app(b"subscribe: rpm, battery_voltage, coolant_temp").expect("client send");
    let rsp = sess.recv_app().expect("client recv");
    assert_eq!(rsp, b"rpm=0 battery=12.4V coolant=20C");

    sess.send_app(b"get: vin").expect("client send2");
    let rsp2 = sess.recv_app().expect("client recv2");
    assert_eq!(rsp2, b"vin=TMBXXXXXXXXXXXXXX");

    server.join().expect("server thread");
}

/// Production-style config (`ClientConfig::random`) against the test server.
#[test]
fn handshake_with_random_client_config() {
    let (client_half, server_half) = testpipe::pair();
    let server = thread::spawn(move || {
        let cfg = ServerConfig {
            group: &RFC5054_1536,
            password: b"1234",
            salt: vec![0x5A; 32],
            server_random: [0xB1u8; 32],
            b: bn("0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF"),
            session_id: vec![],
            finished_iv: [0x55u8; 16],
        };
        let mut sess = server_handshake(server_half, &cfg).expect("server handshake");
        let req = sess.recv_app().expect("server recv");
        sess.send_app(&req).expect("server echo");
    });
    let cfg = ClientConfig::random(&RFC5054_1536, b"PA", b"1234").expect("rng");
    let mut sess = client_handshake(client_half, &cfg).expect("client handshake");
    sess.send_app(b"ping").expect("send");
    assert_eq!(sess.recv_app().expect("recv"), b"ping");
    server.join().expect("server thread");
}

/// A peer that hangs up mid-handshake yields `Closed` instead of spinning forever.
#[test]
fn peer_close_is_reported_not_spun_on() {
    let (client_half, server_half) = testpipe::pair();
    drop(server_half);
    let cfg = ClientConfig::random(&RFC5054_1536, b"PA", b"1234").expect("rng");
    assert_eq!(client_handshake(client_half, &cfg).err(), Some(HsError::Closed));
}

/// Two records with identical plaintext must not share an explicit IV.
#[test]
fn app_records_use_fresh_random_ivs() {
    let (client_half, server_half) = testpipe::pair();
    let (tap_in, mut tap_out) = testpipe::pair(); // copy of what the server receives
    let server = thread::spawn(move || {
        let cfg = ServerConfig {
            group: &RFC5054_1536,
            password: b"1234",
            salt: vec![0x5A; 32],
            server_random: [0xB2u8; 32],
            b: bn("0F1E2D3C4B5A69788796A5B4C3D2E1F00F1E2D3C4B5A69788796A5B4C3D2E1F0"),
            session_id: vec![],
            finished_iv: [0x44u8; 16],
        };
        let mut sess = server_handshake(Tee { inner: server_half, tap: tap_in }, &cfg)
            .expect("server handshake");
        sess.recv_app().expect("r1");
        sess.recv_app().expect("r2");
        // dropping `sess` drops the Tee and closes the tap
    });
    let cfg = ClientConfig::random(&RFC5054_1536, b"PA", b"1234").expect("rng");
    let mut sess = client_handshake(client_half, &cfg).expect("client handshake");
    sess.send_app(b"same").unwrap();
    sess.send_app(b"same").unwrap();
    server.join().unwrap();

    // Collect everything the server read, split into records, take the last two.
    let mut all = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = tap_out.recv(&mut buf).unwrap();
        if n == 0 { break; }
        all.extend_from_slice(&buf[..n]);
    }
    let mut recs = Vec::new();
    let mut p = 0;
    while p + 5 <= all.len() {
        let len = u16::from_be_bytes([all[p + 3], all[p + 4]]) as usize;
        recs.push((all[p], all[p + 5..p + 5 + len].to_vec()));
        p += 5 + len;
    }
    let app: Vec<_> = recs.iter().filter(|(ct, _)| *ct == 23).collect();
    assert_eq!(app.len(), 2);
    assert_ne!(app[0].1[..16], app[1].1[..16], "explicit IVs must differ");
}

/// Transport wrapper that copies every received byte to a tap.
struct Tee<T: Transport> { inner: T, tap: testpipe::Half }
impl<T: Transport> Transport for Tee<T> {
    fn send(&mut self, d: &[u8]) -> std::io::Result<()> { self.inner.send(d) }
    fn recv(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.recv(b)?;
        self.tap.send(&b[..n])?;
        Ok(n)
    }
}
