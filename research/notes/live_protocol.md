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

`url` is `category<<16 | index`. The numeric ids are all the dongle sends; their
meaning comes from a small id → name table the app keeps on the phone (dated 2019),
cross-checked against the live values from our car (engine idling, then a drive).
Subscriptions are per **parameter group** (`paramId`); each response carries that
group's objects.

| paramId | group | url (hex) | object | unit | seen on our car |
|--:|---|---|---|---|---|
| 1  | fuel consumption  | 0x0002_0009 | instantaneous consumption | l/h | no data |
|    |                   | 0x0002_001a | average consumption | l/100 km | 5.9 |
| 4  | odometer          | 0x0002_0004 | odometer | km | ✓ |
| 6  | range             | 0x0002_0005 | cruising range | km | 570 |
| 7  | fuel level        | 0x0002_0001 | fuel level, relative | % | no data |
|    |                   | 0x0002_0002 | fuel in tank | l | 37.4 |
| 8  | service           | 0x0007_0009 | time since last service | days | 179 |
|    |                   | 0x0007_0012 | distance since last service | km | 6100 |
| 14 | engine speed      | 0x0004_0000 | engine speed | rpm | 768 |
| 16 | coolant temperature | 0x0004_0001 | coolant temperature | °C | 48–60 |
| 19 | oil temperature   | 0x0002_0030 | oil temperature (dashboard) | °C | 40–54 |
|    |                   | 0x0004_000f | oil temperature | °C | no data |
| 20 | battery voltage   | 0x0005_0001 | battery voltage | V | 14.8–14.9 |
|    |                   | 0x0014_0001 | battery voltage (dashboard) | V | 14.8–15 |
| 23 | vehicle speed     | 0x0002_0000 | vehicle speed | km/h | 0 |
| 28 | intake air        | 0x0004_000c | intake air pressure | kPa | 101–102 |
|    |                   | 0x0004_000d | intake air temperature | °C | 16–22 |
|    |                   | 0x0004_000e | intake air mass flow | g/s | 6.5–8.4 |
| 30 | throttle position | 0x0004_0026 | throttle position, absolute | % | 83 |
|    |                   | 0x0004_0027 | throttle position, relative | % | 99.6 |

The app also subscribes to groups 9 (service interval status), 11 (dashboard
indicators / warning lights, category `0x0001`), 17 (boost pressure), 34 (catalyst
temperature) and 44 (AdBlue); those are not yet decoded here. Group 22 is the VIN
(category `0x0006`, static). Units are inferred from the value ranges — the table
itself carries none.

**Correction:** an earlier revision of this note labelled `0x0004_000e` "engine RPM ÷
100". That was a wrong guess from a value of 6.9 at idle; it is the air mass flow in
g/s. Engine speed is `0x0004_0000`, in plain rpm. Likewise `0x0007_0009` /
`0x0007_0012` count days / km *since* the last service, not until the next one.

## What this proves

The whole read path is understood end-to-end and reproducible from the Rust core:
`Subscribe(paramId, rate)` → stream of `DataElement{url, value}`. No server, no PDU
needed for live values. The app's login wall is purely a local UI gate (locked
features — Trips/Statistics/Fuel monitor/Parking — are local SQLite, generating no
dongle traffic); the capture sits below it and already sees everything the dongle
streams. Remaining optional work: the raw `sendPDU` (UDS/OBD) path, which the app
doesn't exercise in normal live-data use.
