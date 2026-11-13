# openplug-core

Shared, platform-independent core of OpenPlug (transport, TLS-SRP, DataPlug
protocol). Bindings for Android/iOS come from the UniFFI facade in `src/ffi.rs`.

## Status
- ✅ `transport` — byte-stream trait + in-memory test pipe
- ✅ `srp` — SRP-6a (RFC 5054), verified vs Appendix B + the dongle's 1536-bit group
- ✅ `tls_prf` — TLS 1.2 PRF-SHA256 (known-answer vector)
- ✅ `tls_record` — AES-128-CBC + HMAC-SHA1, Encrypt-then-MAC (RFC 7366)
- ✅ `tls_handshake` — master secret, EMS, ServerKeyExchange parser (real dongle bytes), ClientHello/ClientKeyExchange, Finished
- ✅ `tls_client` — TLS-SRP handshake orchestration (client + server, full loopback)
- ✅ `protobuf` — generic wire codec (decode/encode/inspect)
- ✅ `dtp` — a framing found in the vendor library; not used on the wire by either transport
- ✅ `dongle` — app protocol: Subscribe request, DataElement decode, signal catalog
- ✅ `link` — read API: connect → authenticate → subscribe → value stream
- ✅ `ffi` — UniFFI facade for Kotlin/Swift (`--features ffi`; bindings via `--features bindgen --bin uniffi-bindgen`)
- ✅ verified live against a real dongle: Android app (SPP) and Swift client (BLE)
- ⏳ transactions beyond Subscribe; OBD/UDS PDU

## Examples
- `live` — desktop harness: `--serial DEVICE` (paired dongle) or `--tcp HOST:PORT`
- `fake_dongle` — stand-in dongle on TCP with synthetic values
- `clienthello` — emits the byte-exact ClientHello record (used while probing BLE)

## Test (offline, no dongle)
```sh
cd core && cargo test                 # core only
cd core && cargo test --features ffi  # + the UniFFI facade
```
