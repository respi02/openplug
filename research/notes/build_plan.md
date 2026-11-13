# OpenPlug — state and plan (updated 2026-11-13)

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
| `core/` — tunnel, protocol, read API, UniFFI facade | done; offline tests + live |
| `core/examples/live`, `fake_dongle` | done |
| `android/` — Gradle project, SPP transport, Compose UI | done; live values on a phone |
| `apple/` — Core Bluetooth transport, macOS CLI | done; live values from a Mac |
| `apple/App/` — SwiftUI app for iOS and macOS | done; live values on an iPhone over BLE |

## Next
1. **Trip logging** — implemented in `core/src/journal.rs` and both apps, with
   automatic reconnection and background recording; tested against the fake dongle.
   Remaining: prove it on real drives (see `original_app_features.md` for what the
   original app did beyond this).
2. **Protocol coverage.**
   - Decode the groups the app subscribes to that we do not label yet: 9 (service
     interval status), 11 (warning lights), 17 (boost pressure), 34 (catalyst
     temperature), 44 (AdBlue).
   - The getters the app sends before subscribing (`f15{sel}`, empty `f1/f4/f5/f22`):
     their responses are not decoded.
   - Token authentication, so the PIN is not needed on every connect.
   - The raw OBD/UDS request path (`sendPDU`), which the app does not use for live
     values.
3. **Android polish.** Keep the PIN in the Keystore rather than plain preferences.

## Open questions
- Token lifecycle: how long a token stays valid and whether it is bound to a phone.
- What the `0x…253` "secured" block in each response protects, and whether any value
  depends on it (none seen so far).
- How much of the data-object catalog differs between vehicles; ours is one car.
- Dongle sleep behaviour with the ignition off (it was reachable over BLE at some
  times and not at others).
