# OpenPlug — iOS app (scaffold)

> **Status: skeleton, not built here.** The dongle is dual-mode Bluetooth and
> **not MFi**, so iOS uses **Core Bluetooth (BLE)** — see research/notes/tech_stack.md.
> Confirm the GATT service/characteristic UUIDs with a BLE scan of the powered dongle.

```
SwiftUI
   │
CoreBluetoothTransport  ──implements──►  (Rust) Transport callback interface (UniFFI)
   │
CBPeripheral write/notify characteristic   openplug-core (TLS-SRP + protocol)
```

The same Rust `core/` is built as an XCFramework (cargo + UniFFI Swift bindings);
Swift implements the `Transport` callback interface backed by a BLE
write/notify characteristic pair. The TLS-SRP + protobuf tunnel is transport-agnostic.

## TODO
- BLE scan → fill in service + write/notify characteristic UUIDs.
- Confirm the same TLS-SRP+protobuf tunnel runs over the BLE characteristic
  (very likely; verify MTU/segmentation).
