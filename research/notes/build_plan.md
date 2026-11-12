# OpenPlug — build plan and preparation status (2026-11-12)

Consolidates protocol_spec, protocol_findings, tech_stack, code_licensing. What is
certain, what remains, and in what order.

## Established facts (offline-confirmed)
- **Transport:** Android = BT Classic RFCOMM/SPP; iOS = BLE (Core Bluetooth). The
  dongle is dual-mode (FCC T8RDP15/T8RDP22: DTS+DSS grants), **not MFi**. Both use
  the same tunnel, only a different "pipe".
- **Secure tunnel:** TLS 1.2, cipher `TLS_SRP_SHA_WITH_AES_128_CBC_SHA` (0xC01D),
  mbedTLS 2.4.0, no certificates, with Encrypt-then-MAC + Extended Master Secret.
- **SRP:** RFC 5054 **1536-bit** group, g=2; identity **`PA`** (PIN auth) / a token
  after first pairing; password = **the PIN on the label**; salt comes from the
  dongle in the handshake.
- **Application protocol:** nanopb protobuf, `_TDongleReq`/`_TDongleRsp` +
  fragmentation; below that the OBD/UDS PDU layer. The full API is in
  `com.dat.datlib` (not obfuscated) + `dat::` in libdongles.so.

## Architecture (decided)
Shared **Rust core** (UniFFI → Kotlin+Swift) · platform **transport shim** · thin
native **UI**.
```
core (Rust):  Transport trait │ TlsSrpSession │ DongleCodec(protobuf) │ TransactionMgr │ PduBuilder(UDS/OBD) │ DataObjects
                     ▲                                   api via UniFFI ▼
android: BluetoothSocket(SPP) ──┐                 Kotlin+Compose UI
ios:     CoreBluetooth(GATT)  ──┘                 SwiftUI UI
```

## Mapping the dat::/datlib API → core modules
| Core module | Covers |
|---|---|
| `Transport` trait (`send/recv bytes`) | `IIOStream`, `DTPStream`, `InputBufferedStream` |
| `TlsSrpSession` | `SecureTunnel`, TLS-SRP |
| `DongleCodec` | `_TDongleReq/Rsp`, `_TSubFragment`, `DongleSocket` reassembly, `DataAggregator`/`PduAggregator` |
| `TransactionMgr` | `TransactionManager`, `*Transaction` (Pin/Token/Pdu/DataSubscription/Simple/ConfigUpdate) |
| `Auth` | `authenticateByPIN/Token`, `getToken/resetToken` |
| `PduBuilder` | `sendPDU`, `PDUProtocol`, `TPDURequest`, `PDUResponse` |
| `DataObjects` | `subscribeObject/getDataObject`, `DataElement` (eType/state) |
| `Info` | `getVersion/HWRevision/SerialNumber/DiagStatus/ActivationStatus` |

## Protobuf schema status
- `o/P6F4jn8H6L.java` holds **72 protobuf-java messages** (standard
  GeneratedMessage, obfuscated names). Field numbers + types are in
  `writeTo()`/`readTag()` — certain and readable.
- **The semantic names are not in the obfuscated Java.** A purely static
  extraction is possible but needs a proper Java parser (obfuscated Builder/message
  names + jadx artefacts defeat a quick regex) — not worth it.
- **Chosen path:** one **plaintext capture** → `protoc --decode_raw` gives field
  numbers/types/nesting unambiguously → assign names by the operation that produced
  each message (getVersion, subscribe, sendPDU…) and by the `_T*` names in
  libdongles.so. Then `.proto` → `prost`.

## Work order
### Done offline (2026-11-12)
- Protocol mapped; TLS-SRP params found; transport decided for both platforms;
  API→modules mapped; **crypto core built and verified**.
- `core/` Rust: `transport` + test pipe; **SRP-6a** (RFC 5054 vector + 1536-bit
  dongle group); **TLS 1.2 PRF-SHA256** (known-answer); **TLS record** Encrypt-then-MAC;
  **TLS handshake** (master secret, EMS, ServerKeyExchange parser verified against
  real dongle bytes); **tls_client** full TLS-SRP handshake with a loopback test;
  **generic protobuf** codec. `cargo test` → all pass.

### Remaining offline (no dongle)
1. Desktop test harness: a BT Classic connection from the Mac (USB BT adapter) as a
   "fake phone" — prepare the code, run it with the dongle.
2. UniFFI interface (API from the table above), stubs for the upper layers.
3. Platform transport skeletons (Android RFCOMM, iOS BLE).

### Needs the dongle (when there's time/quiet)
4. **BLE scan** of the powered dongle → confirm LE and the service/characteristic
   UUIDs for the iOS transport.
5. **One plaintext capture** → the `.proto` + the `url` data-object catalog.
6. Verify the handshake→auth→subscribe/PDU sequence on real bytes; finish `DongleCodec`.
7. Android shim + UI (end-to-end), then iOS (BLE) + SwiftUI.

## Open questions
- Does the same TLS-SRP+protobuf tunnel run over BLE GATT? (very likely; confirm).
- Token lifecycle: how long valid, where the dongle binds it (per-phone?).
- `url` → specific vehicle signal: from `getSupportedDiagParameters` at runtime.
