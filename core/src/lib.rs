//! # openplug-core
//!
//! Shared core of OpenPlug — an open replacement for the Škoda Connect LITE app
//! for the "Škoda DataPlug by TEXA" Bluetooth OBD dongle. Platform-independent;
//! Android/iOS bindings via UniFFI (to be added later).
//!
//! Layers (see research/notes/protocol_spec.md):
//!   Transport (SPP/BLE/test)  →  TLS-SRP tunnel (TLS 1.2)  →  protobuf
//!   `_TDongleReq/Rsp`  →  transactions  →  OBD/UDS PDU + data objects.
//!
//! Done: [`transport`] trait + test pipe, [`srp`] (SRP-6a RFC 5054),
//! [`groups`] (dongle group = RFC5054-1536, g=2, identity "PA"),
//! [`tls_prf`], [`tls_record`] (Encrypt-then-MAC), [`tls_handshake`],
//! [`tls_client`] (full TLS-SRP handshake), [`protobuf`] (generic codec),
//! [`dtp`] (link framing) and [`dongle`] (Subscribe request + DataElement decode,
//! from the decoded live protocol in research/notes/live_protocol.md) and [`link`]
//! (the read API: connect → authenticate → subscribe → stream of values).
//! Remaining: transactions beyond Subscribe, the raw OBD/UDS PDU path, and the
//! UniFFI facade.

pub mod dongle;
pub mod dtp;
pub mod groups;
pub mod link;
pub mod protobuf;
pub mod srp;
pub mod tls_record;
pub mod tls_prf;
pub mod tls_handshake;
pub mod tls_client;
pub mod transport;

/// SRP identity for PIN authentication (captured from the SRP extension username).
pub const SRP_IDENTITY_PIN: &str = "PA";

/// The DataPlug ciphersuite (TLS 1.2). SRP hash = SHA-1, bulk = AES-128-CBC.
pub const CIPHERSUITE: u16 = 0xC01D; // TLS_SRP_SHA_WITH_AES_128_CBC_SHA

/// TLS content types (RFC 5246).
pub mod content_type {
    pub const CHANGE_CIPHER_SPEC: u8 = 20;
    pub const ALERT: u8 = 21;
    pub const HANDSHAKE: u8 = 22;
    pub const APPLICATION_DATA: u8 = 23;
}
