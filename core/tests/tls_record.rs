//! Offline test of the TLS 1.2 record layer (AES-128-CBC + HMAC-SHA1). No dongle.

use openplug_core::content_type::APPLICATION_DATA;
use openplug_core::tls_record::{open, seal, DirKeys, RecordError};

fn keys() -> DirKeys {
    DirKeys {
        mac_key: [0x11; 20],
        enc_key: [0x22; 16],
    }
}

#[test]
fn seal_open_roundtrip_various_lengths() {
    let k = keys();
    let iv = [0x33u8; 16];
    for len in [0usize, 1, 5, 11, 16, 17, 31, 32, 100, 255] {
        let msg: Vec<u8> = (0..len).map(|i| i as u8).collect();
        let seq = 7;
        let rec = seal(&k, seq, APPLICATION_DATA, &msg, &iv);
        // EtM: IV(16) + ciphertext(multiple of 16) + MAC(20)
        assert_eq!((rec.len() - 16 - 20) % 16, 0);
        let back = open(&k, seq, APPLICATION_DATA, &rec).expect("open ok");
        assert_eq!(back, msg, "round-trip failed for len={len}");
    }
}

#[test]
fn tamper_is_detected() {
    let k = keys();
    let iv = [0x44u8; 16];
    let msg = b"RPM=850; battery=12.4V";
    let mut rec = seal(&k, 1, APPLICATION_DATA, msg, &iv);
    // flip one byte in the ciphertext → MAC or padding fails
    let last = rec.len() - 1;
    rec[last] ^= 0x01;
    assert!(matches!(open(&k, 1, APPLICATION_DATA, &rec),
                     Err(RecordError::BadMac) | Err(RecordError::BadPadding)));
}

#[test]
fn wrong_seq_breaks_mac() {
    let k = keys();
    let iv = [0x55u8; 16];
    let msg = b"hello dongle";
    let rec = seal(&k, 10, APPLICATION_DATA, msg, &iv);
    // opening with a different seq → MAC mismatch (seq is in the MAC input)
    assert!(matches!(open(&k, 11, APPLICATION_DATA, &rec), Err(RecordError::BadMac)));
    // the correct seq passes
    assert_eq!(open(&k, 10, APPLICATION_DATA, &rec).unwrap(), msg);
}
