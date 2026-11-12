//! TLS 1.2 record vrstva pro DataPlug: `TLS_SRP_SHA_WITH_AES_128_CBC_SHA` (0xC01D)
//! v režimu **Encrypt-then-MAC** (RFC 7366) — dongle ho vyjednává (viz ServerHello).
//! Record = IV(16) || AES-128-CBC(plaintext || TLS-padding) || HMAC-SHA1(20).
//! MAC se počítá nad (seq || type || version || len(IV+CT) || IV || CT).

use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;
const BLOCK: usize = 16;
const MAC_LEN: usize = 20;
pub const TLS12_VERSION: [u8; 2] = [0x03, 0x03];

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RecordError {
    #[error("MAC nesouhlasí")]
    BadMac,
    #[error("špatný padding")]
    BadPadding,
    #[error("záznam příliš krátký / nezarovnaný")]
    Short,
}

/// Směrové klíče (jedna strana): mac_key 20 B (SHA-1), enc_key 16 B (AES-128).
pub struct DirKeys {
    pub mac_key: [u8; MAC_LEN],
    pub enc_key: [u8; 16],
}

fn aes_cbc_encrypt(key: &[u8; 16], iv: &[u8; BLOCK], data: &[u8]) -> Vec<u8> {
    let c = Aes128::new(key.into());
    let mut prev = *iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(BLOCK) {
        let mut b = [0u8; BLOCK];
        for i in 0..BLOCK { b[i] = chunk[i] ^ prev[i]; }
        let mut ga = aes::cipher::generic_array::GenericArray::clone_from_slice(&b);
        c.encrypt_block(&mut ga);
        prev.copy_from_slice(&ga);
        out.extend_from_slice(&ga);
    }
    out
}

fn aes_cbc_decrypt(key: &[u8; 16], iv: &[u8; BLOCK], data: &[u8]) -> Vec<u8> {
    let c = Aes128::new(key.into());
    let mut prev = *iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(BLOCK) {
        let mut ga = aes::cipher::generic_array::GenericArray::clone_from_slice(chunk);
        let mut ct = [0u8; BLOCK];
        ct.copy_from_slice(ga.as_slice());
        c.decrypt_block(&mut ga);
        for i in 0..BLOCK { out.push(ga[i] ^ prev[i]); }
        prev.copy_from_slice(&ct);
    }
    out
}

/// MAC nad ciphertextem (Encrypt-then-MAC):
/// HMAC-SHA1(mac_key, seq(8) || type(1) || version(2) || len16(iv_ct) || iv_ct)
fn mac_etm(mac_key: &[u8], seq: u64, ctype: u8, iv_ct: &[u8]) -> [u8; MAC_LEN] {
    let mut m = <HmacSha1 as Mac>::new_from_slice(mac_key).unwrap();
    m.update(&seq.to_be_bytes());
    m.update(&[ctype]);
    m.update(&TLS12_VERSION);
    m.update(&(iv_ct.len() as u16).to_be_bytes());
    m.update(iv_ct);
    let out = m.finalize().into_bytes();
    let mut r = [0u8; MAC_LEN];
    r.copy_from_slice(&out);
    r
}

/// Zašifruje jeden record (Encrypt-then-MAC). `iv` = explicitní IV (prod. náhodný).
/// Vrací `IV || AES-CBC(plaintext||padding) || HMAC`.
pub fn seal(k: &DirKeys, seq: u64, ctype: u8, plaintext: &[u8], iv: &[u8; BLOCK]) -> Vec<u8> {
    // TLS padding na násobek bloku (padding je uvnitř šifry, bez MAC).
    let mut buf = plaintext.to_vec();
    let pad_len = BLOCK - (buf.len() % BLOCK); // 1..=BLOCK
    for _ in 0..pad_len { buf.push((pad_len - 1) as u8); }
    let ct = aes_cbc_encrypt(&k.enc_key, iv, &buf);
    let mut iv_ct = Vec::with_capacity(BLOCK + ct.len());
    iv_ct.extend_from_slice(iv);
    iv_ct.extend_from_slice(&ct);
    let mac = mac_etm(&k.mac_key, seq, ctype, &iv_ct);
    let mut out = iv_ct;
    out.extend_from_slice(&mac);
    out
}

/// Ověří MAC, dešifruje a odstraní padding. `fragment` = `IV || CT || MAC`.
pub fn open(k: &DirKeys, seq: u64, ctype: u8, fragment: &[u8]) -> Result<Vec<u8>, RecordError> {
    if fragment.len() < BLOCK + MAC_LEN + BLOCK { return Err(RecordError::Short); }
    let (iv_ct, mac) = fragment.split_at(fragment.len() - MAC_LEN);
    if (iv_ct.len() - BLOCK) % BLOCK != 0 || iv_ct.len() < 2 * BLOCK { return Err(RecordError::Short); }
    // 1) ověř MAC nad IV||CT (konstantní čas)
    let exp = mac_etm(&k.mac_key, seq, ctype, iv_ct);
    let diff = mac.iter().zip(exp.iter()).fold(0u8, |a, (x, y)| a | (x ^ y));
    if diff != 0 { return Err(RecordError::BadMac); }
    // 2) dešifruj
    let mut iv = [0u8; BLOCK];
    iv.copy_from_slice(&iv_ct[..BLOCK]);
    let plain = aes_cbc_decrypt(&k.enc_key, &iv, &iv_ct[BLOCK..]);
    // 3) odstraň padding
    let pad_len = *plain.last().ok_or(RecordError::BadPadding)? as usize + 1;
    if pad_len > plain.len() { return Err(RecordError::BadPadding); }
    let (body, pad) = plain.split_at(plain.len() - pad_len);
    if pad.iter().any(|&b| b as usize != pad_len - 1) { return Err(RecordError::BadPadding); }
    Ok(body.to_vec())
}
