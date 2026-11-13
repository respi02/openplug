# OpenPlug — iOS app (not written yet; transport understood)

> The shared Core Bluetooth transport and a macOS command-line client live in
> [`../apple/`](../apple/).

> **Status: no code yet.** The dongle's BLE transport is now fully worked out and
> verified from macOS with the unmodified Rust core, so the iOS (and macOS) client is
> ordinary Core Bluetooth work. The dongle is not MFi and needs no bonding.

## The BLE transport

Details and the investigation: [`research/notes/ble_transport.md`](../research/notes/ble_transport.md).

| | UUID |
|---|---|
| service | `732db477-bfd6-4694-9378-86c246f35def` |
| data characteristic (write + read) | `478a86f3-e47c-4618-9b15-3c7c92cc8184` |
| doorbell characteristic (notify) | `2efb2dd4-cdc3-11e6-9d9d-cec0c932ce01` |

The dongle advertises as `DP_2_1_<serial>`.

- **Send:** write each TLS record whole to the data characteristic, with response
  (Core Bluetooth turns writes longer than the MTU into long writes).
- **Receive:** every one-byte `0xff` notification on the doorbell means one chunk is
  waiting; read the data characteristic to fetch it. Chunks are at most 512 bytes and
  do not respect record boundaries, so treat the reads as a byte stream.

## Architecture

```
SwiftUI
   │
CoreBluetoothTransport  ──implements──►  ByteTransport (UniFFI, core/src/ffi.rs)
   │
CBPeripheral: write / notify / read        openplug-core (TLS-SRP + protocol)
```

`ByteTransport.recv` blocks, while Core Bluetooth is callback-driven: the transport
queues the bytes of each completed read and `recv` waits on that queue from the
core's background thread. The Swift bindings are generated from the core
(`OpenPlugCore` module, see `core/uniffi.toml`).
