# OpenPlug — Android app (scaffold)

> **Status: skeleton, not yet buildable here.** The files below are a starting
> structure for the Android client. They are *not* compiled/tested in this repo
> yet (no Android SDK/Gradle in the dev environment). They show the intended shape:
> a thin native shell over the shared Rust `core/`.

## How it fits together

```
Compose UI (MainActivity)
        │
BluetoothSppTransport  ──implements──►  (Rust) Transport callback interface
        │                                        │
Android BluetoothSocket (RFCOMM/SPP)      openplug-core (TLS-SRP + protocol)
```

The Rust core is built for Android with **cargo-ndk** and exposed to Kotlin via
**UniFFI** (see `android/uniffi/openplug.udl`). Kotlin implements the `Transport`
callback interface backed by a `BluetoothSocket`; the core drives the TLS-SRP
handshake and the dongle protocol over it.

## Build (intended)

```sh
# 1. Build the Rust core for Android targets
cargo ndk -t arm64-v8a -t armeabi-v7a -o app/src/main/jniLibs build --release
# 2. Generate Kotlin bindings
cargo run --bin uniffi-bindgen generate --language kotlin ...
# 3. ./gradlew assembleDebug
```

## Permissions
`BLUETOOTH_CONNECT`, `BLUETOOTH_SCAN` (Android 12+), plus `ACCESS_FINE_LOCATION`
for trip logging. Pairing uses the PIN printed on the dongle label.

## TODO
- Wire up the UniFFI facade in the core (`core/src/ffi.rs`).
- Foreground service to keep the connection alive while driving.
- Replace the demo UI with the real data-object views once the `.proto` is in.
