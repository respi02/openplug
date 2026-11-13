# OpenPlug

> Open-source replacement for the **ŠKODA Connect LITE** app, talking to the
> Bluetooth OBD dongle *Škoda DataPlug by TEXA*. Clean-room, interoperability-only.

**🚧 Under active development — contributions welcome.** The protocol is reverse-
engineered and the security layer is built; the apps are still being assembled. Help
is especially wanted on the **iOS/BLE** side (see below). How to contribute:
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

**odometer / mileage · fuel range · battery voltage · engine RPM · coolant &
intake temperatures · service intervals (km & days) · VIN** — plus ~30 further live
data objects the dongle streams.

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
uses the BLE side — so no MFi is required.

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
| `dongle` — app protocol: Subscribe request + DataElement decode | ✅ built from [`live_protocol.md`](research/notes/live_protocol.md) |

The platform clients are scaffolded (not yet built): `android/` (Kotlin/Compose
+ RFCOMM/SPP transport) and `ios/` (SwiftUI + Core Bluetooth transport), both thin
shells over `core/` via UniFFI.

## Status & roadmap

**Done & offline-verified**

- The full security layer of the tunnel: SRP-6a → TLS 1.2 TLS-SRP handshake
  (Encrypt-then-MAC + Extended Master Secret) → AES-128-CBC/HMAC-SHA1 records,
  anchored to real captured handshake bytes from our own dongle. No C dependency.
- A generic protobuf codec and the DTP link framing.
- The application protocol, implemented in `core/` from the decoded spec: the
  `Subscribe{paramId, rate}` request and the `DataElement{url, value}` response
  decode (byte-scan that survives the dongle's "secured" block).

**Next**

1. Expose a small read API over a live `Session` (connect → authenticate → subscribe
   → event stream) and a desktop test harness.
2. Android transport (RFCOMM/SPP) + a thin Compose UI — the fast path to end-to-end.
3. iOS transport (BLE) — blocked on the session-open step (below).

**iOS help wanted.** On iOS the dongle uses BLE, and it's confirmed to advertise a
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

All cryptographic layers are verified offline, anchored to real captured handshake
bytes from our own dongle.

## Documentation

In [`research/notes/`](research/notes/):

- [`connect_lite_overview.md`](research/notes/connect_lite_overview.md) · [`dataplug_hardware.md`](research/notes/dataplug_hardware.md) — background on the app and the hardware.
- [`protocol_spec.md`](research/notes/protocol_spec.md) · [`protocol_findings.md`](research/notes/protocol_findings.md) — the phone↔dongle protocol.
- [`live_protocol.md`](research/notes/live_protocol.md) — the decrypted application protocol and data-object catalog.
- [`ble_transport.md`](research/notes/ble_transport.md) — the iOS/BLE findings and the open session-open question.
- [`tech_stack.md`](research/notes/tech_stack.md) · [`build_plan.md`](research/notes/build_plan.md) — architecture and build plan.
- [`code_licensing.md`](research/notes/code_licensing.md) — dependency licenses and the clean-room rationale.
- [`ios_contribution_request.md`](research/notes/ios_contribution_request.md) — how an iOS-side collaborator can help.

## Contributing

Contributions are welcome — code, testing against real hardware, documentation, and
most of all the **iOS reference** that unblocks the BLE transport (see *iOS help
wanted* above). Good first areas: wiring the decoded protocol into `core/`, the
Android SPP transport + UI, and a desktop test harness.

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
