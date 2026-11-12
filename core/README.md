# openplug-core

Shared, platform-independent core of OpenPlug (transport, TLS-SRP, DataPlug
protocol). Bindings for Android/iOS via UniFFI will be added later.

## Status
- ✅ `transport` — byte-stream trait + in-memory test pipe
- ✅ `srp` — SRP-6a (RFC 5054), verified vs Appendix B + the dongle's 1536-bit group
- ✅ `tls_prf` — TLS 1.2 PRF-SHA256 (known-answer vector)
- ✅ `tls_record` — AES-128-CBC + HMAC-SHA1, Encrypt-then-MAC (RFC 7366)
- ✅ `tls_handshake` — master secret, EMS, ServerKeyExchange parser (real dongle bytes), ClientHello/ClientKeyExchange, Finished
- ✅ `tls_client` — TLS-SRP handshake orchestration (client + server, full loopback)
- ✅ `protobuf` — generic wire codec (decode/encode/inspect)
- ⏳ `.proto` schema (`_TDongleReq/Rsp`), OBD/UDS PDU, data objects — needs one live plaintext capture

## Test (offline, no dongle)
```sh
cd core && cargo test
```
