# OpenPlug — Android app (scaffold)

> **Status: skeleton, not yet buildable here.** The files below are a starting
> structure for the Android client. They are *not* compiled/tested in this repo
> yet (no Android SDK/Gradle in the dev environment). They show the intended shape:
> a thin native shell over the shared Rust `core/`.

## How it fits together

```
Compose UI (MainActivity)
        │
BluetoothSppTransport  ──implements──►  ByteTransport (UniFFI, from core/src/ffi.rs)
        │                                        │
Android BluetoothSocket (RFCOMM/SPP)      openplug-core (TLS-SRP + protocol)
```

The Rust core exposes a small facade (`core/src/ffi.rs`, feature `ffi`) through
**UniFFI** proc-macros — there is no UDL file. Kotlin implements `ByteTransport` on a
`BluetoothSocket` and then uses:

```kotlin
val dongle = Dongle.connect(transport, pin)   // TLS-SRP with the label PIN
dongle.subscribeDefaults()                    // the original app's 17 parameters
while (true) show(dongle.nextReadings())      // blocking — background thread only
```

## Build (intended)

Prerequisites: `rustup` with the Android targets (`aarch64-linux-android`,
`armv7-linux-androideabi`), the Android NDK (`ANDROID_NDK_HOME`), `cargo-ndk`, and the
Android SDK + Gradle.

```sh
cd core
# 1. Build the Rust core for Android
cargo ndk -t arm64-v8a -t armeabi-v7a -o ../android/app/src/main/jniLibs \
    build --release --features ffi
# 2. Generate the Kotlin bindings (package com.openplug.core, see core/uniffi.toml)
cargo build --features ffi
cargo run --features bindgen --bin uniffi-bindgen -- generate \
    --library target/debug/libopenplug_core.dylib --language kotlin \
    --out-dir ../android/app/src/main/java
# 3. cd ../android && ./gradlew assembleDebug
```

Steps 1 and 2 are verified (the core cross-compiles for both ABIs with NDK r29, and
the bindings generate for Kotlin and Swift); step 3 is not — the Gradle project itself
does not exist yet. The generated bindings need the JNA
dependency (`net.java.dev.jna:jna:<version>@aar`).

## Permissions
`BLUETOOTH_CONNECT`, `BLUETOOTH_SCAN` (Android 12+), plus `ACCESS_FINE_LOCATION`
for trip logging. Pairing uses the PIN printed on the dongle label.

## TODO
- Gradle project, manifest, and a `DongleViewModel` running the read loop.
- Device picker (bonded devices named `DP_2_1_…`) and PIN entry.
- Foreground service to keep the connection alive while driving.
