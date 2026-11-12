# OpenPlug — volba jazyka/stacku pro iOS + Android (2026-11-12)

## Nejdřív ten skutečný problém: Bluetooth transport, ne jazyk

Dongle mluví **Bluetooth Classic RFCOMM/SPP** (náš záznam: 357 SPP rámců, **0 BLE/GATT**).

- **Android**: SPP je bez problémů (`BluetoothSocket.createRfcommSocketToServiceRecord`). Žádné omezení.
- **iOS**: třetí strany **nemají přístup k libovolnému Classic SPP**. Jde to jen přes:
  1. **External Accessory framework** — vyžaduje **MFi** certifikaci dongle a protocol string, nebo
  2. **BLE (Core Bluetooth)** — pokud je dongle dual-mode a nabízí i LE.

**VYŘEŠENO (research, 2026-11-12):** DataPlug je **dual-mode Bluetooth, NENÍ MFi**. iOS jede přes **Core Bluetooth (BLE)**. Důkazy:
- FCC ID **T8RDP15** (model DP15, 2015; novější **T8RDP22**, 2023) má na jednom 2,4 GHz zařízení **dva granty**: DTS (= Bluetooth LE) + DSS (= Bluetooth BR/EDR Classic). Dual-mode rádio. (fccid.io/T8RDP15)
- Nikde žádné „Made for iPhone" / MFi (TEXA, Škoda/VW/SEAT, Apple). SEAT brožura výslovně: dongle **nepárovat přes nastavení Bluetoothu**, párování je v appce přes PIN — to je BLE vzor (ne MFi/Classic bond).
- EU DoC (TEXA DoC-DATAPLUG-08): „CAN BUS INTERFACE", brandy VW/SEAT/Škoda/Audi, „DataPlug 1000" (= náš štítek SW 1000), RED 2014/53/EU, 0 dBm.
- Náš Android záznam = Classic/SPP strana; iOS používá BLE stranu téhož dongle.
- Zbývá jen potvrdit BLE skenerem u zapnutého dongle (uživatel udělá) + že přes BLE GATT běží tentýž TLS-SRP+protobuf tunel (vysoce pravděpodobné — protokol je transport-agnostický).

Tohle rozhodnutí je nezávislé na jazyku — řeší se v tenké transportní vrstvě. **Závěr: iOS i Android proveditelné, jádro sdílené, transport = SPP (Android) / BLE GATT (iOS).**

## Doporučená architektura: sdílené Rust jádro + nativní tenké skořápky

```
┌ UI            Android: Kotlin+Compose   │  iOS: SwiftUI          (tenké, nativní)
├ Transport     Android: RFCOMM SPP        │  iOS: BLE / ExternalAccessory  (tenký shim)
└ JÁDRO (Rust, sdílené, přes UniFFI → Kotlin+Swift bindings)
     TLS-SRP  ·  protobuf  ·  transakce/reassembly  ·  OBD/UDS PDU  ·  dekódování objektů
```

**Jádro v Rustu** (jednou, identické na obou platformách):
- **TLS-SRP**: crate `mbedtls` (Rust bindings na mbedTLS — stejná knihovna jako originál, takže ciphersuite `TLS_SRP_SHA_WITH_AES_128_CBC_SHA` sedí 1:1). Pure-Rust SRP+TLS by byl risk; mbedtls je pragmatická jistota.
- **protobuf**: `prost` z rekonstruovaného `.proto` (schéma je v `o/P6F4jn8H6L.java`, 72 zpráv — viz [[protocol_spec]]).
- Stavový automat spojení, `TransactionManager`, fragmentace, builder OBD/UDS PDU, dekódování datových objektů.
- Ven jednoduché API (`connect(transport)`, `authenticate(pin)`, `subscribe(url)`, `send_pdu(...)`, event stream) přes **UniFFI** → generuje Kotlin i Swift.

**Transport** dodává bajty do jádra, per-platforma (jediné místo, kde se iOS/Android liší).
**UI** tenké nativní; případně Compose Multiplatform, ale UI je malé, nativní je nejjednodušší.

### Proč Rust jádro, ne alternativy
| Varianta | Problém |
|---|---|
| **Rust core + UniFFI** ✅ | TLS-SRP + protokol napsané jednou, memory-safe, identické. mbedtls crate dá přesně potřebný SRP suite. Platformní BT izolovaný v shimu. |
| Kotlin Multiplatform + Compose MP | UI sdílené pěkně, ale TLS-SRP na iOS (Kotlin/Native) je bolest — mbedtls přes cinterop jde, ale křehké. |
| Flutter/Dart | Dart nemá TLS-SRP → stejně FFI do mbedtls; iOS BT Classic omezení zůstává. Nic navíc. |
| Reuse `libdongles.so`/`libmbedtls.so` z APK | ELF jen pro Android, na iOS nepoužitelné; redistribuce cizí binárky (TEXA) právně horší. Reimplementace je čistší. |

## Pořadí prací
1. **Teď (offline, hotovo):** protokol zmapován ([[protocol_spec]]), Frida dumper připraven (`tools/frida_dump_plaintext.js`).
2. **1 živé plaintext zachycení** (zapalování zap.) → doplnit `.proto` a katalog `url` objektů.
3. **Rust jádro** proti zachyceným bajtům: nejdřív TLS-SRP handshake (identita `PA`, PIN), pak `authenticate` → `subscribe`/`sendPDU`. Testovat napřed na desktopu přes BT dongle.
4. **Android shim + UI** (rychlá cesta k ověření end-to-end).
5. **iOS transport** podle výsledku BLE/MFi ověření, pak SwiftUI.
