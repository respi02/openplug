# OpenPlug — Apple platforms (macOS / iOS)

> **Status: works against a real dongle from macOS (2026-11-13).** A Swift package
> with the Core Bluetooth transport (`OpenPlugBLE`, shared by macOS and iOS) and a
> macOS command-line client, which authenticates with the label PIN and prints the
> car's live values over BLE. The scheme is described in
> [`research/notes/ble_transport.md`](../research/notes/ble_transport.md).

## Build and run (macOS)

```sh
cd apple
./build-core.sh        # Rust core -> lib/libopenplug_core.a + Swift bindings
swift build
OPENPLUG_PIN=<PIN from the label> .build/debug/openplug-cli --seconds 30
```

`build-core.sh` outputs are build products (gitignored). The client scans for the
dongle's service, connects, authenticates and prints live values. The dongle serves
one client and stops advertising while a phone is connected to it.

## Layout

- `Sources/OpenPlugBLE/CoreBluetoothTransport.swift` — the BLE transport
  (implements the core's `ByteTransport`).
- `Sources/openplug-cli/` — the macOS command-line client.
- `Sources/OpenPlugCore/`, `Sources/OpenPlugCoreFFI/` — generated UniFFI bindings.

An iOS app needs the core built for the iOS targets (an XCFramework) and a SwiftUI
shell; neither exists yet.
