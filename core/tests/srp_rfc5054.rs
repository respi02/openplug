//! Offline verification of SRP-6a against RFC 5054 Appendix B + a key-agreement
//! self-test. Runs without a dongle: `cargo test`.

use num_bigint::BigUint;
use openplug_core::groups::{RFC5054_1024, RFC5054_1536};
use openplug_core::srp::{valid_public, Srp};

fn bn(hex: &str) -> BigUint {
    BigUint::parse_bytes(hex.as_bytes(), 16).unwrap()
}

#[test]
fn rfc5054_appendix_b_vector() {
    // Published inputs (RFC 5054, Appendix B), 1024-bit group, SHA-1.
    let srp = Srp::new(&RFC5054_1024);
    let identity = b"alice";
    let password = b"password123";
    let salt = hex::decode("BEB25379D1A8581EB5A727673A2441EE").unwrap();
    let a = bn("60975527035CF2AD1989806F0407210BC81EDC04E2762A56AFD529DDDA2D4393");
    let b = bn("E487CB59D31AC550471E81F00F6928E01DDA08E974A004F49E61F5D105284D20");

    // Expected (short) values from the RFC — external anchor.
    let x_exp = bn("94B7555AABE9127CC58CCF4993DB6CF84D16C124");
    let u_exp = bn("CE38B9593487DA98554ED47D70A7AE5F462EF019");

    let x = srp.x(&salt, identity, password);
    assert_eq!(x, x_exp, "x does not match RFC 5054 B");

    let v = srp.verifier(&x);
    let a_pub = srp.client_public(&a);
    let b_pub = srp.server_public(&v, &b);
    assert!(valid_public(&a_pub, &srp.n));
    assert!(valid_public(&b_pub, &srp.n));

    let u = srp.u(&a_pub, &b_pub);
    assert_eq!(u, u_exp, "u does not match RFC 5054 B");

    // Client and server must arrive at the same premaster secret.
    let s_client = srp.client_premaster(&b_pub, &x, &a, &u);
    let s_server = srp.server_premaster(&a_pub, &v, &u, &b);
    assert_eq!(s_client, s_server, "premaster client != server");
}

#[test]
fn dongle_group_1536_roundtrip() {
    // The DataPlug group, identity "PA", a test PIN. Confirms our implementation
    // derives a matching key on the 1536-bit group (as the dongle uses).
    let srp = Srp::new(&RFC5054_1536);
    let identity = openplug_core::SRP_IDENTITY_PIN.as_bytes(); // "PA"
    let password = b"1234"; // test PIN (the real one is on the label)
    let salt = hex::decode("1DDB115A83D11D50171DC1BB4C39F15E643C0FB869B953CC2F28D5DFA7C4F2F0").unwrap();
    let a = bn("ABCDEF00112233445566778899AABBCCDDEEFF00112233445566778899AABBCC");
    let b = bn("1122334455667788990011223344556677889900112233445566778899001122");

    let x = srp.x(&salt, identity, password);
    let v = srp.verifier(&x);
    let a_pub = srp.client_public(&a);
    let b_pub = srp.server_public(&v, &b);
    assert!(valid_public(&a_pub, &srp.n) && valid_public(&b_pub, &srp.n));
    let u = srp.u(&a_pub, &b_pub);

    let s_client = srp.client_premaster(&b_pub, &x, &a, &u);
    let s_server = srp.server_premaster(&a_pub, &v, &u, &b);
    assert_eq!(s_client, s_server, "premaster client != server (1536-bit)");
    assert!(s_client.bits() > 1000, "premaster should be ~1536 bits");
}
