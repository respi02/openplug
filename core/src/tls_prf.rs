//! TLS 1.2 PRF (RFC 5246 §5) s SHA-256 — základ pro master secret,
//! key expansion a Finished verify_data.

use hmac::{Hmac, Mac};
use sha2::Sha256;
type HmacSha256 = Hmac<Sha256>;

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = <HmacSha256 as Mac>::new_from_slice(key).unwrap();
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

/// P_hash(secret, seed) rozvinuté na `out_len` bajtů.
fn p_hash(secret: &[u8], seed: &[u8], out_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(out_len);
    let mut a = hmac(secret, seed); // A(1) = HMAC(secret, seed)
    while out.len() < out_len {
        let mut input = a.clone();
        input.extend_from_slice(seed);
        out.extend_from_slice(&hmac(secret, &input));
        a = hmac(secret, &a); // A(i+1) = HMAC(secret, A(i))
    }
    out.truncate(out_len);
    out
}

/// PRF(secret, label, seed) = P_SHA256(secret, label || seed).
pub fn prf(secret: &[u8], label: &[u8], seed: &[u8], out_len: usize) -> Vec<u8> {
    let mut ls = Vec::with_capacity(label.len() + seed.len());
    ls.extend_from_slice(label);
    ls.extend_from_slice(seed);
    p_hash(secret, &ls, out_len)
}
