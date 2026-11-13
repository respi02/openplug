# OpenPlug — iOS app (scaffold, BLE transport UNVERIFIED)

> **Status: skeleton + open question.** The dongle is dual-mode Bluetooth and
> **not MFi**, so iOS almost certainly uses **Core Bluetooth (BLE)**. But our
> packet capture was 100% Bluetooth Classic SPP — we have **zero bytes of the BLE
> side**. The BLE transport below cannot be written correctly until the BLE path
> is observed.

## What is unverified
BLE is GATT (characteristic write + notify), not a byte stream like RFCOMM/SPP:
- TLS records (~hundreds of bytes) must be segmented across writes and reassembled
  from notifications, bounded by the ATT MTU (default 20 B, negotiable).
- Unknown whether the dongle's BLE characteristic is a transparent serial pipe
  (then our core works unchanged) or adds its own framing (then we must reverse it).
- Unknown: service UUID, write/notify characteristic UUIDs, with/without response,
  bonding requirements.

## How to resolve it (does NOT need the retired iOS app)
1. **BLE scan + GATT enumeration** of the powered dongle (nRF Connect / LightBlue,
   or macOS) → service + characteristic UUIDs and properties.
2. **Active probe from any BLE central (Mac/Android) using our own core:** write a
   TLS-SRP ClientHello to the write characteristic, listen for notifications. A
   sensible ServerHello (`16 03 03 … 02 …`) confirms the same tunnel and reveals
   the framing.
3. Fallback: an iOS HCI capture (PacketLogger over USB) with the original app.

## Architecture (once confirmed)
```
SwiftUI
   │
CoreBluetoothTransport  ──implements──►  ByteTransport (UniFFI, core/src/ffi.rs)
   │
CBPeripheral write/notify characteristic   openplug-core (TLS-SRP + protocol)
```
The shared Rust `core/` is transport-agnostic; only this thin BLE shim (notification
reassembly + MTU segmentation + any dongle-specific framing) depends on the findings.
