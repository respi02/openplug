# OpenPlug — language/stack choice for iOS + Android (2026-11-12)

> **Status (2026-11-13):** the choices below were carried out and hold up: the Rust
> core runs unchanged under an Android app (Classic SPP) and a Swift client (BLE),
> both verified on a real dongle. Open points noted here as "remaining" are resolved;
> see [`build_plan.md`](build_plan.md) for the current state.

## First, the real problem: Bluetooth transport, not language

The dongle speaks **Bluetooth Classic RFCOMM/SPP** (our capture: 357 SPP frames,
**0 BLE/GATT**).

- **Android**: SPP is no problem (`BluetoothSocket.createRfcommSocketToServiceRecord`).
- **iOS**: third parties cannot use arbitrary Classic SPP. Only via:
  1. **External Accessory framework** — requires the accessory to be **MFi** certified and a protocol string, or
  2. **BLE (Core Bluetooth)** — if the dongle is dual-mode and also advertises LE.

**RESOLVED (research, 2026-11-12):** the DataPlug is **dual-mode Bluetooth, NOT
MFi**. iOS goes via **Core Bluetooth (BLE)**. Evidence:
- FCC ID **T8RDP15** (model DP15, 2015; newer **T8RDP22**, 2023) carries **two
  grants** on one 2.4 GHz device: DTS (= Bluetooth LE) + DSS (= Bluetooth BR/EDR
  Classic). A dual-mode radio. (fccid.io/T8RDP15)
- No "Made for iPhone" / MFi anywhere (TEXA, Škoda/VW/SEAT, Apple). The SEAT
  brochure explicitly says do **not** pair the dongle via the phone's Bluetooth
  settings — pairing is in-app via a PIN, which is the BLE pattern (not an
  MFi/Classic bond).
- EU DoC (TEXA DoC-DATAPLUG-08): "CAN BUS INTERFACE", brands VW/SEAT/Škoda/Audi,
  "DataPlug 1000" (= our label SW 1000), RED 2014/53/EU, 0 dBm.
- Our Android capture = the Classic/SPP side; iOS uses the BLE side of the same dongle.
- Confirmed since: the dongle advertises BLE and runs the same TLS-SRP + protobuf
  tunnel over GATT ([`ble_transport.md`](ble_transport.md)).

This decision is independent of the language — it lives in a thin transport layer.
**Conclusion: both iOS and Android feasible, shared core, transport = SPP
(Android) / BLE GATT (iOS).**

## Recommended architecture: shared Rust core + thin native shells

```
┌ UI            Android: Kotlin+Compose   │  iOS: SwiftUI          (thin, native)
├ Transport     Android: RFCOMM SPP        │  iOS: BLE / ExternalAccessory  (thin shim)
└ CORE (Rust, shared, via UniFFI → Kotlin+Swift bindings)
     TLS-SRP  ·  protobuf  ·  transactions/reassembly  ·  OBD/UDS PDU  ·  data objects
```

**Core in Rust** (written once, identical on both platforms):
- **TLS-SRP**: implemented directly (our own TLS 1.2 record + handshake for the one
  ciphersuite the dongle uses, 0xC01D) — no C dependency; fully offline-testable.
- **protobuf**: a generic codec now; `prost` from the reconstructed `.proto` later.
- Connection state machine, `TransactionManager`, fragmentation, an OBD/UDS PDU
  builder, data-object decoding.
- Exposes a simple API (`connect(transport)`, `authenticate(pin)`,
  `subscribe(url)`, `send_pdu(...)`, an event stream) via **UniFFI** → generates
  Kotlin and Swift.

**Transport** feeds bytes into the core, per platform (the only place iOS/Android differ).
**UI** thin native; optionally Compose Multiplatform, but the UI is small so native is simplest.

### Why a Rust core, not the alternatives
| Option | Issue |
|---|---|
| **Rust core + UniFFI** ✅ | TLS-SRP + protocol written once, memory-safe, identical. Platform BT isolated in a shim. |
| Kotlin Multiplatform + Compose MP | UI shares nicely, but TLS-SRP on iOS (Kotlin/Native) is painful. |
| Flutter/Dart | Dart has no TLS-SRP → FFI into C anyway; the iOS BT constraint remains. |
| Reuse `libdongles.so`/`libmbedtls.so` from the APK | ELF Android-only, unusable on iOS; redistributing a vendor binary (TEXA) is legally worse. A reimplementation is cleaner. |

## Work order (as planned on 2026-11-12; all five steps are done — the current state and plan are in build_plan.md)
1. **Offline (done):** protocol mapped, TLS-SRP params found, transport for
   both platforms decided, API→modules mapped, crypto core built and verified.
2. **One live plaintext capture** (ignition on) → fill in the `.proto` and the `url` object catalog.
3. **Rust core against captured bytes**: TLS-SRP handshake (done), then
   `authenticate` → `subscribe`/`sendPDU`. Test first on desktop via a BT adapter.
4. **Android shim + UI** (fast path to an end-to-end check).
5. **iOS transport** (BLE), then SwiftUI.
