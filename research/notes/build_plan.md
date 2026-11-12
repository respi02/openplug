# OpenPlug — build plán a stav příprav (2026-11-12)

Konsoliduje [[protocol_spec]], [[protocol_findings]], [[tech_stack]], [[killswitch_analysis]]. Co je jisté, co zbývá, a v jakém pořadí.

## Jisté fakty (offline potvrzené)
- **Transport:** Android = BT Classic RFCOMM/SPP; iOS = BLE (Core Bluetooth). Dongle je dual-mode (FCC T8RDP15/T8RDP22: DTS+DSS granty), **není MFi**. Oba používají tentýž tunel, jen jiné „potrubí“.
- **Bezpečný tunel:** TLS 1.2, cipher `TLS_SRP_SHA_WITH_AES_128_CBC_SHA` (0xC01D), mbedTLS 2.4.0, bez certifikátů.
- **SRP:** skupina RFC 5054 **1536-bit**, g=2; identita **`PA`** (PIN auth) / token po prvním spárování; heslo = **PIN ze štítku**; salt chodí od dongle v handshake.
- **Aplikační protokol:** nanopb protobuf, `_TDongleReq`/`_TDongleRsp` + fragmentace; pod tím OBD/UDS PDU vrstva. Kompletní API v `com.dat.datlib` (neobfuskované) + `dat::` v libdongles.so.
- **Killswitch i login wall** jsou klientské, obejité (viz [[killswitch_analysis]]).

## Architektura (rozhodnuto)
Sdílené **Rust jádro** (UniFFI → Kotlin+Swift) · platformní **transport shim** · tenké nativní **UI**.
```
core (Rust):  Transport trait │ TlsSrpSession(mbedtls) │ DongleCodec(prost) │ TransactionMgr │ PduBuilder(UDS/OBD) │ DataObjects
                     ▲                                   api přes UniFFI ▼
android: BluetoothSocket(SPP) ──┐                 Kotlin+Compose UI
ios:     CoreBluetooth(GATT)  ──┘                 SwiftUI UI
```

## Mapování dat::/datlib API → moduly jádra
| Modul jádra | Pokrývá API |
|---|---|
| `Transport` trait (`send/recv bytes`) | `IIOStream`, `DTPStream`, `InputBufferedStream` |
| `TlsSrpSession` | `SecureTunnel`, mbedtls SRP |
| `DongleCodec` | `_TDongleReq/Rsp`, `_TSubFragment`, `DongleSocket` reassembly, `DataAggregator`/`PduAggregator` |
| `TransactionMgr` | `TransactionManager`, `*Transaction` (Pin/Token/Pdu/DataSubscription/Simple/ConfigUpdate) |
| `Auth` | `authenticateByPIN/Token`, `getToken/resetToken` |
| `PduBuilder` | `sendPDU`, `PDUProtocol`, `TPDURequest`, `PDUResponse` |
| `DataObjects` | `subscribeObject/getDataObject`, `DataElement` (eType/state) |
| `Info` | `getVersion/HWRevision/SerialNumber/DiagStatus/ActivationStatus` |

## Stav protobuf schématu
- V `o/P6F4jn8H6L.java` je **72 protobuf-java zpráv** (standardní GeneratedMessage, obfuskované názvy). Field čísla + typy jsou v `writeTo()`/`readTag()` — jisté a čitelné.
- **Sémantické názvy v obfuskované Javě nejsou.** Čistě statická extrakce jde, ale potřebuje pořádný Java parser (obfuskované Builder/message názvy + jadx artefakty porazí rychlý regex) — nevyplatí se.
- **Zvolená cesta:** jedno **plaintext zachycení** (Frida, `tools/frida_dump_plaintext.js`) → `protoc --decode_raw` dá field čísla/typy/vnoření jednoznačně → názvy přiřadit podle operace, která zprávu vyrobila (getVersion, subscribe, sendPDU…) a podle `_T*` názvů v libdongles.so. Pak `.proto` → `prost`.

## Pořadí prací
### Hotovo offline (2026-11-12)
- Protokol zmapován, TLS-SRP parametry zjištěny, transport pro obě platformy rozhodnut, API→moduly namapováno, Frida dumper připraven, killswitch+login obejity.

### Zbývá offline (bez dongle)
1. ✅ **Rust kostra** (`core/`): Cargo workspace, `Transport` trait + test pipe, **SRP-6a implementace ověřená offline** (RFC 5054 Appendix B vektor + shoda premaster klient↔server na 1536-bit skupině dongle, identita `PA`). `cd core && cargo test`.
2. ✅ **TLS record vrstva** (`core/src/tls_record.rs`): AES-128-CBC + HMAC-SHA1, MAC-then-encrypt, explicitní IV, TLS padding — vlastní implementace (cesta B, bez mbedtls/C). Ověřeno offline: round-trip (délky 0–255 B), detekce manipulace, seq v MAC. `cargo test` → 5 passed.
3. ✅ **TLS 1.2 handshake — krypto a klíčový rozvrh** (`tls_prf.rs`, `tls_handshake.rs`): PRF-SHA256 (ověřen standardním vektorem), master secret + key expansion → klíče pro record vrstvu, parser ServerKeyExchange **ověřen proti REÁLNÝM bajtům dongle** (frame 312: N=RFC5054-1536, g=2, salt, B), ClientKeyExchange serializace, Finished verify_data. **Plný loopback** SRP→premaster→master→klíče→record (oba směry) prošel. `cargo test` → 8 passed.
4. ✅ **Orchestrace handshake** (`tls_client.rs`): plný TLS-SRP dialog (ClientHello→ServerHello/SKE/SHD→ClientKeyExchange→CCS/Finished) s EtM + EMS, klient i server, **plný loopback test** (navázání + výměna app dat oběma směry). `cargo test` → 9 passed.
4. **protobuf kodek** — generický decode (field/wire) jde offline; `.proto` schéma a názvy čekají na plaintext záznam.
5. Desktop test harness (BT Classic z Macu přes USB adaptér) — kód připravit, spustit s donglem.
6. UniFFI rozhraní + stuby horních vrstev.

### Potřebuje dongle (až bude čas/klid)
4. **BLE sken** zapnutého dongle (uživatel) → potvrdit LE a UUID služby/charakteristik pro iOS transport.
5. **1 plaintext zachycení** (Frida na patchované appce, zapalování zap.) → `.proto` + katalog `url` datových objektů.
6. Ověřit sekvenci handshake→auth→subscribe/PDU na reálných bajtech; dotáhnout `DongleCodec`.
7. Android shim + UI (end-to-end), pak iOS (BLE) + SwiftUI.

## Otevřené otázky
- Běží přes BLE GATT tentýž TLS-SRP+protobuf tunel? (velmi pravděpodobně; potvrdit krokem 5 na iOS strane nebo odvodit z BLE MTU/charakteristik).
- Token lifecycle: jak dlouho platí, kde ho dongle váže (per-telefon?).
- `url` → konkrétní vozidlový signál: z `getSupportedDiagParameters` za běhu.
