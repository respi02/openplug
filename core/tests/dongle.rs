//! Application-protocol tests, checked against the vectors in
//! research/notes/live_protocol.md (clean-room: derived from the decoded spec).

use openplug_core::dongle::{
    build_subscribe, parse_response, state, Client, DataElement, Deframer,
};
use openplug_core::protobuf::Builder;

#[test]
fn subscribe_matches_spec_vector() {
    // live_protocol.md: Subscribe{mode:0, paramId:14, rateMs:1000}, txid 47
    //   -> 0d 82 01 07 08 00 10 0e 18 e8 07 f8 0f 2f
    let got = build_subscribe(14, 1000, 47);
    let want = hex::decode("0d820107080010 0e18e807f80f2f".replace(' ', "")).unwrap();
    assert_eq!(got, want);
}

#[test]
fn client_allocates_incrementing_txids() {
    let mut c = Client::new();
    let a = c.subscribe(14, 1000); // txid 1
    let b = c.subscribe(23, 1000); // txid 2
    assert_eq!(c.peek_txid(), 3);
    // the last byte is the (small) transaction id
    assert_eq!(*a.last().unwrap(), 1);
    assert_eq!(*b.last().unwrap(), 2);
}

/// Build a DataElement the way the dongle lays it out, for the parser test.
fn element(url: u32, state_: u32, value: f32, prec: u32) -> Vec<u8> {
    Builder::default()
        .varint(1, url as u64)
        .varint(2, state_ as u64)
        .fixed32(5, value.to_bits())
        .varint(10, prec as u64)
        .build()
}

#[test]
fn parse_response_extracts_elements_seq_and_txid() {
    // payload (field 3): a small header varint, then repeated DataElement (field 2)
    let payload = Builder::default()
        .varint(1, 28)
        .bytes(2, &element(262158, state::OK, 6.91, 2)) // RPM/100
        .bytes(2, &element(327681, state::OK, 14.9, 3)) // battery V
        .build();
    let msg = Builder::default()
        .varint(1, 276) // seq
        .varint(2, 351)
        .bytes(3, &payload)
        .varint(255, 42) // transaction id
        .build();

    let r = parse_response(&msg);
    assert_eq!(r.seq, Some(276));
    assert_eq!(r.transaction_id, Some(42));
    assert_eq!(
        r.elements,
        vec![
            DataElement { url: 262158, state: 0, value: 6.91, precision: Some(2) },
            DataElement { url: 327681, state: 0, value: 14.9, precision: Some(3) },
        ]
    );
}

#[test]
fn parse_response_survives_a_garbage_tail() {
    // A field-253 "secured" block of high-entropy bytes that strict protobuf can't
    // parse must not stop us from reading the elements that came before it.
    let garbage: Vec<u8> = (0u8..200).map(|x| x.wrapping_mul(37) ^ 0xA5).collect();
    let payload = Builder::default()
        .bytes(2, &element(131077, state::OK, 570.0, 0)) // range km
        .bytes(253, &garbage)
        .build();
    let msg = Builder::default().varint(1, 1).bytes(3, &payload).build();

    let r = parse_response(&msg);
    assert_eq!(r.elements.len(), 1);
    assert_eq!(r.elements[0].url, 131077);
    assert_eq!(r.elements[0].value, 570.0);
}

#[test]
fn deframer_reassembles_across_chunks() {
    let m1 = build_subscribe(1, 1000, 1);
    let m2 = build_subscribe(4, 10000, 2);
    let mut wire = m1.clone();
    wire.extend_from_slice(&m2);

    let mut d = Deframer::new();
    for chunk in wire.chunks(3) {
        d.push(chunk);
    }
    // next_message yields the payload WITHOUT the length prefix
    assert_eq!(d.next_message().unwrap(), &m1[1..]);
    assert_eq!(d.next_message().unwrap(), &m2[1..]);
    assert!(d.next_message().is_none());
}
