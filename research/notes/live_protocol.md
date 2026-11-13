# OpenPlug — decrypted live protocol (plaintext capture, 2026-11-13)

Captured on our own Android phone + our own dongle, engine running, by hooking the
TLS plaintext boundary (`mbedtls_ssl_read`/`mbedtls_ssl_write` in `libmbedtls.so`)
with Frida on the already-patched app. This is the plaintext *inside* the TLS-SRP
tunnel — the nanopb `_TDongleReq`/`_TDongleRsp` layer. 791 messages captured.
(Tooling + raw dump are gitignored under `captures/frida/`.)

## Message framing

Each TLS plaintext write/read is one message, length-delimited:

```
  varint(payload_len) || protobuf(payload)
```

e.g. a 362-byte read begins `e8 02` = varint 360 = the following protobuf length.
Every message carries a transaction id in **field 255** (`f255`), incrementing per
request and echoed on the matching response (`TransactionManager::reserveId`).

## _TDongleReq  (phone → dongle)

One command field selects the operation; `f255` is the transaction id.

```proto
message TDongleReq {
  Subscribe subscribe = 16;     // observed: live-value subscription
  bytes     getStatus = 4;      // empty-body getters (status/activation/firmware/…)
  bytes     getA      = 1;
  bytes     getB      = 5;
  bytes     getC      = 22;
  EnumReq   getObject = 15;     // { uint32 sel = 1 }  (e.g. sel=22, sel=4)
  uint32    transactionId = 255;
}
message Subscribe {             // field 16
  uint32 mode    = 1;           // observed 0
  uint32 paramId = 2;           // small index: 1,4,6,7,8,9,11,14,16,17,19,20,23,28,30,34,44
  uint32 rateMs  = 3;           // 1000 or 10000
}
```

The app subscribes to ~17 diagnostic params, each at 1 s or 10 s. Example raw TX:
`0d 82 01 07 08 00 10 0e 18 e8 07  f8 0f 2f` =
`{ f16:{mode:0, paramId:14, rateMs:1000}, f255:47 }`.

## _TDongleRsp  (dongle → phone)

```proto
message TDongleRsp {
  uint32       seq      = 1;    // increments
  uint32       field2   = 2;
  Payload      payload  = 3;
  MetaHeader   meta     = 253;  // three 64-byte high-entropy blocks (encrypted/signed;
                                //   the "secured" channel — not needed for open values)
  BlockDesc    block    = 254;
  uint32       transactionId = 255;
}
message Payload {               // field 3
  // small header, then repeated DataElement
  repeated DataElement element = 2;
}
message DataElement {           // the live value container
  uint32 url       = 1;         // numeric data-object id
  uint32 state     = 2;         // 0=OK, 1=?, 4=NODATA
  float  value     = 5;         // i32 little-endian float32
  uint32 precision = 10;        // display precision
}
```

Different response sizes carry different object sets (362 B = the 8 fast params,
321 B includes battery voltage, 332 B includes range, 306/469 B others). A robust
reader byte-scans for the `08<url> 10<state> 2d<f32> 50<prec>` element pattern rather
than relying on container parsing (the `f253` block is non-protobuf and defeats a
strict parser). `captures/frida/extract_objects.py` does this; it recovered 39
distinct objects from the session.

### Observed container shape and session start

Decoding a real 362 B response more closely, the outer message is
`{ seq(1), len(2), inner(3) }` where `inner` is again `varint(len) ‖ protobuf` and holds
`{ 15: { 1: { paramId(1), repeated DataElement(2) } }, 253: …, 255: txid }` — i.e. each
data response names the subscribed `paramId` it belongs to, followed by that
parameter's group of elements (e.g. `paramId 28` → several `0x0004_00xx` objects).

The app's request sequence before streaming (from the same capture) is: `f15{sel=22}`,
then empty getters `f22`, `f4`, `f1`, `f4`, `f5`…, then 17 subscriptions in this order
(`paramId@rateMs`): 14@1000, 23@1000, 16@10000, 11@10000, 1@10000, 7@10000, 4@10000,
8@10000, 20@10000, 44@10000, 6@10000, 9@10000, 28@1000, 19@10000, 17@1000, 30@1000,
34@1000. All requests are ≤ 14 B, which matches the encrypted SPP pairing capture:
right after the TLS Finished the phone sends only minimum-size records (57 B on the
wire), so no separate large authentication exchange follows the TLS-SRP handshake.
(The latter is inferred from record sizes, not from decrypted content.)

## Data-object catalog (url → signal)

`url` is `category<<16 | index`. Confirmed against the app UI (engine idling):

| url (hex)   | dec     | value seen | meaning (confirmed)        |
|-------------|---------|-----------:|----------------------------|
| 0x0002_0004 | 131076  |     odom   | **odometer, km**           |
| 0x0002_0005 | 131077  |       570  | **range, km**              |
| 0x0004_000e | 262158  |      6.9   | **engine RPM ÷ 100**       |
| 0x0005_0001 | 327681  |      14.9  | **battery voltage, V**     |
| 0x0007_0009 | 458761  |       179  | **service interval, days** |
| 0x0007_0012 | 458770  |      6100  | **service interval, km**   |

Categories: `0x0002`=vehicle, `0x0004`=engine (live), `0x0005`=electrical,
`0x0007`=service, `0x0010`/`0x0014`=further. ~30 more objects decode to live floats
whose semantics aren't yet labelled (the names live in the app's resources, not in
the protocol; the dongle sends numeric urls only). The dongle also reports the VIN.

## What this proves

The whole read path is understood end-to-end and reproducible from the Rust core:
`Subscribe(paramId, rate)` → stream of `DataElement{url, value}`. No server, no PDU
needed for live values. The app's login wall is purely a local UI gate (locked
features — Trips/Statistics/Fuel monitor/Parking — are local SQLite, generating no
dongle traffic); the capture sits below it and already sees everything the dongle
streams. Remaining optional work: the raw `sendPDU` (UDS/OBD) path, which the app
doesn't exercise in normal live-data use.
