//! Full TLS-SRP handshake loopback: client vs server over an in-memory pipe,
//! session establishment and application-data exchange in both directions.
//! Offline, no dongle.

use std::thread;

use num_bigint::BigUint;
use openplug_core::groups::RFC5054_1536;
use openplug_core::tls_client::{client_handshake, server_handshake, ClientConfig, ServerConfig};
use openplug_core::transport::testpipe;

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
