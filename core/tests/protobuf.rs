//! Tests for the generic protobuf codec. Offline.

use openplug_core::protobuf::{decode, inspect, Builder, Field, Value};

#[test]
fn decode_known_wire_bytes() {
    // field 1 varint 150  => 08 96 01  (the canonical protobuf example)
    assert_eq!(decode(&[0x08, 0x96, 0x01]).unwrap(),
               vec![Field { number: 1, value: Value::Varint(150) }]);
    // field 2 length-delim "testing" => 12 07 ...
    let d = decode(&[0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g']).unwrap();
    assert_eq!(d, vec![Field { number: 2, value: Value::Bytes(b"testing".to_vec()) }]);
}

#[test]
fn encode_decode_roundtrip() {
    let msg = Builder::default()
        .varint(1, 42)
        .bytes(2, b"vin")
        .message(3, Builder::default().varint(1, 7).varint(2, 12_400))
        .build();
    let fields = decode(&msg).unwrap();
    assert_eq!(fields[0], Field { number: 1, value: Value::Varint(42) });
    assert_eq!(fields[1], Field { number: 2, value: Value::Bytes(b"vin".to_vec()) });
    // nested message in field 3
    if let Value::Bytes(sub) = &fields[2].value {
        let subf = decode(sub).unwrap();
        assert_eq!(subf, vec![
            Field { number: 1, value: Value::Varint(7) },
            Field { number: 2, value: Value::Varint(12_400) },
        ]);
    } else { panic!("field 3 should be a message"); }
}

#[test]
fn inspect_shows_nested_and_strings() {
    let msg = Builder::default()
        .varint(1, 3)
        .bytes(2, b"TMBXX")
        .message(5, Builder::default().varint(1, 850))
        .build();
    let out = inspect(&msg);
    assert!(out.contains("#1 varint 3"));
    assert!(out.contains("#2 str \"TMBXX\""));
    assert!(out.contains("#5 msg {"));
    assert!(out.contains("#1 varint 850"));
}

#[test]
fn rejects_truncated() {
    // length-delim claims 5 bytes but only 2 are present
    assert!(decode(&[0x12, 0x05, 0x01, 0x02]).is_err());
}
