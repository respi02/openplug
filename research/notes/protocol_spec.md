# OpenPlug — DataPlug protocol specification (offline reconstruction, 2026-11-12)

All derived offline from our own APK + one of our own HCI captures (CZ § 66(1)(d)).
Library vendor: **DAT / CYMOTIVE** (Java package `com.dat.datlib`, not obfuscated;
native `dat::` in `libdongles.so`). See protocol_findings and build_plan.

## Layer stack

```
┌ app (de.volkswagen.avacar)            trip logic, scoring, UI
├ com.dat.datlib (Java)                  Connection / ConnectionImpl (JNI)  ← not obfuscated
├ libdat.so (JNI bridge)                 Java_com_dat_datlib_ConnectionImpl_lib_*
├ libdongles.so (C++, namespace dat::)   transactions, protobuf (nanopb), reassembly
│    ├ nanopb protobuf                   _TDongleReq / _TDongleRsp (+ fragments)
│    └ dat::SecureTunnel ──► libmbedtls.so  TLS-SRP (mbedTLS 2.4.0)
└ Bluetooth RFCOMM / SPP                  serial channel phone↔dongle
```

## 1. Transport
- **Bluetooth Classic, RFCOMM/SPP** (not BLE). Dongle `DP_2_1_<serial>`, paired via
  the PIN on the label (BT pairing).
- Over SPP runs **TLS 1.2**, cipher **0xC01D `TLS_SRP_SHA_WITH_AES_128_CBC_SHA`**,
  no certificates.
- Plaintext boundary in the code: `dat::SecureTunnel::send/recv` →
  `mbedtls_ssl_write/read`. Below that `DTPStream` (fragmentation) and
  `InputBufferedStream`.

## 2. Authentication (SRP / PSK, RFC 5054)
- `ConnectionImpl.lib_authenticateByPIN(String identity, String pin)` → SRP
  identity **`PA`**, password = **the PIN on the label**.
- On success the dongle issues a **token** (`getToken`/`onTokenStore`); later
  connections use `authenticateByToken` with identity `SRP_TOKEN_…` (no PIN).
- `dat::Configuration` identities: `SRP_PIN` (="PA"), `SRP_TOKEN`, `PSK_PIN`,
  `PSK_TOKEN`. This dongle used SRP; a PSK branch also exists (`mbedtls_ssl_conf_psk`).
- State: `isAuthenticatedByPIN()`, `isAuthenticatedByToken()`, `isSessionEstablished()`.

## 3. Message framing (nanopb protobuf)
- Top-level: **`_TDongleReq`** (phone→dongle) and **`_TDongleRsp`** (dongle→phone).
- Responses are assembled from fragments: `_TSubFragment` →
  `DongleSocket::ReassembleSubFragment` → `getReassemledRsp(_TDongleRsp*)`. Large
  data (`DataChunk`/`PduChunk`) are aggregated by `DataAggregator`/`PduAggregator`
  via `_TDongleRspMetaHeader` + `_TDongleRspBlockDescriptor`.
- Each request has a `transactionId` (`TransactionManager::reserveId`); the response
  is matched via `isMessageTypeExpected(type)`.
- **Java mirror of the schema:** `o/P6F4jn8H6L.java` = 72 protobuf-java messages
  (only the names are obfuscated). Field numbers are readable from the `readTag()`
  branches (tags: field 1–10, wire types 0/2/5). → the full `.proto` is mechanically
  reconstructable from here; assign semantic names from the `dat::` symbols and the
  plaintext capture.

## 4. Commands (JNI `com.dat.datlib.ConnectionImpl`, native `dat::Connection`)

| Command | Meaning |
|---|---|
| `connect` / `disconnect` | establish/tear down (SPP+TLS) |
| `authenticateByPIN(id,pin)` / `authenticateByToken(tok)` | SRP authentication |
| `getToken` / `resetToken` | token management |
| `getDataObject(url, …)` / `subscribeObject(url, rate, …)` / `unsubscribeObject(url)` | read/subscribe **data objects** (live values) |
| `sendPDU(id, protocol, …, params[], requests[], …)` | **raw diagnostics** (OBD/UDS/KWP) on CAN |
| `getSupportedDiagParameters` / `getAvailableDiagParameters` | the diagnostic-parameter catalog (from the dongle at runtime) |
| `getDiagStatus` / `getDiagConfigReport` / `getActivationStatus` | status/config/activation |
| `getFirmwareStatus` / `getDongleHWRevision` / `getDongleSerialNumber` / `getVersion` / `getCymotiveVersion` | dongle identity |
| `startUpdate`/`sendUpdateData`/`flashChunk`/`completeUpdate`/`cancelUpdate` | flash the configuration (signed blob, see §6) |
| `readMemoryAsync`, `switchToServiceMode` | service |

## 5. PDU layer (reading the vehicle) — `com.dat.datlib`
- **`PDUProtocol`**: `ISOUDSonCAN(0)`, `KWPonCAN(1)`, `ISOOBDonCAN(2)`, `VW2000LPonTP20(3)`, `NO_PROTOCOLTYPE(5)`.
- **`TPDURequest`**: `byte[] DataField` (the raw UDS/OBD request), `CP_P2Max`
  (timeout), `CP_RepeatReqCountApp`, `ContinueIfFail`, `index`.
- **`TPDUCommunicationParameter`**: `{int ID, int Value}` (CAN communication parameters).
- **`PDUResponse`**: `{respID, ErrorCode, FragmentID, rows[]}`; a row =
  `{PDUIndex, Timestamp, ErrorCode, CANID, byte[] DataField}`. (fragment type 23 = PDUFragment.)
- **`PDURequestResult`**: `OK, CanIdError, CanSpeedError, CanProtocolError, VehicleSpeedInvalid`.
- Conclusion: the dongle is a **CAN/OBD gateway** — you give it a UDS/OBD PDU, it
  puts it on the bus and returns responses with CANID + data. Exactly what OpenPlug
  reimplements.

## 6. Data-object model — `com.dat.datlib.DataElement`
- `DataObject { int url; DataElement[] }`, `DataElement { int url, int eType,
  Object value, int precision, int state, String msg }`.
- `eType`: ABSOLUTE0, ACTIVITY1, ALTERNATIVE2, BINARY3, ENUMERATION4, LISTENTITY5,
  OBJECTENTITY6, RELATIVE7, TEXT8, TIME9.
- `state`: OK0, ERROR2, NODATA4.
- `url` = numeric object ID (RPM, battery voltage, mileage…). **The url→signal
  mapping is not a static constant** — it comes from the dongle
  (`getSupportedDiagParameters`) or the signed configuration blob.

## 7. States and errors
- **`DiagnosticStatus`**: 1 DeviceIsBusy, 2 ConfigurationVerifyInProgress, 3
  ConfiguredAndNotConnected, 4 NotConfiguredAndEOBDcommunication, 5
  ConfiguredAndConnected, 6 NotConfiguredAndNotConnected, 7 ErrorCorruptedDevice,
  8 ErrorMemoryProblem, 9 Unknown, 10 ConnectionReset, 11 ProtobufDecodeError.
- `DongleStatus_e`, `AuthenticationResult` (native enums).

## 8. Configuration blob (not needed for reading)
- `assets/mappingtable/.../configuration_update_blob.dat` (116 kB) + `.sig` (132 B,
  likely ECDSA). A protobuf with a leading encrypted/hashed field. Flashed to the
  dongle (`ConfigurationUpdateTransaction`), **signed → not forgeable**. OpenPlug
  does not need it to read the vehicle (PDU + data objects suffice).

## What remains (needs one live plaintext capture)
1. Fill in the semantic names and exact field layouts of the 72 protobuf messages
   (Java schema + plaintext).
2. The `url` data-object catalog ↔ specific signals (from `getSupportedDiagParameters` at runtime).
3. Verify the handshake→auth→subscribe sequence on real bytes.
→ Plan: a Frida hook on `mbedtls_ssl_read/write` on the running app with ignition on.

## Feasibility status: GREEN
No server. Everything is phone↔dongle, standard TLS-SRP + protobuf + OBD/UDS. A
replacement is achievable purely client-side, with the PIN on the label as the only
secret, which we already have.
