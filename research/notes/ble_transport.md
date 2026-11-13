# OpenPlug — BLE transport, CONFIRMED on live hardware (2026-11-13)

Previously the iOS/BLE path was **unverified** (our HCI capture was 100% Bluetooth
Classic SPP, 0 BLE frames). It is now confirmed by a live GATT enumeration and an
active probe of a powered dongle, done from macOS CoreBluetooth (Python `bleak`).

## The dongle advertises BLE

- Advertised as a dual-mode peripheral, name `DP_2_1_<serial>` (same `DP_2_1_…`
  pattern as the Classic SPP side), strong RSSI next to the car's OBD port.
- Advertisement carries Apple iBeacon manufacturer data (type 0x02 0x15 …). That is
  just advertising; the usable channel is the GATT service below.
- Confirms the FCC dual-grant finding (DTS=LE + DSS=Classic): one radio, both modes.
  iOS uses this LE side.

## GATT layout (connected, enumerated)

```
SERVICE 0000180a-…  Device Information
  2a27 Hardware Revision   [read,write]
  2a26 Firmware Revision   [read,write,notify]
  2a25 Serial Number       [read,write]
  2a29 Manufacturer Name   [read,write]
  2a28 Software Revision   [read,write]
  (all read back empty before authentication)

SERVICE 732db477-bfd6-4694-9378-86c246f35def   ← vendor "serial-over-BLE"
  478a86f3-e47c-4618-9b15-3c7c92cc8184   [read,write]   phone → dongle   (WRITE)
  2efb2dd4-cdc3-11e6-9d9d-cec0c932ce01   [notify]       dongle → phone   (NOTIFY)
    + CCCD 2902
```

This write/notify pair is the BLE equivalent of the Classic SPP channel. The
transport-agnostic Rust core plugs onto it via a thin BLE shim. Negotiated ATT MTU
on macOS was 185 B (so a ~83 B record fits in a single write).

## Active probe result — there is an extra framing layer over TLS

Wrote one real `ClientHello` record emitted by our core
(`core/examples/clienthello.rs`, 83 B, identity `PA`, cipher 0xC01D) to the WRITE
characteristic in a single write, with NOTIFY subscribed.

- The write was accepted; NOTIFY delivered a response → the channel is live and the
  dongle reacts.
- Response was **`ff ff ff`** (three 1-byte notifications), **not** a raw TLS
  ServerHello (`16 03 03 …`).

Conclusion: the dongle does **not** take raw TLS bytes directly on this
characteristic. There is a **DTP link-framing layer** between the GATT characteristic
and the TLS stream. `0xff` is a NAK / "bad frame".

## DTP link framing — REVERSED from the vendor libs (2026-11-13)

From `libdongle.so` (`dtp_add_header`, `dtp_from_net`, `dtp_needed_bytes`, `dtp_len`,
`dtp_data`) and `dat::DTPStream::send` in `libdongles.so`. One frame per `send()`:

```
  +--------+--------+------+------+---------------- ... ----------+
  | len hi | len lo | 0x44 | 0x5e |   payload (len bytes)         |
  +--------+--------+------+------+---------------- ... ----------+
    \__ u16 big-endian __/  \_ marker _/
```

- `len` = payload length (big-endian), excluding the 4-byte header.
- `dtp_add_header` writes `buf->len` via `rev16` (→ big-endian) then the constant
  `0x5e44` (stored little-endian → wire bytes `44 5e`).
- `dtp_needed_bytes` collects 4 header bytes, reads the big-endian length, then waits
  for `len + 4` total; it reassembles across packets and does **not** validate the 2
  marker bytes. `dtp_data` returns `buf + 4` (payload after the header).
- `DTPStream::send` hands header+payload to the lower stream in a single write, so on
  BLE one DTP frame goes out as one GATT write (fits the 185 B MTU for the handshake).

Stack on the wire: `protobuf → TLS(SecureTunnel) → DTPStream(this framing) → BLE/SPP`.

Implemented in the core as `openplug_core::dtp` (`frame`, `Deframer`,
`DtpTransport<T>`), tested in `core/tests/dtp.rs`. The 83 B ClientHello frames to
header `00 53 44 5e`.

## Live probe results (2026-11-13, engine running) — framing right, dongle silent

Probes from macOS (`captures/ble-tools/`), dongle powered, engine on:

- Raw TLS (no DTP) written to the WRITE char → `ff ff ff`. Explained by
  `dtp_from_net`: it checks the marker at offset 2 == `0x445e`; raw TLS has `03 00`
  there → returns -1 → the layer above answers `0xff` (NAK).
- **Correctly DTP-framed** ClientHello (valid `44 5e` marker, valid length) → **no
  response at all**, on either notify char (`2efb2dd4` and the DIS `2a26`), via
  read-polling the WRITE char, with write-with-response and write-without-response,
  and as one 93 B write or as 20 B ATT chunks. The frame is accepted (no NAK) but
  produces no TLS ServerHello.
- Decisive test: we made our ClientHello **byte-for-byte identical** to the real
  app's (see below) and sent exactly those bytes — still silence. So the blocker is
  **not** the TLS/ClientHello content; it is **BLE-session-level**: the real iOS app
  must do something on connect we don't (candidates: BLE bonding/pairing with the
  label PIN as passkey, a session-open control write, a specific notify-enable /
  init sequence). We have no reference for it.

**No offline reference exists for the BLE session-open.** The Android APK is
SPP-only (`Rfcomm`, `BluetoothSocket`; `BluetoothLeScanner` is used only to *find*
the dongle — no `BluetoothGatt`/`writeCharacteristic`, and the `732db477…` UUIDs are
absent). The GATT data path is iOS-app-only, and we have neither the iOS IPA nor a
BLE capture of it. Reversing it further is a rabbit hole without that reference.

## ClientHello correction (from the SPP capture)

Comparing our hand-built ClientHello to the real app's (extracted from
`captures/dataplug-pairing-2026-11-12.btsnoop`, SRP identity `PA` confirmed) found
our earlier version was not byte-exact. Fixed in `build_client_hello`, now identical:
- record version `0x0301` on the initial ClientHello (was `0x0303`),
- cipher list `C01D 00FF` — adds `TLS_EMPTY_RENEGOTIATION_INFO_SCSV` (was `C01D` only),
- supported_groups `secp256r1 + secp521r1 (0017 001a)` (was `0017 0018`),
- adds an empty `session_ticket (0x0023)` extension.
This is correct for the SPP path too, so it is a net win regardless of BLE.

## Decision: Android/SPP first, iOS/BLE deferred

The SPP transport is fully understood and matches this whole protocol stack; the
Android OpenPlug needs no BLE. Build and validate Android/SPP end to end (the core
can be checked against the captured real handshake offline, then run live over
RFCOMM from the phone). Revisit iOS/BLE only with an iOS-app traffic reference, or a
deliberate empirical study of the dongle's BLE session-open (bonding first).

Raw logs (gitignored): `captures/gatt_dump.txt`, `captures/probe{1..7}*.txt`.
