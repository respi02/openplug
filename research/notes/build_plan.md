# OpenPlug — state and plan (updated 2026-11-14)

Where the project stands and what comes next. The reasoning behind the architecture
is in [`tech_stack.md`](tech_stack.md); the protocol is in
[`protocol_spec.md`](protocol_spec.md), [`live_protocol.md`](live_protocol.md) and
[`ble_transport.md`](ble_transport.md).

## Established facts (all confirmed on real hardware)
- **Transport:** Bluetooth Classic RFCOMM/SPP (used by Android) and BLE (used by iOS
  and macOS). The dongle is dual-mode, not MFi. Both carry the same bare TLS records;
  no extra framing on either.
- **Secure tunnel:** TLS 1.2, `TLS_SRP_SHA_WITH_AES_128_CBC_SHA` (0xC01D), no
  certificates, Encrypt-then-MAC + Extended Master Secret. The dongle sends
  ServerHello, ServerKeyExchange and ServerHelloDone as three separate records.
- **SRP:** RFC 5054 1536-bit group, g=2; identity `PA`; password = the PIN on the
  label (alphanumeric — ours is 8 capital letters); salt from the dongle.
- **Application protocol:** length-prefixed protobuf. The client subscribes to
  parameter groups; the dongle streams data objects. No further authentication
  exchange follows the TLS handshake.
- **One client at a time.** A second connection takes the link over, and the dongle
  does not advertise over BLE while a Classic client holds it.

## What exists
| Piece | State |
|---|---|
| `core/` — tunnel, protocol, read API, UniFFI facade | done; offline tests + a real dongle |
| `core/` — trip journal (trips, refuels, last values, routes, parking, CSV), recording and replay | done; offline tests |
| `core/examples/` — `live`, `fake_dongle` (synthetic or replaying a recording), `replay` | done |
| `android/` — SPP transport, session service, Compose UI (Car · Trips · Settings) | done; live values from a real dongle, the rest against the fake dongle |
| `apple/` — Core Bluetooth transport, macOS CLI, BLE fake dongle | done; live values from a real dongle |
| `apple/App/` — SwiftUI app for iOS and macOS, Live Activity extension | done; live values from a real dongle, the rest against stand-ins |

Done since the first live run, all against stand-ins only: the trip log; automatic
reconnection and running in the background (Android: sticky service, BLE presence
scan, boot/update receiver; iOS: pending connect, state restoration); value recording
and replay; the three-tab UI with the car's last known state; routes on a map and the
parking place; the trip in progress as a Live Activity / live notification; the app icon;
a start threshold for trips (3 km/h held for 3 s); filing trips (business/private, a
note), deleting them, addresses for their ends (opt-in); the tank size as a fuel gauge;
the service interval with a countdown and a reminder; the Android PIN sealed with a
Keystore key; CI (GitHub Actions: core tests and clippy, the Android build, the iOS and
macOS builds). The Android UI for these was built but not yet run on the phone.

## Next
1. **A real drive, recorded.** With "Record values" and "Record routes" on. It settles
   at once: whether the trip rules hold (a trip was seen to start with the car
   standing; the start threshold added for that is a guess to check), whether Android's filtered scan matches
   the real dongle's advertisement, how soon a waiting phone notices the car, GPS in the
   background on both platforms, and the Live Activity on a phone. The recording then
   becomes a test fixture (`examples/replay`, `fake_dongle --replay`).
2. **Trip log, next features.** Manual trips; refuel price and station; a reminder
   as a system notification, not only in the app; decoding group 9 (the car's own
   service interval status) would make the interval setting unnecessary.
3. **Protocol coverage.**
   - Decode the groups the app subscribes to that we do not label yet: 9 (service
     interval status), 11 (warning lights), 17 (boost pressure), 34 (catalyst
     temperature), 44 (AdBlue).
   - The getters the app sends before subscribing (`f15{sel}`, empty `f1/f4/f5/f22`):
     their responses are not decoded.
   - Token authentication, so the PIN is not needed on every connect.
   - The raw OBD/UDS request path (`sendPDU`), which the app does not use for live
     values.
4. **Polish and distribution.** Signed Android releases; TestFlight (needs a paid
   developer account; with a free one the iOS app has to be reinstalled every 7 days).

## Open questions
- Token lifecycle: how long a token stays valid and whether it is bound to a phone.
- What the `0x…253` "secured" block in each response protects, and whether any value
  depends on it (none seen so far).
- How much of the data-object catalog differs between vehicles; ours is one car.
- Dongle sleep behaviour with the ignition off (it was reachable over BLE at some
  times and not at others).
- iOS starts a Live Activity only from the foreground; whether anything short of a
  push server lets a trip that begins in the background show one.
