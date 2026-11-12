# OpenPlug — protokolová specifikace DataPlugu (offline rekonstrukce, 2026-11-12)

Celé odvozeno offline z vlastního APK + jednoho vlastního HCI záznamu (§ 66/1/d AZ). Dodavatel knihovny: **DAT / CYMOTIVE** (Java balík `com.dat.datlib`, neobfuskovaný; nativní `dat::` v `libdongles.so`). Navazuje na [[protocol_findings]] (TLS-SRP) a [[killswitch_analysis]].

## Zásobník vrstev

```
┌ aplikace (de.volkswagen.avacar)      trip logika, scoring, UI
├ com.dat.datlib (Java)                 Connection / ConnectionImpl (JNI)  ← neobfuskováno
├ libdat.so (JNI bridge)                Java_com_dat_datlib_ConnectionImpl_lib_*
├ libdongles.so (C++, namespace dat::)  transakce, protobuf (nanopb), reassembly
│    ├ nanopb protobuf                  _TDongleReq / _TDongleRsp (+ fragmenty)
│    └ dat::SecureTunnel ──► libmbedtls.so  TLS-SRP (mbedTLS 2.4.0)
└ Bluetooth RFCOMM / SPP                 sériový kanál telefon↔dongle
```

## 1. Transport
- **Bluetooth Classic, RFCOMM/SPP** (ne BLE). Dongle `DP_2_1_<serial>`, pár přes PIN ze štítku (BT párování).
- Nad SPP běží **TLS 1.2**, cipher **0xC01D `TLS_SRP_SHA_WITH_AES_128_CBC_SHA`**, bez certifikátů.
- Plaintext hranice v kódu: `dat::SecureTunnel::send/recv` → `mbedtls_ssl_write/read`. Pod tím `DTPStream` (fragmentace) a `InputBufferedStream`.

## 2. Autentizace (SRP / PSK, RFC 5054)
- `ConnectionImpl.lib_authenticateByPIN(String identity, String pin)` → SRP identita **`PA`**, heslo = **PIN ze štítku**.
- Po úspěchu dongle vydá **token** (`getToken`/`onTokenStore`); další spojení `authenticateByToken` s identitou `SRP_TOKEN_…` (PIN se nezadává).
- `dat::Configuration` identity: `SRP_PIN` (="PA"), `SRP_TOKEN`, `PSK_PIN`, `PSK_TOKEN`. Tento dongle použil SRP; existuje i PSK větev (`mbedtls_ssl_conf_psk`).
- Stav: `isAuthenticatedByPIN()`, `isAuthenticatedByToken()`, `isSessionEstablished()`.

## 3. Rámcování zpráv (nanopb protobuf)
- Top-level: **`_TDongleReq`** (telefon→dongle) a **`_TDongleRsp`** (dongle→telefon).
- Odpovědi se skládají z fragmentů: `_TSubFragment` → `DongleSocket::ReassembleSubFragment` → `getReassemledRsp(_TDongleRsp*)`. Velká data (`DataChunk`/`PduChunk`) agregují `DataAggregator`/`PduAggregator` přes `_TDongleRspMetaHeader` + `_TDongleRspBlockDescriptor`.
- Každý request má `transactionId` (`TransactionManager::reserveId`), odpověď se páruje přes `isMessageTypeExpected(type)`.
- **Java zrcadlo schématu:** `o/P6F4jn8H6L.java` = 72 protobuf-java zpráv (jen obfuskované názvy). Field čísla jsou čitelná z `readTag()` větví (tagy: field 1–10, wire typy 0/2/5). → kompletní `.proto` je odtud mechanicky rekonstruovatelný; sémantické názvy doplnit z `dat::` symbolů a z plaintext záznamu.

## 4. Příkazy (JNI `com.dat.datlib.ConnectionImpl`, nativní `dat::Connection`)

| Příkaz | Význam |
|---|---|
| `connect` / `disconnect` | navázání/ukončení (SPP+TLS) |
| `authenticateByPIN(id,pin)` / `authenticateByToken(tok)` | SRP autentizace |
| `getToken` / `resetToken` | správa tokenu |
| `getDataObject(url, …)` / `subscribeObject(url, rate, …)` / `unsubscribeObject(url)` | čtení/odběr **datových objektů** (živé hodnoty) |
| `sendPDU(id, protocol, …, params[], requests[], …)` | **surová diagnostika** (OBD/UDS/KWP) na CAN |
| `getSupportedDiagParameters` / `getAvailableDiagParameters` | katalog diag parametrů (z dongle za běhu) |
| `getDiagStatus` / `getDiagConfigReport` / `getActivationStatus` | stav/konfigurace/aktivace |
| `getFirmwareStatus` / `getDongleHWRevision` / `getDongleSerialNumber` / `getVersion` / `getCymotiveVersion` | identifikace dongle |
| `startUpdate`/`sendUpdateData`/`flashChunk`/`completeUpdate`/`cancelUpdate` | flash konfigurace (podepsaný blob, viz §6) |
| `readMemoryAsync`, `switchToServiceMode` | servisní |

## 5. PDU vrstva (čtení vozidla) — `com.dat.datlib`
- **`PDUProtocol`**: `ISOUDSonCAN(0)`, `KWPonCAN(1)`, `ISOOBDonCAN(2)`, `VW2000LPonTP20(3)`, `NO_PROTOCOLTYPE(5)`.
- **`TPDURequest`**: `byte[] DataField` (syrový UDS/OBD požadavek), `CP_P2Max` (timeout), `CP_RepeatReqCountApp`, `ContinueIfFail`, `index`.
- **`TPDUCommunicationParameter`**: `{int ID, int Value}` (parametry CAN komunikace).
- **`PDUResponse`**: `{respID, ErrorCode, FragmentID, rows[]}`; řádek = `{PDUIndex, Timestamp, ErrorCode, CANID, byte[] DataField}`. (typ fragmentu 23 = PDUFragment.)
- **`PDURequestResult`**: `OK, CanIdError, CanSpeedError, CanProtocolError, VehicleSpeedInvalid`.
- Závěr: dongle je **CAN/OBD brána** — pošleš UDS/OBD PDU, on je vystaví na sběrnici a vrátí odpovědi s CANID + daty. Přesně tohle OpenPlug reimplementuje.

## 6. Datový model objektů — `com.dat.datlib.DataElement`
- `DataObject { int url; DataElement[] }`, `DataElement { int url, int eType, Object value, int precision, int state, String msg }`.
- `eType`: ABSOLUTE0, ACTIVITY1, ALTERNATIVE2, BINARY3, ENUMERATION4, LISTENTITY5, OBJECTENTITY6, RELATIVE7, TEXT8, TIME9.
- `state`: OK0, ERROR2, NODATA4.
- `url` = číselné ID objektu (RPM, napětí baterie, nájezd…). **Mapování url→signál není statická konstanta** — chodí z dongle (`getSupportedDiagParameters`) nebo z podepsaného konfiguračního blobu.

## 7. Stavy a chyby
- **`DiagnosticStatus`**: 1 DeviceIsBusy, 2 ConfigurationVerifyInProgress, 3 ConfiguredAndNotConnected, 4 NotConfiguredAndEOBDcommunication, 5 ConfiguredAndConnected, 6 NotConfiguredAndNotConnected, 7 ErrorCorruptedDevice, 8 ErrorMemoryProblem, 9 Unknown, 10 ConnectionReset, 11 ProtobufDecodeError.
- `DongleStatus_e`, `AuthenticationResult` (enumy v nativním kódu).

## 8. Konfigurační blob (nepotřebný pro čtení)
- `assets/mappingtable/app/210323_SOP-Model_V42_Release/configuration_update_blob.dat` (116 kB) + `.sig` (132 B, pravděpodobně ECDSA). Protobuf s úvodním zašifrovaným/hash polem. Flashuje se do dongle (`ConfigurationUpdateTransaction`), **podepsaný → nefalšovat**. Pro čtení vozidla ho OpenPlug nepotřebuje (stačí PDU + data objekty).

## Co zbývá (potřebuje 1 živé plaintext zachycení)
1. Doplnit sémantické názvy a přesné field layouty 72 protobuf zpráv (Java schéma + plaintext).
2. Katalog `url` datových objektů ↔ konkrétní signály (z `getSupportedDiagParameters` za běhu).
3. Ověřit sekvenci handshake→auth→subscribe na reálných bajtech.
→ Plán: `tools/frida_dump_plaintext.js` (hook `mbedtls_ssl_read/write`) na patchované aplikaci při zapnutém zapalování.

## Stav proveditelnosti: ZELENÁ
Žádný server. Vše je telefon↔dongle, standardní TLS-SRP + protobuf + OBD/UDS. Náhrada je realizovatelná čistě z klientské strany, s PINem ze štítku jako jediným tajemstvím, které už máme.
