# OpenPlug

> Open-source replacement for the **ŠKODA Connect LITE** app, talking to the
> Bluetooth OBD dongle *Škoda DataPlug by TEXA*. Clean-room, interoperability-only.

ŠKODA discontinued the Connect LITE app on 1 July 2025. The **DataPlug hardware
still works** — only the app that drove it is gone. OpenPlug is a clean,
cross-platform (Android + iOS) reimplementation of the phone↔dongle protocol so
the dongle you own stays usable.

**This is not affiliated with or endorsed by Škoda, Volkswagen or TEXA.** It
reimplements an *interface* for interoperability with hardware the user owns; it
contains no vendor code, firmware, keys or trademarks.

## How the dongle works (reverse-engineered from our own device + traffic)

```
Bluetooth  →  TLS-SRP tunnel        →  protobuf          →  OBD / UDS
(Android:      (TLS 1.2, SRP-6a,        (_TDongleReq/Rsp)     over CAN
 Classic SPP;   cipher 0xC01D,                                (the dongle is a
 iOS: BLE)      identity "PA",                                 CAN/OBD gateway)
                password = PIN on label)
```

No server is involved — everything is local between phone and dongle. The
authentication secret is the **PIN printed on the dongle's label**, which the
owner has. The dongle is dual-mode Bluetooth (FCC ID T8RDP15/T8RDP22): Android
uses the Classic/SPP side, iOS uses the BLE side — so no MFi is required.

## Architecture

Shared **Rust core** (`core/`) → platform transport shim → thin native UI
(Kotlin/Compose on Android, SwiftUI on iOS), bound via UniFFI.

| Layer | Status |
|---|---|
| `transport` — byte-stream trait + test pipe | ✅ |
| `srp` — SRP-6a (RFC 5054) | ✅ verified vs RFC vector |
| `tls_prf` — TLS 1.2 PRF-SHA256 | ✅ verified vs known-answer |
| `tls_record` — AES-128-CBC + HMAC-SHA1, Encrypt-then-MAC (RFC 7366) | ✅ |
| `tls_handshake` — master secret, EMS (RFC 7627), ServerKeyExchange parse | ✅ parses real dongle bytes |
| `tls_client` — TLS-SRP handshake orchestration | ✅ full loopback test |
| `protobuf` — generic wire codec (decode/encode/inspect) | ✅ |
| `dtp` — BLE/SPP link framing (`len ‖ 0x44 0x5e ‖ payload`) | ✅ reversed from vendor libs |
| `.proto` schema + data-object layer | ✅ decoded from a live capture ([notes](research/notes/live_protocol.md)) |

The platform clients are scaffolded (not yet built): `android/` (Kotlin/Compose
+ RFCOMM/SPP transport) and `ios/` (SwiftUI + Core Bluetooth transport), both thin
shells over `core/` via UniFFI.

**iOS help wanted.** The Android/SPP read path is fully reversed and the live
protocol is decoded. On iOS the dongle uses BLE, and it's confirmed to advertise a
write/notify GATT pair — but it ignores our (correctly framed, byte-exact)
ClientHello, so there's a BLE **session-open** step the retired iOS app does that we
can't observe without an iOS reference. If you have that app on your Apple ID and can
do static analysis of its build, see
[`research/notes/ios_contribution_request.md`](research/notes/ios_contribution_request.md)
— the one thing we need is the connect→first-TLS-byte sequence.

## Build & test (offline, no dongle needed)

```sh
cd core && cargo test
```

All cryptographic layers are verified offline, with no C dependency, anchored to
real captured handshake bytes from the author's own dongle.

## Status

The entire security layer of the tunnel (SRP-6a → TLS 1.2 TLS-SRP handshake with
Encrypt-then-MAC and Extended Master Secret → AES-128-CBC/HMAC-SHA1 records) is
implemented and offline-verified, plus a generic protobuf codec and the DTP link
framing. The application protocol is now **decoded from a live plaintext capture**
(`Subscribe{paramId, rate}` → a stream of `DataElement{url, value}`; see
`research/notes/live_protocol.md`). What remains: wiring the decoded protocol and
transports into the core, the native UIs, and the iOS BLE session-open (help wanted,
above).

## Legal

OpenPlug is a clean reimplementation of an open protocol stack (SRP-6a RFC 5054,
TLS 1.2 RFC 5246, protobuf, ISO OBD/UDS) for **interoperability** with hardware
the user owns — lawful under EU Directive 2009/24/EC Art. 6 and Czech Act
No. 121/2000 Sb. § 66. See `research/notes/code_licensing.md`. Code is **MIT**
licensed; dependencies are MIT/Apache-2.0.
