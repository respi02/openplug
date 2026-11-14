# OpenPlug — Android app

> **Status:** live values work against a real dongle (first live run 2026-11-13).
> Everything added since — the three-tab UI, the trip log, running in the background,
> routes and maps, the live notification — runs on a phone against the fake dongle,
> **not yet on a real drive.** Debug builds offer a TCP "fake dongle" target for that.

## How it fits together

```
Compose UI (MainActivity: Car · Trips · Settings)
        │  observes
Session (in DongleService, a foreground service) ── TripJournal, ValueRecorder (core)
        │
BluetoothSppTransport  ──implements──►  ByteTransport (UniFFI, from core/src/ffi.rs)
        │                                        │
Android BluetoothSocket (RFCOMM/SPP)      openplug-core (TLS-SRP + protocol)
```

| File | Role |
|---|---|
| `MainActivity.kt` | the theme, the three tabs, shared UI pieces |
| `CarTab.kt`, `TripsTab.kt`, `TripDetail.kt`, `SettingsTab.kt` | the screens |
| `Session.kt` | the one dongle session: connect, stream, wait and reconnect; feeds the journal |
| `DongleService.kt` | the foreground service and its live notification |
| `Presence.kt` | notices the dongle advertising (BLE scan), resumes after boot or update |
| `PositionFeed.kt` | the phone's position while connected, for routes |
| `Settings.kt` | stored choices (dongle, PIN, switches) |
| `transport/` | RFCOMM/SPP, and TCP for the fake dongle |

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

Then in a debug build, in **Settings**, choose **Fake dongle (TCP 127.0.0.1:4455)**,
PIN `1234`, and Connect. The phone has to stay on USB for `adb reverse`. The
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

## Live notification

The service's ongoing notification follows the session: the trip in progress
("Trip · 12.4 km", "62 km/h · 18 min · 6.1 l/100 km"), updated every 3 s, otherwise what
the session is waiting for. On Android 16+ (API 36) it asks to be promoted to a
**Live Update** (status-bar chip with the distance, lock screen) and declares
`POST_PROMOTED_NOTIFICATIONS`; that path is untested, as our phone runs Android 14,
where it is an ordinary ongoing notification.

## The PIN

The PIN is kept in the app's preferences encrypted with AES-GCM under a key in the
Android Keystore (`PinVault`), which never leaves it. The key needs no user
authentication, since a session resumes on its own in the background. A PIN stored in
plain text by an older build is sealed and removed on first read. App backup is off.

## Routes and maps

With **Record routes** on (Settings; off by default), the app takes the phone's
position while the car is connected — from the platform location service, no Google
services — and the core stores each trip's route and the place the car was left. The
position never leaves the phone and is not written to value recordings. Trip detail
and the parking card draw OpenStreetMap tiles with
[osmdroid](https://github.com/osmdroid/osmdroid) (no API key; tiles are fetched from
the network and cached). "Allow all the time" is needed for drives recorded after the
session was resumed in the background.

With **Addresses for trips** on (off by default), finished trips get a street address
for their start and end, and the parking card shows one. `Addresses` uses the phone's
geocoder when `Geocoder.isPresent()` (phones with Google services), otherwise
OpenStreetMap's [Nominatim](https://nominatim.org/) at most once a second, as its
usage policy asks. Either way the positions of the trip ends leave the phone; the
switch says so. Found addresses are stored in the trip log.

## Permissions
`ACCESS_FINE_LOCATION` / `ACCESS_COARSE_LOCATION` / `ACCESS_BACKGROUND_LOCATION` and
`INTERNET` only serve routes, maps and addresses (above); location is asked for when
routes are switched on.
`BLUETOOTH_CONNECT` (Android 12+) to talk to the bonded dongle; `BLUETOOTH_SCAN`
(`neverForLocation`) to notice it advertising, optional (without it only timed
retries). `FOREGROUND_SERVICE` / `FOREGROUND_SERVICE_CONNECTED_DEVICE` and
`POST_NOTIFICATIONS` for the session service; `WAKE_LOCK`,
`REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` and `RECEIVE_BOOT_COMPLETED` for running in
the background (above). Pair the
dongle (`DP_2_1_…`) in the system Bluetooth settings first.

## Notes from the first live run
- The label PIN is alphanumeric (ours: 8 capital letters).
- The dongle serves one client. If the original app is installed it reconnects on its
  own and takes the link over — stop or disable it while using OpenPlug.

## TODO
- Run the trip log, reconnection and the foreground service on a phone and a real drive.
- Labels for more data objects.
- Switch to the dongle's token authentication once that is decoded, so the PIN is
  needed only once.
