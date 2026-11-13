# OpenPlug

> Open-source replacement for the **ŠKODA Connect LITE** app, talking to the
> Bluetooth OBD dongle *Škoda DataPlug by TEXA*. Clean-room, interoperability-only.

**🚧 Under active development — contributions welcome.** It already reads live
values from a real dongle: on Android over Bluetooth Classic, and from a Mac over BLE.
An iOS/macOS app is in progress. See [Status](#status) and [Contributing](#contributing).

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
Bluetooth            →  TLS-SRP tunnel         →  protobuf            →  the car
Classic SPP (Android)    TLS 1.2, SRP-6a,          subscribe to a          the dongle is a
or BLE (iOS, macOS)      cipher 0xC01D,            parameter group,        CAN/OBD gateway
                         identity "PA",            get a stream of
                         password = label PIN      data objects
```

No server is involved — everything is local between phone and dongle. The dongle is
dual-mode Bluetooth (FCC ID T8RDP15/T8RDP22), so no MFi is required. Both sides carry
the same bare TLS records: on Classic they travel on the serial channel; on BLE each
record is written to one characteristic and read back from it after a one-byte
notification ([`ble_transport.md`](research/notes/ble_transport.md)).

## Status

Everything below has run against a real dongle in a real car (2026-11-13).

| Piece | Where | State |
|---|---|---|
| Protocol core (Rust) | [`core/`](core/) | ✅ TLS-SRP tunnel + read path; offline tests + live |
| Android app | [`android/`](android/) | ✅ picks a paired dongle, takes the PIN, shows live values (Classic SPP) |
| BLE transport (Swift) + macOS CLI | [`apple/`](apple/) | ✅ live values over BLE from a Mac |
| iOS / macOS app (SwiftUI) | [`apple/App/`](apple/App/) | 🚧 runs on macOS against the fake dongle; not yet run on an iPhone or against a real dongle |
| Desktop harness + fake dongle | `core/examples/` | ✅ for development without a car |

What OpenPlug does today is **read live values**. Not there yet: trip logging and
statistics, the warning-light and AdBlue groups, the dongle's token authentication
(the PIN is used on every connect), and the raw OBD/UDS request path.

## Architecture

A shared **Rust core** does all protocol work; each platform adds a small Bluetooth
transport and a thin native UI, bound through UniFFI.

```
Compose UI (Android)              SwiftUI (iOS, macOS) / CLI (macOS)
BluetoothSppTransport             CoreBluetoothTransport
        └──────── ByteTransport (UniFFI) ────────┘
                    openplug-core (Rust)
   link → dongle (protobuf) → tls_client / tls_record / srp → bytes
```

| Core module | Role |
|---|---|
| `srp`, `tls_prf`, `tls_handshake`, `tls_record`, `tls_client` | the TLS-SRP tunnel: SRP-6a (RFC 5054), TLS 1.2 with Encrypt-then-MAC (RFC 7366) and Extended Master Secret (RFC 7627), AES-128-CBC + HMAC-SHA1. Pure Rust, no C dependency |
| `protobuf` | schema-less protobuf wire codec |
| `dongle` | the application protocol: Subscribe request, DataElement decode, signal catalog |
| `link` | the read API: connect → authenticate → subscribe → stream of values |
| `ffi` | the UniFFI facade for Kotlin and Swift (feature `ffi`) |
| `transport` | the byte-channel trait, an adapter for any `Read + Write`, a test pipe |
| `dtp` | a framing found in the vendor library; not used on the wire by either transport |

## Build & run

**Core, offline (no dongle needed):**

```sh
cd core
cargo test --features ffi
```

**End to end without a car** — a stand-in dongle with synthetic values:

```sh
cd core
cargo run --example fake_dongle                                      # terminal 1
OPENPLUG_PIN=1234 cargo run --example live -- --tcp 127.0.0.1:4455   # terminal 2
```

**Android** (details in [`android/README.md`](android/README.md)):

```sh
cd android && ./build-core.sh && ./gradlew assembleDebug
```

**macOS / iOS over BLE** (details in [`apple/README.md`](apple/README.md)):

```sh
cd apple && ./build-core.sh && swift build
OPENPLUG_PIN=<PIN from the label> .build/debug/openplug-cli   # command line
xcodegen generate && open OpenPlug.xcodeproj                   # the SwiftUI app
```

The dongle serves one client at a time, and stops advertising over BLE while a phone
holds it. If the original app is still installed, it reconnects on its own and takes
the link — stop it while using OpenPlug.

## Roadmap

1. iOS app: first run on an iPhone and against a real dongle (the SwiftUI app and
   the core for iOS are built; see `apple/`).
2. Trip logging and statistics on the phone (the original app's main feature).
3. More of the protocol: warning lights, AdBlue, token authentication, a foreground
   service on Android to keep the link alive while driving.
4. More cars: the catalog so far reflects one vehicle.

## Documentation

In [`research/notes/`](research/notes/):

- [`connect_lite_overview.md`](research/notes/connect_lite_overview.md) · [`dataplug_hardware.md`](research/notes/dataplug_hardware.md) — background on the retired app and the hardware.
- [`protocol_spec.md`](research/notes/protocol_spec.md) · [`protocol_findings.md`](research/notes/protocol_findings.md) — the protocol stack and how the tunnel was identified.
- [`live_protocol.md`](research/notes/live_protocol.md) — the application protocol and the data-object catalog.
- [`ble_transport.md`](research/notes/ble_transport.md) — how the BLE transport works.
- [`tech_stack.md`](research/notes/tech_stack.md) · [`build_plan.md`](research/notes/build_plan.md) — architecture rationale, current state and plan.
- [`code_licensing.md`](research/notes/code_licensing.md) — dependency licenses and the clean-room rationale.

## Contributing

Contributions are welcome — code, testing against real hardware (other cars and
dongle revisions especially), and documentation. Good first areas: the iOS/macOS
SwiftUI app, labelling more data objects, and trip logging.

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
