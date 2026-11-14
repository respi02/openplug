# OpenPlug — Android app

> **Status:** live values work against a real dongle (first live run 2026-11-13).
> Since then the app gained a trip log, automatic reconnection and a foreground
> service; **that version builds but has not been run on a phone yet.** Debug builds
> also offer a TCP "fake dongle" target for development.

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

Then in a debug build choose **Fake dongle (TCP 127.0.0.1:4455)**, PIN `1234`. The
fake dongle has a trip log of its own (`files/journal-demo.json`), so its synthetic
values never mix with the car's.

## Recordings

With **Record values** on (default in debug builds), each session is written to
`Android/data/com.openplug/files/recordings/drive-<date>-<time>.jsonl`, which needs no
storage permission:

```sh
adb pull /sdcard/Android/data/com.openplug/files/recordings
```

Replay one into the trip log with `cargo run --example replay -- FILE`, or play it to
the app with `cargo run --example fake_dongle -- --replay FILE` (see the main README).

## Running in the background

Once connected, the session stays on until **Disconnect**: when the car goes out of
reach it waits for it and reconnects, so every drive is recorded without opening the
app.

- **Foreground service** (`DongleService`, type `connectedDevice`) with an ongoing
  notification; a partial wake lock only while connected.
- **Waking for the car:** timed retries stall while the phone sleeps, so a waiting
  session also hands a filtered BLE scan (the dongle's service UUID, low power) to the
  system with a `PendingIntent`. The dongle advertises over BLE whenever it is powered
  and free; a match wakes `PresenceReceiver`, which connects over SPP at once. Verified
  in forced Doze with a Mac advertising the service UUID; against the real dongle's
  advertisement not yet.
- **Coming back:** that the session is wanted is stored. The service is sticky, so it
  comes back if the system kills the process; it also resumes after a reboot or an app
  update (`RestartReceiver`), when the app is opened, and when the dongle is seen.
- **Battery optimization:** the Live tab asks to lift it. Android lets an app start a
  foreground service from the background only with such an exemption (or a few other
  ones), so without it a resume from a reboot or from the dongle being seen can be
  refused; the sticky restart worked without it on our phone.

## Permissions
`BLUETOOTH_CONNECT` (Android 12+) to talk to the bonded dongle; `BLUETOOTH_SCAN`
(`neverForLocation`) to notice it advertising, optional (without it only timed
retries). `FOREGROUND_SERVICE` / `FOREGROUND_SERVICE_CONNECTED_DEVICE` and
`POST_NOTIFICATIONS` for the session service; `WAKE_LOCK`,
`REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` and `RECEIVE_BOOT_COMPLETED` for running in
the background (above). Debug builds add `INTERNET` for the fake dongle. Pair the
dongle (`DP_2_1_…`) in the system Bluetooth settings first.

## Notes from the first live run
- The label PIN is alphanumeric (ours: 8 capital letters).
- The dongle serves one client. If the original app is installed it reconnects on its
  own and takes the link over — stop or disable it while using OpenPlug.

## TODO
- Run the trip log, reconnection and the foreground service on a phone and a real drive.
- Labels for more data objects.
- The PIN is remembered in app-private preferences in plain text; move it to the
  Keystore, or switch to the dongle's token authentication once that is decoded.
