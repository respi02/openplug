# OpenPlug

> Open-source replacement for the **ŠKODA Connect LITE** app, talking to the
> Bluetooth OBD dongle *Škoda DataPlug by TEXA*. Clean-room, interoperability-only.
>
> Otevřená náhrada aplikace **ŠKODA Connect LITE** pro Bluetooth OBD dongle
> *Škoda DataPlug by TEXA*. Čistá reimplementace, jen pro interoperabilitu.

---

## 🇬🇧 English

ŠKODA discontinued the Connect LITE app on 1 July 2025. The **DataPlug hardware
still works** — only the app that drove it is gone. OpenPlug is a clean,
cross-platform (Android + iOS) reimplementation of the phone↔dongle protocol so
the dongle you own stays usable.

**This is not affiliated with or endorsed by Škoda, Volkswagen or TEXA.** It
reimplements an *interface* for interoperability with hardware the user owns; it
contains no vendor code, firmware, keys or trademarks.

### How the dongle works (reverse-engineered from our own device + traffic)

```
Bluetooth  →  TLS-SRP tunnel        →  protobuf          →  OBD / UDS
(Android:      (TLS 1.2, SRP-6a,        (_TDongleReq/Rsp)     over CAN
 Classic SPP;   cipher 0xC01D,                                (the dongle is a
 iOS: BLE)      identity "PA",                                 CAN/OBD gateway)
                password = PIN on label)
```

No server is involved — everything is local between phone and dongle. The
authentication secret is the **PIN printed on the dongle's label**, which the
owner has.

### Architecture

Shared **Rust core** (`core/`) → platform transport shim → thin native UI
(Kotlin/Compose on Android, SwiftUI on iOS), bound via UniFFI.

| Layer | Status |
|---|---|
| `transport` — byte-stream trait + test pipe | ✅ |
| `srp` — SRP-6a (RFC 5054) | ✅ verified vs RFC vector |
| `tls_prf` — TLS 1.2 PRF-SHA256 | ✅ verified vs known-answer |
| `tls_record` — AES-128-CBC + HMAC-SHA1, Encrypt-then-MAC (RFC 7366) | ✅ |
| `tls_handshake` — master secret, EMS (RFC 7627), ServerKeyExchange parse | ✅ parses real dongle bytes |
| `tls_client` — TLS-SRP handshake orchestration | 🚧 in progress |
| protobuf codec + OBD/data-object layer | ⏳ needs one live capture |

### Build & test (offline, no dongle needed)

```sh
cd core && cargo test
```

All cryptographic layers are verified offline, with no C dependency, anchored to
real captured handshake bytes from the author's own dongle.

### Legal

OpenPlug is a clean reimplementation of an open protocol stack (SRP-6a RFC 5054,
TLS 1.2 RFC 5246, protobuf, ISO OBD/UDS) for **interoperability** with hardware
the user owns — lawful under EU Directive 2009/24/EC Art. 6 and Czech Act
No. 121/2000 Sb. § 66. See `research/notes/code_licensing.md`. Code is **MIT**
licensed; dependencies are MIT/Apache-2.0.

---

## 🇨🇿 Česky

Škoda vypnula aplikaci Connect LITE 30. 6. / 1. 7. 2025. **Hardware DataPlugu
funguje dál** — zmizela jen aplikace, která ho obsluhovala. OpenPlug je čistá,
multiplatformní (Android + iOS) reimplementace protokolu telefon↔dongle, aby
dongle, který vlastníš, zůstal použitelný.

**Není nijak spojeno se Škodou, Volkswagenem ani TEXA a není jimi schváleno.**
Reimplementuje *rozhraní* kvůli interoperabilitě s hardwarem, který uživatel
vlastní; neobsahuje žádný kód výrobce, firmware, klíče ani ochranné známky.

### Jak dongle funguje (zpětně odvozeno z vlastního zařízení a provozu)

```
Bluetooth  →  tunel TLS-SRP        →  protobuf          →  OBD / UDS
(Android:      (TLS 1.2, SRP-6a,       (_TDongleReq/Rsp)     přes CAN
 Classic SPP;   cipher 0xC01D,                                (dongle je
 iOS: BLE)      identita „PA",                                 CAN/OBD brána)
                heslo = PIN ze štítku)
```

Žádný server v tom není — vše běží lokálně mezi telefonem a donglem.
Autentizační tajemství je **PIN vytištěný na štítku dongle**, který má majitel.

### Architektura

Sdílené **Rust jádro** (`core/`) → platformní transportní vrstva → tenké nativní
UI (Kotlin/Compose na Androidu, SwiftUI na iOS), propojené přes UniFFI.

| Vrstva | Stav |
|---|---|
| `transport` — bajtový trait + test pipe | ✅ |
| `srp` — SRP-6a (RFC 5054) | ✅ ověřeno proti RFC vektoru |
| `tls_prf` — TLS 1.2 PRF-SHA256 | ✅ ověřeno proti známému vektoru |
| `tls_record` — AES-128-CBC + HMAC-SHA1, Encrypt-then-MAC (RFC 7366) | ✅ |
| `tls_handshake` — master secret, EMS (RFC 7627), parser ServerKeyExchange | ✅ parsuje reálné bajty dongle |
| `tls_client` — orchestrace TLS-SRP handshake | 🚧 rozpracováno |
| protobuf kodek + OBD/datové objekty | ⏳ čeká na 1 živé zachycení |

### Build a test (offline, bez dongle)

```sh
cd core && cargo test
```

Všechny kryptografické vrstvy jsou ověřené offline, bez C závislosti, ukotvené
na reálné zachycené bajty handshake z autorova vlastního dongle.

### Právní rámec

OpenPlug je čistá reimplementace otevřeného zásobníku protokolů (SRP-6a RFC 5054,
TLS 1.2 RFC 5246, protobuf, ISO OBD/UDS) kvůli **interoperabilitě** s hardwarem,
který uživatel vlastní — zákonné dle směrnice EU 2009/24/ES čl. 6 a § 66 zákona
č. 121/2000 Sb. Viz `research/notes/code_licensing.md`. Kód je pod licencí
**MIT**; závislosti jsou MIT/Apache-2.0.
