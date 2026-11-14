# OpenPlug — Apple platforms (iOS / macOS)

> **Status: works against a real dongle (2026-11-13).** The SwiftUI app (one target
> for iOS and macOS) shows the car's live values on an iPhone over BLE; the macOS
> command-line client does the same from a Mac. The app also runs on macOS and in the
> iOS simulator against the fake dongle.
>
> Newer than that verification: the **trip log** and **reconnecting in the
> background**. Exercised on an iPhone against a BLE stand-in for the dongle (below),
> not yet on a real drive.

## What is here

| | |
|---|---|
| `Sources/OpenPlugBLE/` | the Core Bluetooth transport, shared by iOS and macOS |
| `Sources/openplug-cli/` | macOS command-line client |
| `Sources/openplug-fake-ble/` | a BLE stand-in for the dongle on a Mac, for testing the iOS app without the car |
| `App/` | the SwiftUI app: PIN entry (kept in the Keychain), live values, trips and refuels |
| `project.yml` | XcodeGen spec for the app's Xcode project |
| `build-core.sh` | builds the Rust core into `OpenPlugCoreFFI.xcframework` + Swift bindings |

## Build

Prerequisites: Xcode, `rustup` with the targets `aarch64-apple-darwin`,
`aarch64-apple-ios` and `aarch64-apple-ios-sim`, and [XcodeGen](https://github.com/yonaskolb/XcodeGen)
for the app. Apple silicon only for now.

```sh
cd apple
./build-core.sh              # Rust core -> OpenPlugCoreFFI.xcframework + bindings
swift build                  # the package and the command-line client
xcodegen generate            # -> OpenPlug.xcodeproj
open OpenPlug.xcodeproj      # run the "OpenPlug" scheme on My Mac or an iPhone
```

`build-core.sh` outputs and the generated Xcode project are build products
(gitignored). Running on an iPhone needs the iOS platform installed in Xcode and a
signing team selected for the target (a free personal team works; the phone then has
to trust the developer profile under Settings → General → VPN & Device Management).

Command-line client:

```sh
OPENPLUG_PIN=<PIN from the label> .build/debug/openplug-cli --seconds 30
```

## Try the app without a car

```sh
(cd core && cargo run --example fake_dongle)
```

In a debug build choose **Fake dongle (TCP 127.0.0.1:4455)**, PIN `1234`. This works
from the Mac app and from the iOS simulator. For scripted checks a debug build also
accepts `--autoconnect bluetooth|demo [--record]` with the PIN in `OPENPLUG_PIN` and
traces the session to stdout (`OPENPLUG_JOURNAL` and `OPENPLUG_RECORDINGS` override
where the trip log and recordings go). The fake dongle has a trip log of its own
(`journal-demo.json`), so its synthetic values never mix with the car's.

## Recordings

With **Record values** on (default in debug builds), each session is written to
`drive-<date>-<time>.jsonl` in the app's Documents: on an iPhone in the Files app
under *On My iPhone › OpenPlug › Recordings* (AirDrop it to the Mac from there), on a
Mac in `~/Documents/OpenPlug Recordings`. From a Mac with the iPhone attached:

```sh
xcrun devicectl device copy from --device <id> --domain-type appDataContainer \
    --domain-identifier org.openplug.app --source Documents/Recordings --destination recordings
```

Replay one into the trip log with `cargo run --example replay -- FILE`, or play it to
the app with `cargo run --example fake_dongle -- --replay FILE` (see the main README).

## Running in the background

Once connected, the session stays on until **Disconnect**, so every drive is recorded
without opening the app:

- One shared `CBCentralManager` (`DongleCentral`) with a **restore identifier**,
  created at launch.
- After a connection is lost, the app at once places a **pending connect** to the
  dongle it knows (remembered by identifier) plus a scan for the dongle's service.
  Neither times out, and iOS completes them while the app is suspended. The session
  thread never sleeps before that, since a suspended app's sleeping thread would not
  wake.
- If iOS terminated the app meanwhile, **state restoration** relaunches it in the
  background when the dongle connects; the model, created at launch, resumes the
  stored session. The PIN is kept readable after the first unlock, so this works with
  the phone locked. (iOS does not relaunch an app the user swiped away.)

Verified on an iPhone with `openplug-fake-ble`: reconnecting after a minute in the
background, and a relaunch after the app was killed while waiting, with the phone
locked.

```sh
(cd core && cargo run --example fake_dongle)   # the dongle's TLS end (PIN 1234)
cd apple && swift run openplug-fake-ble         # advertise it over BLE from the Mac
```

Stopping `openplug-fake-ble` looks to the phone like the ignition going off.

## The BLE transport

Details and the investigation: [`research/notes/ble_transport.md`](../research/notes/ble_transport.md).

| | UUID |
|---|---|
| service | `732db477-bfd6-4694-9378-86c246f35def` |
| data characteristic (write + read) | `478a86f3-e47c-4618-9b15-3c7c92cc8184` |
| doorbell characteristic (notify) | `2efb2dd4-cdc3-11e6-9d9d-cec0c932ce01` |

- **Send:** write each TLS record whole to the data characteristic, with response.
- **Receive:** every one-byte `0xff` notification on the doorbell means one chunk is
  waiting; read the data characteristic to fetch it. Chunks are at most 512 bytes and
  do not respect record boundaries, so the reads are treated as a byte stream.
- Find the dongle by its service UUID, not by name (seen: `DP_2_1_<serial>` and
  `CARe`). It is not MFi and needs no bonding. It serves one client and stops
  advertising while a phone is connected to it.

`ByteTransport.recv` blocks while Core Bluetooth is callback-driven: the transport
queues the bytes of each completed read and `recv` waits on that queue from the
session's background thread.
