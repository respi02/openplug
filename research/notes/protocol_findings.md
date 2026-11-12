# Protokol DataPlug ↔ aplikace: rozhodovací brána vyřešena (2026-11-12)

Zdroj: vlastní Bluetooth HCI snoop log z vlastního telefonu + vlastního dongle
(`captures/dataplug-pairing-2026-11-12.btsnoop`, 920 rámců) + čtení nativních knihoven z APK
(jen statická analýza řetězců a symbolů, nic se nespouštělo). § 66 odst. 1 písm. d) AZ.

## Závěr: provoz je šifrovaný TLS-SRP, ale klíčem je PIN ze štítku — náhrada JE proveditelná bez serveru

Dongle `DP_2_1_TOHMY009093` (BD_ADDR `b4:b8:59:3b:5e:87`). Telefon `50:50:a4:51:08:1b`.

### Vrstvy (zvenčí dovnitř)
1. **Bluetooth Classic RFCOMM / SPP** (sériový profil). Žádné BLE GATT. V snoop logu: 357 SPP rámců, ~95 KB.
2. **TLS 1.2**, cipher **`TLS_SRP_SHA_WITH_AES_128_CBC_SHA` (0xC01D)** — jediná nabízená šifra + SCSV. Implementace **mbedTLS 2.4.0** (`libmbedtls.so`).
   - Handshake: ClientHello → ServerHello → ServerKeyExchange → ServerHelloDone → ClientKeyExchange → CCS → Application Data (`17 03 03 …`, šifrováno).
   - **Žádné certifikáty** (chybí Certificate i CertificateRequest). Autentizace je čistě SRP (password-authenticated, RFC 5054).
   - ClientHello nese SRP rozšíření (typ 12) s **identitou `PA`** (= PIN Auth).
   - client_random zachyceného spojení: `6af6293db8d3bc559320e15921067c44ca9d6f50bc169e61bb2f03212fd8c539`.
3. **nanopb protobuf** uvnitř tunelu: `_TDongleReq` / `_TDongleRsp` (+ `_TDongleRspMetaHeader`, `_TDongleRspBlockDescriptor`), pod tím PDU/OBD vrstva (`_TPDURequest`, `sendPDU(PDUPin_e, PDUProtocol_e, …)`).

### Autentizace (symboly v `libdongles.so`, namespace `dat`)
- `dat::SecureTunnel::startSRPHandshake(std::string identity, std::string password, AuthenticationMethod)`
- `dat::Connection::authenticateByPIN(const std::string&)` a `authenticateByToken(const std::string&)`
- `getTokenAsync` / `resetTokenAsync` / `DataListener::onToken(int, const std::string&)`
- `dat::Configuration`: `SRP_PIN_AUTHENTICATION_IDENTITY` (= `"PA"`), `SRP_TOKEN_AUTHENTICATION_IDENTITY`, + PSK varianty (`PSK_PIN_…`, `PSK_TOKEN_…`).

**Model:** první spárování = SRP s identitou `PA`, heslo = **PIN ze štítku** (proto se zadává do 2 min od zapnutí zapalování). Po úspěchu dongle vydá **token**, kterým se další spojení autentizují bez PINu (`authenticateByToken`, identita `SRP_TOKEN_…`). Existuje i alternativní PSK režim (`mbedtls_ssl_conf_psk`), ale tenhle dongle použil SRP.

## Co to znamená pro OpenPlug
- **Žádný server.** Celý handshake i data jsou lokální telefon↔dongle. Killswitch i login byly jen v aplikaci (viz [[killswitch_analysis]]). Vypnuté backendy Škody na provoz s donglem nemají vliv.
- **Náhrada je proveditelná.** Stačí:
  1. TLS-SRP klient (identita `PA`, heslo = PIN ze štítku, cipher 0xC01D / AES-128-CBC). Podporuje mbedTLS, OpenSSL (SRP build), BouncyCastle.
  2. Reimplementovat protobuf zprávy `_TDongleReq`/`_TDongleRsp` a PDU/OBD vrstvu. Schéma lze rekonstruovat z nanopb deskriptorů v `libdongles.so` (`pb_field_s` tabulky) a z dešifrovaného provozu (viz níže).
  3. Volitelně uložit vydaný token, aby se PIN nemusel zadávat při každém spojení.
- **Jak přečíst plaintext pro reverzování schématu** (vlastní zařízení): PIN SRP heslo známe (štítek), takže jde dešifrovat buď (a) vlastní SRP klient, který spojení naváže sám, nebo (b) Frida hook na `mbedtls_ssl_write`/`mbedtls_ssl_read` v `libdongles.so`/`libmbedtls.so` → odchytit protobuf před zašifrováním. Patchovaná aplikace už běží s rootem.

## Citlivé (nezveřejňovat, viz pravidla v README)
- `libkeys.so` drží API klíče (Phrase, Adjust), heslo k SQLCipher DB (`getDBPass`) a „anonymous client“. Patří k analytice Škody, s donglem nesouvisí. Jen přečteno lokálně, nikam neodesláno.
- PIN dongle je na fotce štítku (`photos/`), rovněž v `.gitignore`.

## SRP parametry (z ServerKeyExchange, frame 312)
- Skupina: **standardní RFC 5054, 1536-bit** (N začíná `9DEF3CAF…` = RFC 5054 Appendix A), generátor **g = 2**.
- Salt: 32 B, per-dongle, posílá dongle v handshake (klient ho nepotřebuje znát předem).
- Klient potřebuje jen: skupinu RFC5054-1536, identitu `PA`, heslo = PIN. Verifier `v=g^x` drží dongle; `x=H(salt,"PA",PIN)`.
- mbedTLS klient: `mbedtls_ssl_conf_srp` + standardní 1536-bit N/g; žádné custom parametry.

## Vyjednaná rozšíření (z reálného ServerHello, frame 311) — DŮLEŽITÉ pro klienta
- cipher `c01d`, komprese 0, session_id 32 B.
- **encrypt_then_mac** (RFC 7366) — MAC se počítá nad ciphertextem (Encrypt-then-MAC), ne MAC-then-encrypt. Record = `IV || AES-CBC(plaintext||padding) || HMAC-SHA1`.
- **extended_master_secret** (RFC 7627) — `master = PRF(premaster, "extended master secret", SHA256(ClientHello..ClientKeyExchange))`, NE client_random+server_random.
- renegotiation_info, ec_point_formats.
- Klient musí obě rozšíření nabídnout i respektovat, jinak handshake/record selže.
