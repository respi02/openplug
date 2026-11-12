//! # openplug-core
//!
//! Sdílené jádro OpenPlugu — otevřené náhrady aplikace Škoda Connect LITE pro
//! Bluetooth OBD dongle „Škoda DataPlug by TEXA". Nezávislé na platformě;
//! bindingy pro Android/iOS přes UniFFI (připraví se později).
//!
//! Vrstvy (viz research/notes/protocol_spec.md):
//!   Transport (SPP/BLE/test)  →  TLS-SRP tunel (mbedtls)  →  protobuf
//!   `_TDongleReq/Rsp`  →  transakce  →  OBD/UDS PDU + datové objekty.
//!
//! Hotovo: [`transport`] trait + test pipe, [`srp`] (SRP-6a RFC 5054),
//! [`groups`] (skupina DataPlugu = RFC5054-1536, g=2, identita „PA").
//! Zbývá (čeká na 1 plaintext záznam): TLS record vrstva, protobuf kodek,
//! transakční automat, PDU builder.

pub mod groups;
pub mod srp;
pub mod tls_record;
pub mod tls_prf;
pub mod tls_handshake;
pub mod tls_client;
pub mod transport;

/// SRP identita pro PIN autentizaci (zachyceno: SRP extension username).
pub const SRP_IDENTITY_PIN: &str = "PA";

/// Ciphersuite DataPlugu (TLS 1.2). SRP hash = SHA-1, data = AES-128-CBC.
pub const CIPHERSUITE: u16 = 0xC01D; // TLS_SRP_SHA_WITH_AES_128_CBC_SHA

/// TLS content types (RFC 5246).
pub mod content_type {
    pub const CHANGE_CIPHER_SPEC: u8 = 20;
    pub const ALERT: u8 = 21;
    pub const HANDSHAKE: u8 = 22;
    pub const APPLICATION_DATA: u8 = 23;
}
