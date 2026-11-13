# OpenPlug — Apple platforms (macOS / iOS)

> **Status: builds; not yet verified against a dongle.** A Swift package with the Core
> Bluetooth transport (`OpenPlugBLE`, shared by macOS and iOS) and a macOS
> command-line client. The transport follows the scheme verified from macOS with a
> Python bridge (see [`research/notes/ble_transport.md`](../research/notes/ble_transport.md));
> this Swift implementation itself has not completed a session yet.

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
