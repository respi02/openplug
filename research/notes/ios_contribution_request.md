# OpenPlug — iOS-side contribution request (for a collaborator's coding agent)

**Audience:** a coding agent working for a collaborator who legitimately has the
retired OEM companion app on their Apple ID and can run **local static analysis** of
its iOS build (IPA). The collaborator no longer has the dongle hardware (it was
transferred to this project's maintainer, who has it and a second vehicle), so **no
live Bluetooth capture is possible on their side — this is a pure static-analysis
request.** The live BLE testing is done here, with the dongle.

## What OpenPlug is

A clean-room, **interoperability** reimplementation of the retired companion app for
the "DataPlug" OBD-II Bluetooth dongle, so an owner can keep reading their own
vehicle's data after the app's shutdown (1 Jul 2025). No cloud — everything is local,
phone ↔ dongle. Legal basis: the decompilation-for-interoperability exception
(EU Directive 2009/24/EC Art. 6; CZ Copyright Act § 66(1)). See `research/notes/` for
the full write-up.

## Where the project already is (context for you)

- **Android / Bluetooth Classic SPP path: fully reversed and verified.** We hooked
  the TLS plaintext and decoded the application protocol (`research/notes/live_protocol.md`):
  length-delimited protobuf, `Subscribe{paramId, rate}` → a stream of
  `DataElement{url, value}`. The security layer (TLS-SRP + a "DTP" link framing) is
  reimplemented in a transport-agnostic Rust core (`core/`), offline-tested.
- The dongle is **dual-mode** Bluetooth: Android uses Classic SPP; **iOS uses BLE
  (Core Bluetooth)**.
- On BLE (confirmed with our own dongle) it advertises as `DP_2_1_<serial>` and
  exposes a vendor "serial-over-BLE" GATT service:
  - service `732db477-bfd6-4694-9378-86c246f35def`
  - **write** char `478a86f3-e47c-4618-9b15-3c7c92cc8184` (phone → dongle)
  - **notify** char `2efb2dd4-cdc3-11e6-9d9d-cec0c932ce01` (dongle → phone)
  - plus standard Device Information (0x180A)
- We reversed the **DTP link frame** (`len:u16-BE ‖ 0x44 0x5e ‖ payload`) from the
  vendor native libs, and made our TLS-SRP ClientHello byte-identical to the real
  one (captured over SPP).

### The exact blocker we need iOS help with

Over BLE, the dongle **accepts** our correctly DTP-framed ClientHello (its framing
parser validates it — no NAK) but **never replies** on any characteristic. Sending
even the byte-for-byte real ClientHello changes nothing. So the blocker is **not the
TLS/ClientHello content — it is a BLE-session-level step the iOS app performs on
connect that we cannot observe** without an iOS reference. Full detail:
`research/notes/ble_transport.md`. Leading hypotheses: BLE pairing/bonding
(encrypted link) with the dongle label PIN as the passkey; a session-open / init
write before TLS; or a specific notify-enable / connection-parameter sequence.

## What to extract from the IPA (static analysis)

Find and document the app's Core Bluetooth / connection logic. In priority order:

1. **The connect → session-open sequence**, step by step, from the
   `CBCentralManagerDelegate` / `CBPeripheralDelegate` callbacks up to the first TLS
   handshake byte:
   - After `didConnect` / `didDiscoverCharacteristics`, what happens **before** the
     first TLS ClientHello is written? Any **write to a characteristic** (an
     "open"/"hello"/version/handshake control frame)? Any **read**?
   - The order of `setNotifyValue(true)` vs the first `writeValue(...)`.
2. **Pairing / bonding / encryption** — does the app require an encrypted link? Look
   for a pairing prompt, encryption-required attribute permissions, or a passkey
   derived from the dongle label PIN. (This is our top hypothesis.)
3. **GATT identifiers** — confirm the service + write/notify UUIDs above; list any
   other characteristics it uses (a config/control one, how it uses Device Info).
4. **Write / notify framing at the GATT layer** — how a byte stream is chunked onto
   the write characteristic (chunk size; `.withResponse` vs `.withoutResponse`) and
   how notifications are reassembled. Confirm whether the DTP frame is written as-is
   or wrapped with an **extra per-write header** on BLE. Any MTU /
   `maximumWriteValueLength` handling.
5. **TLS / stream glue** — how the BLE byte stream is fed to the TLS engine (the
   BIO/stream object), and whether the iOS build ships the **same native core** as
   Android (search for `dat::` / `libdongles` / mbedTLS / nanopb symbols) or a
   Swift/ObjC reimplementation.
6. **Info.plist / entitlements** — Bluetooth usage strings, `UIBackgroundModes`
   (`bluetooth-central`), any declared service UUIDs, and the bundle id + version.

## How to deliver

- A written **markdown report** with the above, including the relevant
  **decompiled/disassembled function bodies or class/method names** and any constant
  byte sequences (magic numbers, per-write headers, control-frame opcodes).
  Behavior / pseudocode is what we need — this keeps the result clean-room.
- **Do not commit the IPA or any copyrighted binary to the public repo.** Treat it
  exactly as we treat the Android APK: keep it local and gitignored. The IPA may be
  shared privately with the maintainer for local analysis; the repo deliverable is
  your **findings**, not the binary.
- No Apple-ID credentials, no account secrets, and no App-Store-DRM circumvention —
  only static analysis of a legitimately-held build.

## What you get from us

Our full notes (`research/notes/`: `protocol_spec`, `ble_transport`, `live_protocol`,
`tech_stack`, `build_plan`) and the transport-agnostic Rust core (`core/`). The core
is already transport-agnostic, so once we know the BLE session-open we can test it
live against our dongle immediately: your static findings + our hardware = a finished
iOS transport. The single highest-value deliverable is **the precise connect→first-
TLS-byte sequence (items 1–2)**.
