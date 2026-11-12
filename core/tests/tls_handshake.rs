//! Offline testy TLS handshake: PRF vektor, parser ServerKeyExchange proti
//! REÁLNÝM bajtům dongle (frame 312), a plný crypto loopback. Bez dongle.

use num_bigint::BigUint;
use openplug_core::content_type::APPLICATION_DATA;
use openplug_core::groups::RFC5054_1536;
use openplug_core::srp::Srp;
use openplug_core::tls_handshake::{
    build_client_key_exchange, handshake_body, key_block, master_secret, parse_server_key_exchange,
};
use openplug_core::tls_prf::prf;
use openplug_core::tls_record::{open, seal};

fn bn(h: &str) -> BigUint { BigUint::parse_bytes(h.as_bytes(), 16).unwrap() }

// Reálný ServerKeyExchange z našeho HCI záznamu (captures/dataplug-pairing...).
const SKE_RECORD_HEX: &str = "16030301ac0c0001a800c09def3cafb939277ab1f12a8617a47bbbdba51df499ac4c80beeea9614b19cc4d5f4f5f556e27cbde51c6a94be4607a291558903ba0d0f84380b655bb9a22e8dcdf028a7cec67f0d08134b1c8b97989149b609e0be3bab63d47548381dbc5b1fc764e3f4b53dd9da1158bfd3e2b9c8cf56edf019539349627db2fd53d24b7c48665772e437d6c7f8ce442734af7ccb7ae837c264ae3a9beb87f8a2fe9b8b5292e5a021fff5e91479e8ce7a28c2442c6f315180f93499a234dcf76e3fed135f9bb000102201ddb115a83d11d50171dc1bb4c39f15e643c0fb869b953cc2f28d5dfa7c4f2f000c09398f3fd444c8929fc5889e6e04899ec53caa9fc86a67b39d092e1d917520311fae326d2e954d0a3911bfda776982f365630f65f1f8a4b2fd05a3ce624217c1c66ba1db581d5e83061abc2c208b2048b501fb3b95a6c40d1b2f43454c61612036b9af3a27a3b9da7b66740a9c97b40ca4200195c81da44cdf321b97080bcf0206c10f52cefab8792726cf1f5c7ef91ccfb70821705bd9babd36a44b736e2a73f8a7b2d95de851a08121b397548f471c73437bc5975bcc9152f35bc532cb059f8";

#[test]
fn prf_sha256_known_answer() {
    // Standardní TLS 1.2 PRF-SHA256 test vektor.
    let secret = hex::decode("9bbe436ba940f017b17652849a71db35").unwrap();
    let seed = hex::decode("a0ba9f936cda311827a6f796ffd5198c").unwrap();
    let expected = hex::decode("e3f229ba727be17b8d122620557cd453c2aab21d07c3d495329b52d4e61edb5a6b301791e90d35c9c9a46b4e14baf9af0fa022f7077def17abfd3797c0564bab4fbc91666e9def9b97fce34f796789baa48082d122ee42c5a72e5a5110fff70187347b66").unwrap();
    let out = prf(&secret, b"test label", &seed, expected.len());
    assert_eq!(out, expected, "PRF-SHA256 nesouhlasí se standardním vektorem");
}

#[test]
fn parse_real_dongle_server_key_exchange() {
    let rec = hex::decode(SKE_RECORD_HEX).unwrap();
    // TLS record: type(1) ver(2) len(2) | handshake...
    assert_eq!(rec[0], 22, "content type = handshake");
    let hs = &rec[5..];
    let (htype, body) = handshake_body(hs).expect("handshake body");
    assert_eq!(htype, 12, "ServerKeyExchange");
    let p = parse_server_key_exchange(body).expect("SRP params");
    // ukotveno na reálná data: RFC5054-1536 skupina, g=2, salt 32 B, B 192 B
    assert_eq!(p.g, BigUint::from(2u32));
    assert_eq!(p.n.to_bytes_be().len(), 192, "N má 1536 bitů");
    assert!(p.n.to_bytes_be().starts_with(&[0x9d, 0xef, 0x3c, 0xaf]), "N = RFC5054-1536");
    assert_eq!(p.salt.len(), 32);
    assert_eq!(p.b_pub.to_bytes_be().len(), 192, "B má 1536 bitů");
    // N z capture se musí shodovat s naší zabudovanou skupinou
    let srp = Srp::new(&RFC5054_1536);
    assert_eq!(p.n, srp.n, "N z dongle == naše RFC5054-1536");
}

#[test]
fn full_crypto_pipeline_loopback() {
    // SRP (jako klient i server lokálně) -> premaster -> master -> klíče -> record.
    let srp = Srp::new(&RFC5054_1536);
    let identity = b"PA";
    let password = b"1234";
    let salt = hex::decode("1DDB115A83D11D50171DC1BB4C39F15E643C0FB869B953CC2F28D5DFA7C4F2F0").unwrap();
    let a = bn("A1B2C3D4E5F600112233445566778899AABBCCDDEEFF00112233445566778899");
    let b = bn("0099887766554433221100FFEEDDCCBBAA998877665544332211009988776655");

    let x = srp.x(&salt, identity, password);
    let v = srp.verifier(&x);
    let a_pub = srp.client_public(&a);
    let b_pub = srp.server_public(&v, &b);
    let u = srp.u(&a_pub, &b_pub);
    let s_client = srp.client_premaster(&b_pub, &x, &a, &u);
    let s_server = srp.server_premaster(&a_pub, &v, &u, &b);
    assert_eq!(s_client, s_server);
    let premaster = s_client.to_bytes_be();

    let client_random = [0xAAu8; 32];
    let server_random = [0xBBu8; 32];
    let ms = master_secret(&premaster, &client_random, &server_random);
    let (client_w, server_w) = key_block(&ms, &client_random, &server_random);

    // klient zašifruje app data, server (stejné client_write klíče) je přečte
    let iv = [0x01u8; 16];
    let msg = b"subscribe RPM; battery voltage";
    let rec = seal(&client_w, 0, APPLICATION_DATA, msg, &iv);
    let got = open(&client_w, 0, APPLICATION_DATA, &rec).expect("server čte klientův záznam");
    assert_eq!(&got, msg);

    // opačný směr: server -> klient
    let iv2 = [0x02u8; 16];
    let rsp = b"RPM=0; battery=12.4V";
    let rec2 = seal(&server_w, 0, APPLICATION_DATA, rsp, &iv2);
    let got2 = open(&server_w, 0, APPLICATION_DATA, &rec2).expect("klient čte serverův záznam");
    assert_eq!(&got2, rsp);

    // sanity: ClientKeyExchange se serializuje a začíná typem 16
    let cke = build_client_key_exchange(&a_pub);
    assert_eq!(cke[0], 16);
}
