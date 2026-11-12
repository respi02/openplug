# OpenPlug — code licensing and legal usability (2026-11-12)

## 1. Core dependencies (`core/`) — all permissive, OK
| Crate | License |
|---|---|
| num-bigint, num-traits, num-integer | MIT OR Apache-2.0 |
| sha1, sha2, digest, block-buffer, crypto-common, cpufeatures, cfg-if | MIT OR Apache-2.0 |
| aes, hmac, cipher, inout | MIT OR Apache-2.0 |
| thiserror, hex | MIT OR Apache-2.0 |
| generic-array | MIT |

No copyleft (GPL/LGPL/MPL). Usable in open or closed projects, commercially, on
iOS and Android. The only obligation is to keep the license texts and copyright
notices (a standard "third-party licenses" screen in the app). OpenPlug itself is
licensed **MIT** (see `core/Cargo.toml`).

## 2. Standards we implement — freely implementable
- **SRP-6a / TLS-SRP**: RFC 2945 / RFC 5054 (IETF, free).
- **TLS 1.2**: RFC 5246 (free). AES, HMAC, SHA — public standards.
- **protobuf**: Google's format; our own implementation (or `prost`, Apache-2.0).
- **OBD-II / UDS / KWP / CAN**: ISO 15765, ISO 14229, ISO 14230 — open standards.
  The ISO documents themselves are paid, but implementing the protocols is allowed.

None of this is TEXA/Cymotive intellectual property — they are open standards.

## 3. Reimplementing the DataPlug protocol — lawful interoperability
- **We do not take vendor code.** OpenPlug is a clean reimplementation of the
  *protocol* (on-the-wire behavior), not a copy of `libdongles.so`/`libmbedtls.so`.
  Protocols, interfaces and "methods of operation" are not protected by copyright.
- **EU framework:** Directive 2009/24/EC on the legal protection of computer
  programs — Art. 1(2) excludes ideas, principles and **interfaces** from
  protection; Art. 6 explicitly permits the decompilation necessary for
  **interoperability**. CZ: § 66(1)(d) and § 66(4) of Act No. 121/2000 Sb.
- **Ownership:** the user owns the dongle and analyses their own traffic from
  their own device with their own data from their own car. This is the strongest
  position for interoperability.
- **TLS-SRP as a "technological protection measure" (TPM):** in theory someone
  could call TLS-SRP a TPM and object to circumventing it. Against that: (a) the
  purpose is purely interoperability with the user's own hardware, not accessing
  someone else's protected content; (b) the password (PIN) is legitimately held
  by the owner (on the label); (c) the EU interoperability exception. Low risk but
  not zero — keep a strictly interoperability-focused framing.

## 4. Rules (what must NOT go into the repo or distribution)
- TEXA/Cymotive binaries (`lib*.so`), the APK, firmware, the configuration blob, signatures.
- `libkeys.so` secrets (Phrase/Adjust API keys, the DB password).
- The dongle PIN and photos of the label with the PIN.
- Trademarks: the project is named OpenPlug; in descriptions at most "compatible
  with the Škoda DataPlug". No Škoda/VW/TEXA logos.

## Conclusion
The code can be **used and published legally** (permissive dependencies, open
standards, a clean reimplementation for interoperability with one's own hardware).
Conditions: keep the interoperability purpose, do not distribute vendor
binaries/keys/PIN, do not use trademarks. A short legal review before a public
release is recommended because of the TPM nuance.
