# DataPlug ↔ app protocol: decision gate resolved (2026-11-12)

> **Status (2026-11-13):** the conclusion below was borne out — OpenPlug now
> authenticates with the label PIN and reads live values from a real dongle. Two
> refinements: the PIN is alphanumeric (ours is 8 capital letters), and it was accepted
> on repeated connects many minutes into a running session — we did not observe the
> "within 2 min of switching on the ignition" limit mentioned below.

Source: our own Bluetooth HCI snoop log from our own phone + our own dongle
(920 frames) + reading the native libraries from the APK (static string/symbol
analysis only, nothing executed). CZ § 66(1)(d) of Act No. 121/2000 Sb.

## Conclusion: the traffic is TLS-SRP encrypted, but the key is the PIN on the label — a replacement IS feasible without a server

Dongle `DP_2_1_<serial>` (BD_ADDR `b4:b8:59:…`). Phone `50:50:a4:…`.

### Layers (outside in)
1. **Bluetooth Classic RFCOMM / SPP** (serial profile). No BLE GATT in this
   capture. 357 SPP frames, ~95 KB.
2. **TLS 1.2**, cipher **`TLS_SRP_SHA_WITH_AES_128_CBC_SHA` (0xC01D)** — the only
   offered suite + SCSV. Implementation **mbedTLS 2.4.0** (`libmbedtls.so`).
   - Handshake: ClientHello → ServerHello → ServerKeyExchange → ServerHelloDone →
     ClientKeyExchange → CCS → Application Data (`17 03 03 …`, encrypted).
   - **No certificates** (no Certificate, no CertificateRequest). Authentication is
     purely SRP (password-authenticated, RFC 5054).
   - The ClientHello carries the SRP extension (type 12) with **identity `PA`**
     (= PIN Auth).
3. **nanopb protobuf** inside the tunnel: `_TDongleReq` / `_TDongleRsp`, below that
   the PDU/OBD layer (`_TPDURequest`, `sendPDU(PDUPin_e, PDUProtocol_e, …)`).

### Authentication (symbols in `libdongles.so`, namespace `dat`)
- `dat::SecureTunnel::startSRPHandshake(std::string identity, std::string password, AuthenticationMethod)`
- `dat::Connection::authenticateByPIN(const std::string&)` and `authenticateByToken(const std::string&)`
- `getTokenAsync` / `resetTokenAsync` / `DataListener::onToken(int, const std::string&)`
- `dat::Configuration`: `SRP_PIN_AUTHENTICATION_IDENTITY` (= `"PA"`),
  `SRP_TOKEN_AUTHENTICATION_IDENTITY`, + PSK variants (`PSK_PIN_…`, `PSK_TOKEN_…`).

**Model:** first pairing = SRP with identity `PA`, password = **the PIN on the
label** (hence it is entered within 2 min of switching on the ignition). On
success the dongle issues a **token** authenticating later connections without the
PIN (`authenticateByToken`, identity `SRP_TOKEN_…`). A PSK mode also exists
(`mbedtls_ssl_conf_psk`), but this dongle used SRP.

## What this means for OpenPlug
- **No server.** The whole handshake and data are local, phone↔dongle. The
  killswitch and login wall were app-side only. Škoda's shut-down backends have no
  bearing on dongle traffic.
- **A replacement is feasible.** We need: (1) a TLS-SRP client (identity `PA`,
  password = PIN, cipher 0xC01D / AES-128-CBC); (2) a reimplementation of the
  `_TDongleReq/Rsp` protobuf messages and the PDU/OBD layer.

## SRP parameters (from the ServerKeyExchange, frame 312)
- Group: **standard RFC 5054, 1536-bit** (N starts with `9DEF3CAF…` = RFC 5054
  Appendix A), generator **g = 2**.
- Salt: 32 B, per-dongle, sent by the dongle in the handshake (the client need not
  know it in advance).
- The client only needs: the RFC5054-1536 group, identity `PA`, password = PIN.
  The verifier `v=g^x` is held by the dongle; `x=H(salt,"PA",PIN)`.

## Negotiated extensions (from the real ServerHello, frame 311) — IMPORTANT for the client
- cipher `c01d`, compression 0, session_id 32 B.
- **encrypt_then_mac** (RFC 7366) — the MAC is computed over the ciphertext
  (Encrypt-then-MAC), not MAC-then-encrypt. Record = `IV || AES-CBC(plaintext ||
  padding) || HMAC-SHA1`.
- **extended_master_secret** (RFC 7627) — `master = PRF(premaster,
  "extended master secret", SHA256(ClientHello..ClientKeyExchange))`, not
  client_random+server_random.
- renegotiation_info, ec_point_formats.
- The client must both offer and honor these two extensions, otherwise the
  handshake/record fails.

## Sensitive (do not publish — see code_licensing.md)
- `libkeys.so` holds API keys (Phrase, Adjust), the SQLCipher DB password
  (`getDBPass`) and an "anonymous client". These belong to Škoda's analytics and
  are unrelated to the dongle. Read locally only, never transmitted.
- The dongle PIN is on the label photo, also kept out of the repo.
