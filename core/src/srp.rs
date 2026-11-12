//! SRP-6a per RFC 5054 (TLS-SRP). Client and server sides for computing the
//! premaster secret. Hash = SHA-1 (ciphersuite TLS_SRP_SHA_WITH_AES_128_CBC_SHA).
//!
//! The DataPlug is the SRP *server* (it holds the verifier). OpenPlug is the SRP
//! *client*. The server side here exists only for the offline self-test
//! (verifying both sides derive the same key).

use num_bigint::BigUint;
use num_traits::Zero;
use sha1::{Digest, Sha1};

use crate::groups::SrpGroup;

fn h(parts: &[&[u8]]) -> Vec<u8> {
    let mut d = Sha1::new();
    for p in parts { d.update(p); }
    d.finalize().to_vec()
}

/// PAD(x): left-pad x with zeros to the byte length of N.
fn pad(x: &BigUint, n_len: usize) -> Vec<u8> {
    let b = x.to_bytes_be();
    if b.len() >= n_len { return b; }
    let mut out = vec![0u8; n_len - b.len()];
    out.extend_from_slice(&b);
    out
}

/// Parameters derived from the group.
pub struct Srp {
    pub n: BigUint,
    pub g: BigUint,
    n_len: usize,
    /// k = H(N | PAD(g))
    pub k: BigUint,
}

impl Srp {
    pub fn new(group: &SrpGroup) -> Self {
        let n = BigUint::parse_bytes(group.n_hex.as_bytes(), 16).expect("valid N hex");
        let g = BigUint::from(group.g);
        let n_len = n.to_bytes_be().len();
        let k = BigUint::from_bytes_be(&h(&[&n.to_bytes_be(), &pad(&g, n_len)]));
        Srp { n, g, n_len, k }
    }

    /// x = H(s | H(I | ":" | P))
    pub fn x(&self, salt: &[u8], identity: &[u8], password: &[u8]) -> BigUint {
        let inner = h(&[identity, b":", password]);
        BigUint::from_bytes_be(&h(&[salt, &inner]))
    }

    /// u = H(PAD(A) | PAD(B))
    pub fn u(&self, a_pub: &BigUint, b_pub: &BigUint) -> BigUint {
        BigUint::from_bytes_be(&h(&[&pad(a_pub, self.n_len), &pad(b_pub, self.n_len)]))
    }

    /// verifier v = g^x mod N
    pub fn verifier(&self, x: &BigUint) -> BigUint {
        self.g.modpow(x, &self.n)
    }

    /// Client: A = g^a mod N
    pub fn client_public(&self, a: &BigUint) -> BigUint {
        self.g.modpow(a, &self.n)
    }

    /// Server: B = (k*v + g^b) mod N
    pub fn server_public(&self, v: &BigUint, b: &BigUint) -> BigUint {
        (&self.k * v + self.g.modpow(b, &self.n)) % &self.n
    }

    /// Client premaster: S = (B - k * g^x) ^ (a + u*x) mod N
    pub fn client_premaster(&self, b_pub: &BigUint, x: &BigUint, a: &BigUint, u: &BigUint) -> BigUint {
        let gx = self.g.modpow(x, &self.n);
        let kgx = (&self.k * gx) % &self.n;
        // (B - kgx) mod N, safe even when B < kgx
        let base = (b_pub % &self.n + &self.n - kgx) % &self.n;
        let exp = a + u * x;
        base.modpow(&exp, &self.n)
    }

    /// Server premaster: S = (A * v^u) ^ b mod N
    pub fn server_premaster(&self, a_pub: &BigUint, v: &BigUint, u: &BigUint, b: &BigUint) -> BigUint {
        let avu = (a_pub * v.modpow(u, &self.n)) % &self.n;
        avu.modpow(b, &self.n)
    }
}

/// A must be non-zero mod N (guards against a trivial attack).
pub fn valid_public(p: &BigUint, n: &BigUint) -> bool {
    !(p % n).is_zero()
}
