# OpenPlug — Android app

> **Status: builds and installs; not yet run against a real dongle.** A thin Compose
> shell over the shared Rust `core/`: pick a paired dongle, enter the label PIN, see
> the live values. Debug builds also offer a TCP "fake dongle" target for development.

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

## Build

Prerequisites: `rustup` with the Android targets (`aarch64-linux-android`,
`armv7-linux-androideabi`), `cargo-ndk`, the Android NDK (`ANDROID_NDK_HOME`), the
Android SDK with platform 37 (`ANDROID_HOME`), and JDK 17 or 21.

```sh
cd android
./build-core.sh            # Rust core -> app/src/main/jniLibs + Kotlin bindings
./gradlew assembleDebug    # -> app/build/outputs/apk/debug/app-debug.apk
```

`build-core.sh` outputs are build products (gitignored): the native libraries and
the UniFFI-generated `com/openplug/core/openplug_core.kt`.

## Try it without a car

```sh
(cd core && cargo run --example fake_dongle)   # on the computer
adb reverse tcp:4455 tcp:4455
```

Then in a debug build choose **Fake dongle (TCP 127.0.0.1:4455)**, PIN `1234`.

## Permissions
`BLUETOOTH_CONNECT` only (Android 12+): the app talks to an already bonded dongle, so
it needs neither scanning nor location. Debug builds add `INTERNET` for the fake
dongle. Pair the dongle (`DP_2_1_…`) in the system Bluetooth settings first.

## TODO
- First run against real hardware.
- Foreground service to keep the connection alive while driving.
- Trip logging, statistics, and labels for more data objects.
- The PIN is remembered in app-private preferences in plain text; move it to the
  Keystore, or switch to the dongle's token authentication once that is decoded.
