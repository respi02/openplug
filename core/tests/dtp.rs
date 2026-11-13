//! DTP link-framing tests, checked against the layout reversed from libdongle.so.

use openplug_core::dtp::{frame, Deframer, DtpTransport, DTP_HEADER_LEN};
use openplug_core::transport::{testpipe, Transport};

#[test]
fn header_matches_reversed_layout() {
    // 83-byte payload (our ClientHello size) -> header 00 53 44 5e.
    let payload = vec![0xAAu8; 83];
    let f = frame(&payload);
    assert_eq!(&f[..4], &[0x00, 0x53, 0x44, 0x5e]);
    assert_eq!(f.len(), 83 + DTP_HEADER_LEN);
    assert_eq!(&f[4..], &payload[..]);
}

#[test]
fn deframer_reassembles_across_chunks() {
    let p1 = vec![1u8, 2, 3, 4, 5];
    let p2 = vec![9u8; 200];
    let mut wire = frame(&p1);
    wire.extend_from_slice(&frame(&p2));

    let mut d = Deframer::new();
    // Feed the wire in awkward 7-byte chunks (splits headers and payloads).
    for chunk in wire.chunks(7) {
        d.push(chunk);
    }
    assert_eq!(d.next_payload().unwrap(), p1);
    assert_eq!(d.next_payload().unwrap(), p2);
    assert!(d.next_payload().is_none());
}

#[test]
fn transport_roundtrip_through_framing() {
    let (a, b) = testpipe::pair();
    let mut ta = DtpTransport::new(a);
    let mut tb = DtpTransport::new(b);

    let msg = b"hello dongle over DTP";
    ta.send(msg).unwrap();

    let mut got = [0u8; 64];
    let n = tb.recv(&mut got).unwrap();
    assert_eq!(&got[..n], msg);
}
