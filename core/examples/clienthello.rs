//! Emit one TLS-SRP ClientHello record (hex) for the live BLE probe.
//! Uses the real core framing: record(0x16,ver,len) + handshake(1,len) + body.
//! By default reproduces the exact ClientHello the real app sent over SPP
//! (record version 0x0301 + the captured 32-byte random), so the only variable
//! left in a BLE probe is the transport. Pass "fresh" for a zero-filled random.
use openplug_core::content_type::HANDSHAKE;
use openplug_core::tls_handshake::{build_client_hello, hs_wrap};
use openplug_core::SRP_IDENTITY_PIN;

fn main() {
    // 32-byte random captured from the real DataPlug app's ClientHello (SPP).
    let real_random: [u8; 32] = [
        0x6a, 0xf6, 0x29, 0x3d, 0xb8, 0xd3, 0xbc, 0x55, 0x93, 0x20, 0xe1, 0x59, 0x21, 0x06, 0x7c,
        0x44, 0xca, 0x9d, 0x6f, 0x50, 0xbc, 0x16, 0x9e, 0x61, 0xbb, 0x2f, 0x03, 0x21, 0x2f, 0xd8,
        0xc5, 0x39,
    ];
    let client_random = if std::env::args().any(|a| a == "fresh") {
        [0u8; 32]
    } else {
        real_random
    };
    let body = build_client_hello(&client_random, SRP_IDENTITY_PIN.as_bytes());
    let hs = hs_wrap(1, &body); // HT_CLIENT_HELLO = 1
    // Record version 0x0301 on the initial ClientHello, matching the real app.
    let mut rec = vec![HANDSHAKE, 0x03, 0x01, (hs.len() >> 8) as u8, hs.len() as u8];
    rec.extend_from_slice(&hs);
    println!("{}", rec.iter().map(|b| format!("{:02x}", b)).collect::<String>());
    eprintln!("# {} bytes, identity={}", rec.len(), SRP_IDENTITY_PIN);
}
