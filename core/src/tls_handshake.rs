//! TLS 1.2 handshake for TLS-SRP (RFC 5246 + RFC 5054). Key derivation from the
//! SRP premaster and (de)serialization of the handshake messages specific to
//! ciphersuite 0xC01D.

use num_bigint::BigUint;
use sha2::{Digest, Sha256};

use crate::tls_prf::prf;
use crate::tls_record::DirKeys;

/// master_secret = PRF(premaster, "master secret", client_random || server_random)[0..48]
pub fn master_secret(premaster: &[u8], client_random: &[u8; 32], server_random: &[u8; 32]) -> [u8; 48] {
    let mut seed = Vec::with_capacity(64);
    seed.extend_from_slice(client_random);
    seed.extend_from_slice(server_random);
    let out = prf(premaster, b"master secret", &seed, 48);
    let mut ms = [0u8; 48];
    ms.copy_from_slice(&out);
    ms
}

/// Extended Master Secret (RFC 7627): master = PRF(premaster, "extended master secret", session_hash).
/// session_hash = SHA256(ClientHello .. ClientKeyExchange).
pub fn extended_master_secret(premaster: &[u8], session_hash: &[u8]) -> [u8; 48] {
    let out = prf(premaster, b"extended master secret", session_hash, 48);
    let mut ms = [0u8; 48];
    ms.copy_from_slice(&out);
    ms
}

/// Key expansion for AES-128-CBC-SHA: mac 20 B, key 16 B, no fixed IV.
/// key_block = PRF(master, "key expansion", server_random || client_random, 72).
/// Returns (client_write_keys, server_write_keys).
pub fn key_block(master: &[u8; 48], client_random: &[u8; 32], server_random: &[u8; 32]) -> (DirKeys, DirKeys) {
    let mut seed = Vec::with_capacity(64);
    seed.extend_from_slice(server_random);
    seed.extend_from_slice(client_random);
    let kb = prf(master, b"key expansion", &seed, 2 * (20 + 16));
    let mut cm = [0u8; 20]; cm.copy_from_slice(&kb[0..20]);
    let mut sm = [0u8; 20]; sm.copy_from_slice(&kb[20..40]);
    let mut ck = [0u8; 16]; ck.copy_from_slice(&kb[40..56]);
    let mut sk = [0u8; 16]; sk.copy_from_slice(&kb[56..72]);
    (DirKeys { mac_key: cm, enc_key: ck }, DirKeys { mac_key: sm, enc_key: sk })
}

/// Finished verify_data = PRF(master, label, SHA256(handshake_messages))[0..12].
/// label = "client finished" / "server finished".
pub fn finished_verify_data(master: &[u8; 48], label: &[u8], handshake_hash: &[u8]) -> [u8; 12] {
    let out = prf(master, label, handshake_hash, 12);
    let mut v = [0u8; 12];
    v.copy_from_slice(&out);
    v
}

/// Accumulator of handshake messages (for session_hash and Finished verify_data).
#[derive(Default)]
pub struct Transcript(pub Vec<u8>);
impl Transcript {
    pub fn add(&mut self, handshake_msg: &[u8]) { self.0.extend_from_slice(handshake_msg); }
    pub fn hash(&self) -> Vec<u8> { Sha256::digest(&self.0).to_vec() }
}

/// Wraps a handshake message in the `type(1) | len(3)` header.
pub fn hs_wrap(htype: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![htype, (body.len() >> 16) as u8, (body.len() >> 8) as u8, body.len() as u8];
    out.extend_from_slice(body);
    out
}

/// Unwraps one TLS handshake message `type(1)|len(3)|body` from a buffer.
pub fn handshake_body(hs: &[u8]) -> Option<(u8, &[u8])> {
    if hs.len() < 4 { return None; }
    let typ = hs[0];
    let len = ((hs[1] as usize) << 16) | ((hs[2] as usize) << 8) | hs[3] as usize;
    if 4 + len > hs.len() { return None; }
    Some((typ, &hs[4..4 + len]))
}

// ---- handshake messages specific to TLS-SRP ----

/// SRP parameters from the ServerKeyExchange (RFC 5054 §2.8.4).
pub struct ServerSrpParams {
    pub n: BigUint,
    pub g: BigUint,
    pub salt: Vec<u8>,
    pub b_pub: BigUint,
}

/// Parses the ServerKeyExchange body (without TLS record/handshake header):
/// opaque N<1..2^16-1> | g<1..2^16-1> | salt<1..2^8-1> | B<1..2^16-1>.
pub fn parse_server_key_exchange(body: &[u8]) -> Option<ServerSrpParams> {
    let mut p = 0usize;
    let r2 = |p: &mut usize| -> Option<Vec<u8>> {
        if *p + 2 > body.len() { return None; }
        let l = u16::from_be_bytes([body[*p], body[*p + 1]]) as usize; *p += 2;
        if *p + l > body.len() { return None; }
        let v = body[*p..*p + l].to_vec(); *p += l; Some(v)
    };
    let r1 = |p: &mut usize| -> Option<Vec<u8>> {
        if *p + 1 > body.len() { return None; }
        let l = body[*p] as usize; *p += 1;
        if *p + l > body.len() { return None; }
        let v = body[*p..*p + l].to_vec(); *p += l; Some(v)
    };
    let n = r2(&mut p)?; let g = r2(&mut p)?; let salt = r1(&mut p)?; let b = r2(&mut p)?;
    Some(ServerSrpParams {
        n: BigUint::from_bytes_be(&n),
        g: BigUint::from_bytes_be(&g),
        salt,
        b_pub: BigUint::from_bytes_be(&b),
    })
}

/// Serializes the ClientKeyExchange for SRP: opaque A<1..2^16-1>.
pub fn build_client_key_exchange(a_pub: &BigUint) -> Vec<u8> {
    let a = a_pub.to_bytes_be();
    let mut body = Vec::with_capacity(2 + a.len());
    body.extend_from_slice(&(a.len() as u16).to_be_bytes());
    body.extend_from_slice(&a);
    // handshake header: type 16 (ClientKeyExchange) | len(3)
    let mut out = vec![16u8, (body.len() >> 16) as u8, (body.len() >> 8) as u8, body.len() as u8];
    out.extend_from_slice(&body);
    out
}

/// Extensions negotiated in the ServerHello.
pub struct ServerHello {
    pub server_random: [u8; 32],
    pub cipher: u16,
    pub session_id: Vec<u8>,
    pub encrypt_then_mac: bool,
    pub extended_master_secret: bool,
}

/// Parses the ServerHello body (without the handshake header).
pub fn parse_server_hello(body: &[u8]) -> Option<ServerHello> {
    let mut p = 0usize;
    if body.len() < 2 + 32 + 1 { return None; }
    p += 2; // version
    let mut sr = [0u8; 32]; sr.copy_from_slice(&body[p..p + 32]); p += 32;
    let sidl = body[p] as usize; p += 1;
    if p + sidl + 3 > body.len() { return None; }
    let session_id = body[p..p + sidl].to_vec(); p += sidl;
    let cipher = u16::from_be_bytes([body[p], body[p + 1]]); p += 2;
    p += 1; // compression
    let (mut etm, mut ems) = (false, false);
    if p + 2 <= body.len() {
        let extl = u16::from_be_bytes([body[p], body[p + 1]]) as usize; p += 2;
        let end = (p + extl).min(body.len());
        while p + 4 <= end {
            let et = u16::from_be_bytes([body[p], body[p + 1]]);
            let el = u16::from_be_bytes([body[p + 2], body[p + 3]]) as usize;
            p += 4 + el;
            match et { 0x0016 => etm = true, 0x0017 => ems = true, _ => {} }
        }
    }
    Some(ServerHello { server_random: sr, cipher, session_id, encrypt_then_mac: etm, extended_master_secret: ems })
}

/// Builds the ClientHello body (without the handshake header) for TLS-SRP with an
/// identity. Byte-for-byte matched to the real DataPlug app's ClientHello captured
/// over SPP (only the 32-byte random differs): ciphers 0xC01D + the renegotiation
/// SCSV 0x00FF; extensions supported_groups (secp256r1, secp521r1), ec_point_formats,
/// encrypt_then_mac, extended_master_secret, session_ticket, SRP(identity).
pub fn build_client_hello(client_random: &[u8; 32], identity: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[0x03, 0x03]);      // client_version TLS 1.2
    b.extend_from_slice(client_random);       // random (32)
    b.push(0);                                // session_id len = 0
    b.extend_from_slice(&[0x00, 0x04]);       // cipher_suites len = 4
    b.extend_from_slice(&[0xC0, 0x1D]);       // TLS_SRP_SHA_WITH_AES_128_CBC_SHA
    b.extend_from_slice(&[0x00, 0xFF]);       // TLS_EMPTY_RENEGOTIATION_INFO_SCSV
    b.push(1); b.push(0);                     // compression: 1 method, null
    // extensions
    let mut ext = Vec::new();
    // supported_groups: secp256r1(0x0017), secp521r1(0x001a)
    ext.extend_from_slice(&[0x00, 0x0a, 0x00, 0x06, 0x00, 0x04, 0x00, 0x17, 0x00, 0x1a]);
    // ec_point_formats: uncompressed
    ext.extend_from_slice(&[0x00, 0x0b, 0x00, 0x02, 0x01, 0x00]);
    // encrypt_then_mac (empty)
    ext.extend_from_slice(&[0x00, 0x16, 0x00, 0x00]);
    // extended_master_secret (empty)
    ext.extend_from_slice(&[0x00, 0x17, 0x00, 0x00]);
    // session_ticket (empty)
    ext.extend_from_slice(&[0x00, 0x23, 0x00, 0x00]);
    // SRP: type 0x000c, len = 1+identity, srp_I<1..255>
    ext.extend_from_slice(&[0x00, 0x0c]);
    ext.extend_from_slice(&((identity.len() + 1) as u16).to_be_bytes());
    ext.push(identity.len() as u8);
    ext.extend_from_slice(identity);
    b.extend_from_slice(&(ext.len() as u16).to_be_bytes());
    b.extend_from_slice(&ext);
    b
}

// ---- server side (for the loopback test / protocol documentation) ----

/// Extracts client_random and the SRP identity from a ClientHello.
pub fn parse_client_hello(body: &[u8]) -> Option<([u8; 32], Vec<u8>)> {
    let mut p = 0usize;
    if body.len() < 2 + 32 + 1 { return None; }
    p += 2;
    let mut cr = [0u8; 32]; cr.copy_from_slice(&body[p..p + 32]); p += 32;
    let sidl = *body.get(p)? as usize; p += 1 + sidl;
    let csl = u16::from_be_bytes([*body.get(p)?, *body.get(p + 1)?]) as usize; p += 2 + csl;
    let cml = *body.get(p)? as usize; p += 1 + cml;
    if p + 2 > body.len() { return None; }
    let extl = u16::from_be_bytes([body[p], body[p + 1]]) as usize; p += 2;
    let end = (p + extl).min(body.len());
    let mut identity = None;
    while p + 4 <= end {
        let et = u16::from_be_bytes([body[p], body[p + 1]]);
        let el = u16::from_be_bytes([body[p + 2], body[p + 3]]) as usize;
        let data = &body[p + 4..(p + 4 + el).min(body.len())];
        if et == 0x000c && !data.is_empty() {
            let ilen = data[0] as usize;
            if 1 + ilen <= data.len() { identity = Some(data[1..1 + ilen].to_vec()); }
        }
        p += 4 + el;
    }
    Some((cr, identity?))
}

/// Builds the ServerHello body (negotiates encrypt_then_mac + extended_master_secret).
pub fn build_server_hello(server_random: &[u8; 32], session_id: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[0x03, 0x03]);
    b.extend_from_slice(server_random);
    b.push(session_id.len() as u8);
    b.extend_from_slice(session_id);
    b.extend_from_slice(&[0xC0, 0x1D]); // cipher
    b.push(0x00);                       // compression null
    let mut ext = Vec::new();
    ext.extend_from_slice(&[0x00, 0x16, 0x00, 0x00]); // encrypt_then_mac
    ext.extend_from_slice(&[0x00, 0x17, 0x00, 0x00]); // extended_master_secret
    b.extend_from_slice(&(ext.len() as u16).to_be_bytes());
    b.extend_from_slice(&ext);
    b
}

/// Builds the ServerKeyExchange body: N<2> | g<2> | salt<1> | B<2>.
pub fn build_server_key_exchange(n: &BigUint, g: &BigUint, salt: &[u8], b_pub: &BigUint) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [n.to_bytes_be(), g.to_bytes_be()] {
        out.extend_from_slice(&(v.len() as u16).to_be_bytes());
        out.extend_from_slice(&v);
    }
    out.push(salt.len() as u8);
    out.extend_from_slice(salt);
    let bb = b_pub.to_bytes_be();
    out.extend_from_slice(&(bb.len() as u16).to_be_bytes());
    out.extend_from_slice(&bb);
    out
}
