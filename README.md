# OpenPlug

> Open-source replacement for the **ŠKODA Connect LITE** app, talking to the
> Bluetooth OBD dongle *Škoda DataPlug by TEXA*. Clean-room, interoperability-only.

**🚧 Under active development — contributions welcome.** The protocol is reverse-
engineered and both transports work against a real dongle: the Android app reads live
values over Bluetooth Classic, and the same core runs over BLE. See
[Contributing](#contributing).

ŠKODA discontinued the Connect LITE app on 1 July 2025. The **DataPlug hardware
still works** — only the app that drove it is gone. OpenPlug is a clean,
cross-platform (Android + iOS) reimplementation of the phone↔dongle protocol so
the dongle you own stays usable.

It talks to the dongle **locally over Bluetooth** — no account, no server, no cloud.
The only secret needed is the **PIN printed on the dongle's label**, which the owner
has.

**Not affiliated with or endorsed by Škoda, Volkswagen or TEXA.** It reimplements an
*interface* for interoperability with hardware the user owns; it contains no vendor
code, firmware, keys or trademarks.

## What it reads

Decoded from live traffic on our own car (see
[`research/notes/live_protocol.md`](research/notes/live_protocol.md)):

**odometer · range · fuel in tank · average consumption · vehicle speed · engine
speed · coolant, oil & intake-air temperatures · battery voltage · throttle position ·
time & distance since the last service** — the full catalog is in the note above.

The app *subscribes* to a signal at a chosen rate and the dongle streams updates;
OpenPlug reimplements exactly that read path. (The original app also did phone-side
trip logging and statistics — see the background notes below.)

## Background

- [**What ŠKODA Connect LITE was**](research/notes/connect_lite_overview.md) — the
  retired app this project replaces, and its features.
- [**What the DataPlug by TEXA is**](research/notes/dataplug_hardware.md) — the
  Bluetooth-to-CAN/OBD dongle it talked to.

## How the dongle works (reverse-engineered from our own device + traffic)

```
Bluetooth  →  TLS-SRP tunnel        →  protobuf          →  OBD / UDS
(Android:      (TLS 1.2, SRP-6a,        (_TDongleReq/Rsp)     over CAN
 Classic SPP;   cipher 0xC01D,                                (the dongle is a
 iOS: BLE)      identity "PA",                                 CAN/OBD gateway)
                password = PIN on label)
```

No server is involved — everything is local between phone and dongle. The dongle is
dual-mode Bluetooth (FCC ID T8RDP15/T8RDP22): Android uses the Classic/SPP side, iOS
uses the BLE side — so no MFi is required. Both carry the same bare TLS records; on
BLE each record is written to one characteristic and read back from it after a
one-byte notification (see [`ble_transport.md`](research/notes/ble_transport.md)).

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
| `dtp` — a framing found in the vendor library (`len ‖ 0x44 0x5e ‖ payload`) | ✅ implemented, but not used on the wire by either transport |
| `dongle` — app protocol: Subscribe request + DataElement decode | ✅ built from [`live_protocol.md`](research/notes/live_protocol.md) |
| `link` — read API: connect → authenticate → subscribe → value stream | ✅ verified live against a real dongle (Android/SPP, 2026-11-13) |
| `ffi` — UniFFI facade for Kotlin/Swift (feature `ffi`) | ✅ tested from Rust; Kotlin + Swift bindings generate |

Platform clients, both thin shells over `core/` via UniFFI: `android/`
(Kotlin/Compose + RFCOMM/SPP transport) builds and works against a real dongle;
`ios/` (SwiftUI + Core Bluetooth transport) is not written yet, but its transport is
now fully understood.

## Status & roadmap

**Done & offline-verified**

- The full security layer of the tunnel: SRP-6a → TLS 1.2 TLS-SRP handshake
  (Encrypt-then-MAC + Extended Master Secret) → AES-128-CBC/HMAC-SHA1 records,
  anchored to real captured handshake bytes from our own dongle. No C dependency.
- A generic protobuf codec.
- The application protocol, implemented in `core/` from the decoded spec: the
  `Subscribe{paramId, rate}` request and the `DataElement{url, value}` response
  decode (byte-scan that survives the dongle's "secured" block).

**Next**

1. ~~First live run against a real dongle~~ — done on Android/SPP (2026-11-13): the
   app authenticates with the label PIN and shows the car's live values.
2. Android transport (RFCOMM/SPP) + a thin Compose UI — the fast path to end-to-end.
3. ~~Work out the BLE transport~~ — done (2026-11-13): full session and live values
   over BLE from a Mac, with the unmodified core.
4. iOS/macOS app: a Core Bluetooth transport + SwiftUI shell over the same core.

## Build & test (offline, no dongle needed)

```sh
cd core && cargo test
```

All cryptographic layers are verified offline, anchored to real captured handshake
bytes from our own dongle.

### Try it end to end (still no dongle needed)

```sh
cd core
cargo run --example fake_dongle                       # terminal 1: stand-in dongle on TCP
OPENPLUG_PIN=1234 cargo run --example live -- --tcp 127.0.0.1:4455   # terminal 2
```

`fake_dongle` speaks the real stack (TLS-SRP + the framed protobuf) with synthetic
values. Against real hardware, pair the dongle with a computer and point the harness
at its serial port: `cargo run --example live -- --serial /dev/cu.<dongle>` (the PIN
from the label goes in `OPENPLUG_PIN` or is prompted). Add `--raw` to dump every
message.

## Documentation

In [`research/notes/`](research/notes/):

- [`connect_lite_overview.md`](research/notes/connect_lite_overview.md) · [`dataplug_hardware.md`](research/notes/dataplug_hardware.md) — background on the app and the hardware.
- [`protocol_spec.md`](research/notes/protocol_spec.md) · [`protocol_findings.md`](research/notes/protocol_findings.md) — the phone↔dongle protocol.
- [`live_protocol.md`](research/notes/live_protocol.md) — the decrypted application protocol and data-object catalog.
- [`ble_transport.md`](research/notes/ble_transport.md) — how the dongle's BLE transport works.
- [`tech_stack.md`](research/notes/tech_stack.md) · [`build_plan.md`](research/notes/build_plan.md) — architecture and build plan.
- [`code_licensing.md`](research/notes/code_licensing.md) — dependency licenses and the clean-room rationale.

## Contributing

Contributions are welcome — code, testing against real hardware (other cars and
dongle revisions especially), and documentation. Good first areas: the iOS/macOS Core
Bluetooth transport and app, labelling more data objects, and trip logging.

To keep the project clean-room and interoperability-only, please:

- **Write implementations from the specs** in [`research/notes/`](research/notes/)
  (the protocol is documented there), not by copying vendor code.
- **Never commit vendor binaries or secrets** — no `lib*.so`, APK/IPA, firmware,
  config blobs, dongle PINs, VINs or raw captures. The `.gitignore` enforces this;
  keep it that way.
- Keep the **interoperability framing**: this project reads *your own* vehicle via
  *your own* dongle with the owner's label PIN; it needs no account, server, or the
  original app.

Open an issue to discuss bigger changes first. By contributing you agree your code is
MIT-licensed.

## Legal

OpenPlug is a clean reimplementation of an open protocol stack (SRP-6a RFC 5054,
TLS 1.2 RFC 5246, protobuf, ISO OBD/UDS) for **interoperability** with hardware
the user owns — lawful under EU Directive 2009/24/EC Art. 6 and Czech Act
No. 121/2000 Sb. § 66. See [`research/notes/code_licensing.md`](research/notes/code_licensing.md).
Code is **MIT** licensed; dependencies are MIT/Apache-2.0.
