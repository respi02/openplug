//! TLS 1.2 TLS-SRP handshake orchestration (RFC 5246 + 5054 + 7366 EtM + 7627 EMS).
//! Client (= OpenPlug) and server (= for the loopback test / protocol documentation).
//! The handshake randoms are injected via [`ClientConfig`] / [`ServerConfig`] so tests
//! are deterministic; production builds them with [`ClientConfig::random`] (OS CSPRNG).
//! Per-record CBC IVs of an established [`Session`] always come from the OS CSPRNG.

use num_bigint::BigUint;

use crate::content_type::{ALERT, APPLICATION_DATA, CHANGE_CIPHER_SPEC, HANDSHAKE};
use crate::groups::SrpGroup;
use crate::srp::Srp;
use crate::tls_handshake as hs;
use crate::tls_record::{open, seal, DirKeys};
use crate::transport::Transport;

const HT_CLIENT_HELLO: u8 = 1;
const HT_SERVER_HELLO: u8 = 2;
const HT_SERVER_KEY_EXCHANGE: u8 = 12;
const HT_SERVER_HELLO_DONE: u8 = 14;
const HT_CLIENT_KEY_EXCHANGE: u8 = 16;
const HT_FINISHED: u8 = 20;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum HsError {
    #[error("I/O")] Io,
    #[error("connection closed by peer")] Closed,
    /// The peer sent a TLS alert (RFC 5246 §7.2). During the handshake, a
    /// `bad_record_mac` (20) right after our Finished means the PIN was wrong.
    #[error("TLS alert from peer: level {level}, description {description}")]
    Alert { level: u8, description: u8 },
    #[error("OS random number generator failed")] Rng,
    #[error("unexpected message / format")] Protocol,
    #[error("server did not select cipher 0xC01D")] BadCipher,
    #[error("server Finished mismatch")] BadFinished,
    #[error("record: {0}")] Record(crate::tls_record::RecordError),
}

// ---- low-level TLS record framing over a Transport ----

fn read_exact<T: Transport>(t: &mut T, n: usize) -> Result<Vec<u8>, HsError> {
    let mut buf = vec![0u8; n];
    let mut filled = 0;
    while filled < n {
        let got = t.recv(&mut buf[filled..]).map_err(|_| HsError::Io)?;
        if got == 0 { return Err(HsError::Closed); } // Transport contract: 0 = closed
        filled += got;
    }
    Ok(buf)
}

fn read_record<T: Transport>(t: &mut T) -> Result<(u8, Vec<u8>), HsError> {
    let hdr = read_exact(t, 5)?;
    let ctype = hdr[0];
    let len = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
    let payload = read_exact(t, len)?;
    Ok((ctype, payload))
}

/// Reads one cleartext record of type `want`; a cleartext alert becomes [`HsError::Alert`].
fn read_plain<T: Transport>(t: &mut T, want: u8) -> Result<Vec<u8>, HsError> {
    let (ct, payload) = read_record(t)?;
    if ct == ALERT && payload.len() == 2 {
        return Err(HsError::Alert { level: payload[0], description: payload[1] });
    }
    if ct != want { return Err(HsError::Protocol); }
    Ok(payload)
}

fn write_record<T: Transport>(t: &mut T, ctype: u8, payload: &[u8]) -> Result<(), HsError> {
    write_record_ver(t, ctype, 0x03, payload)
}

/// `minor` = record-layer minor version (3 = TLS 1.2; the initial ClientHello uses 1).
fn write_record_ver<T: Transport>(t: &mut T, ctype: u8, minor: u8, payload: &[u8]) -> Result<(), HsError> {
    let mut rec = vec![ctype, 0x03, minor, (payload.len() >> 8) as u8, payload.len() as u8];
    rec.extend_from_slice(payload);
    t.send(&rec).map_err(|_| HsError::Io)
}

/// An established session: encrypted bidirectional channel for application data (EtM).
pub struct Session<T: Transport> {
    t: T,
    write_keys: DirKeys,
    read_keys: DirKeys,
    write_seq: u64,
    read_seq: u64,
}

/// Fresh unpredictable CBC IV (TLS 1.2 explicit IV must not be predictable).
fn random_iv() -> Result<[u8; 16], HsError> {
    let mut iv = [0u8; 16];
    getrandom::fill(&mut iv).map_err(|_| HsError::Rng)?;
    Ok(iv)
}

impl<T: Transport> Session<T> {
    /// Sends application data (one encrypted record).
    pub fn send_app(&mut self, data: &[u8]) -> Result<(), HsError> {
        let iv = random_iv()?;
        let rec = seal(&self.write_keys, self.write_seq, APPLICATION_DATA, data, &iv);
        self.write_seq += 1;
        write_record(&mut self.t, APPLICATION_DATA, &rec)
    }

    /// Reads one application record and decrypts it. An encrypted `close_notify`
    /// alert is reported as [`HsError::Closed`], any other alert as [`HsError::Alert`].
    pub fn recv_app(&mut self) -> Result<Vec<u8>, HsError> {
        let (ct, payload) = read_record(&mut self.t)?;
        if ct != APPLICATION_DATA && ct != ALERT { return Err(HsError::Protocol); }
        let pt = open(&self.read_keys, self.read_seq, ct, &payload).map_err(HsError::Record)?;
        self.read_seq += 1;
        if ct == ALERT {
            return Err(match pt[..] {
                [_, 0] => HsError::Closed, // close_notify
                [level, description] => HsError::Alert { level, description },
                _ => HsError::Protocol,
            });
        }
        Ok(pt)
    }

    /// Sends an encrypted `close_notify` so the peer sees an orderly shutdown.
    pub fn close(mut self) -> Result<(), HsError> {
        let iv = random_iv()?;
        let rec = seal(&self.write_keys, self.write_seq, ALERT, &[1, 0], &iv);
        self.write_seq += 1;
        write_record(&mut self.t, ALERT, &rec)
    }
}

/// Parameters injected for deterministic tests.
pub struct ClientConfig<'a> {
    pub group: &'a SrpGroup,
    pub identity: &'a [u8],
    pub password: &'a [u8],
    pub client_random: [u8; 32],
    pub a: BigUint,       // client's SRP ephemeral private
    pub finished_iv: [u8; 16],
}

impl<'a> ClientConfig<'a> {
    /// Production config: client random, SRP ephemeral `a` (256 bits) and the
    /// Finished IV all drawn from the OS CSPRNG.
    pub fn random(group: &'a SrpGroup, identity: &'a [u8], password: &'a [u8]) -> Result<Self, HsError> {
        let mut client_random = [0u8; 32];
        let mut a = [0u8; 32];
        getrandom::fill(&mut client_random).map_err(|_| HsError::Rng)?;
        getrandom::fill(&mut a).map_err(|_| HsError::Rng)?;
        a[0] |= 0x80; // keep the full 256-bit size (and never zero)
        Ok(Self {
            group,
            identity,
            password,
            client_random,
            a: BigUint::from_bytes_be(&a),
            finished_iv: random_iv()?,
        })
    }
}

/// Runs the client handshake over `t` and returns an established `Session`.
pub fn client_handshake<T: Transport>(mut t: T, cfg: &ClientConfig) -> Result<Session<T>, HsError> {
    let srp = Srp::new(cfg.group);
    let mut tr = hs::Transcript::default();

    // 1) ClientHello
    let ch = hs::hs_wrap(HT_CLIENT_HELLO, &hs::build_client_hello(&cfg.client_random, cfg.identity));
    tr.add(&ch);
    // The real app's first record carries version 0x0301 (mbedTLS default); match it.
    write_record_ver(&mut t, HANDSHAKE, 0x01, &ch)?;

    // 2) read handshake messages (SH, SKE, SHD) — buffered across records
    let mut hbuf: Vec<u8> = Vec::new();
    let mut server_hello: Option<hs::ServerHello> = None;
    let mut srp_params: Option<hs::ServerSrpParams> = None;
    // The dongle sends ServerHello, ServerKeyExchange and ServerHelloDone as three
    // separate records; read until ServerHelloDone itself has been consumed.
    let mut hello_done = false;
    while !hello_done {
        while let Some((htype, body, consumed)) = take_handshake(&hbuf) {
            let raw = hbuf[..consumed].to_vec();
            tr.add(&raw);
            match htype {
                HT_SERVER_HELLO => {
                    let sh = hs::parse_server_hello(body).ok_or(HsError::Protocol)?;
                    if sh.cipher != 0xC01D { return Err(HsError::BadCipher); }
                    server_hello = Some(sh);
                }
                HT_SERVER_KEY_EXCHANGE => {
                    srp_params = Some(hs::parse_server_key_exchange(body).ok_or(HsError::Protocol)?);
                }
                HT_SERVER_HELLO_DONE => hello_done = true,
                _ => {}
            }
            hbuf.drain(..consumed);
            if hello_done { break; }
        }
        if hello_done { break; }
        let payload = read_plain(&mut t, HANDSHAKE)?;
        hbuf.extend_from_slice(&payload);
    }
    let sh = server_hello.ok_or(HsError::Protocol)?;
    let sp = srp_params.ok_or(HsError::Protocol)?;

    // 3) SRP: A, premaster
    let x = srp.x(&sp.salt, cfg.identity, cfg.password);
    let a_pub = srp.client_public(&cfg.a);
    let u = srp.u(&a_pub, &sp.b_pub);
    let premaster = srp.client_premaster(&sp.b_pub, &x, &cfg.a, &u).to_bytes_be();

    // 4) ClientKeyExchange
    let cke = hs::build_client_key_exchange(&a_pub); // already has the handshake header
    tr.add(&cke);
    write_record(&mut t, HANDSHAKE, &cke)?;

    // 5) keys (EMS, because the dongle negotiated extended_master_secret)
    let session_hash = tr.hash(); // CH..CKE
    let master = if sh.extended_master_secret {
        hs::extended_master_secret(&premaster, &session_hash)
    } else {
        hs::master_secret(&premaster, &cfg.client_random, &sh.server_random)
    };
    let (client_keys, server_keys) = hs::key_block(&master, &cfg.client_random, &sh.server_random);

    // 6) ChangeCipherSpec + (encrypted) Finished
    write_record(&mut t, CHANGE_CIPHER_SPEC, &[0x01])?;
    let cfin_vd = hs::finished_verify_data(&master, b"client finished", &tr.hash());
    let cfin = hs::hs_wrap(HT_FINISHED, &cfin_vd);
    tr.add(&cfin); // the Finished is part of the transcript for the server Finished
    let sealed = seal(&client_keys, 0, HANDSHAKE, &cfin, &cfg.finished_iv);
    write_record(&mut t, HANDSHAKE, &sealed)?;

    // 7) server ChangeCipherSpec + Finished
    read_plain(&mut t, CHANGE_CIPHER_SPEC)?;
    let (ct2, sfin_rec) = read_record(&mut t)?;
    if ct2 != HANDSHAKE { return Err(HsError::Protocol); }
    let sfin = open(&server_keys, 0, HANDSHAKE, &sfin_rec).map_err(HsError::Record)?;
    let (ht, vd) = hs::handshake_body(&sfin).ok_or(HsError::Protocol)?;
    if ht != HT_FINISHED { return Err(HsError::Protocol); }
    let exp = hs::finished_verify_data(&master, b"server finished", &tr.hash());
    if vd != exp { return Err(HsError::BadFinished); }

    Ok(Session { t, write_keys: client_keys, read_keys: server_keys, write_seq: 1, read_seq: 1 })
}

/// Cuts one complete handshake message out of a buffer: (type, body, consumed).
fn take_handshake(buf: &[u8]) -> Option<(u8, &[u8], usize)> {
    if buf.len() < 4 { return None; }
    let len = ((buf[1] as usize) << 16) | ((buf[2] as usize) << 8) | buf[3] as usize;
    if 4 + len > buf.len() { return None; }
    Some((buf[0], &buf[4..4 + len], 4 + len))
}

/// Server configuration (loopback / documentation). In reality the server = dongle.
pub struct ServerConfig<'a> {
    pub group: &'a SrpGroup,
    pub password: &'a [u8],
    pub salt: Vec<u8>,
    pub server_random: [u8; 32],
    pub b: BigUint,            // server's SRP ephemeral private
    pub session_id: Vec<u8>,
    pub finished_iv: [u8; 16],
}

/// Server side of the handshake (mirror of the client). Returns an established Session.
pub fn server_handshake<T: Transport>(mut t: T, cfg: &ServerConfig) -> Result<Session<T>, HsError> {
    let srp = Srp::new(cfg.group);
    let mut tr = hs::Transcript::default();

    // 1) ClientHello
    let (ct, ch_payload) = read_record(&mut t)?;
    if ct != HANDSHAKE { return Err(HsError::Protocol); }
    let (ht, ch_body, consumed) = take_handshake(&ch_payload).ok_or(HsError::Protocol)?;
    if ht != HT_CLIENT_HELLO { return Err(HsError::Protocol); }
    tr.add(&ch_payload[..consumed]);
    let (client_random, identity) = hs::parse_client_hello(ch_body).ok_or(HsError::Protocol)?;

    // 2) verifier, B; SH + SKE + SHD
    let x = srp.x(&cfg.salt, &identity, cfg.password);
    let v = srp.verifier(&x);
    let b_pub = srp.server_public(&v, &cfg.b);

    let sh = hs::hs_wrap(HT_SERVER_HELLO, &hs::build_server_hello(&cfg.server_random, &cfg.session_id));
    let ske = hs::hs_wrap(HT_SERVER_KEY_EXCHANGE,
        &hs::build_server_key_exchange(&srp.n, &srp.g, &cfg.salt, &b_pub));
    let shd = hs::hs_wrap(HT_SERVER_HELLO_DONE, &[]);
    for m in [&sh, &ske, &shd] { tr.add(m); }
    // One record per message, as the real dongle does.
    for m in [&sh, &ske, &shd] { write_record(&mut t, HANDSHAKE, m)?; }

    // 3) ClientKeyExchange → A
    let (ct2, cke_payload) = read_record(&mut t)?;
    if ct2 != HANDSHAKE { return Err(HsError::Protocol); }
    let (ht2, cke_body, consumed2) = take_handshake(&cke_payload).ok_or(HsError::Protocol)?;
    if ht2 != HT_CLIENT_KEY_EXCHANGE { return Err(HsError::Protocol); }
    tr.add(&cke_payload[..consumed2]);
    if cke_body.len() < 2 { return Err(HsError::Protocol); }
    let alen = u16::from_be_bytes([cke_body[0], cke_body[1]]) as usize;
    let a_pub = BigUint::from_bytes_be(&cke_body[2..2 + alen]);

    // 4) premaster (server) + keys (EMS)
    let u = srp.u(&a_pub, &b_pub);
    let premaster = srp.server_premaster(&a_pub, &v, &u, &cfg.b).to_bytes_be();
    let session_hash = tr.hash(); // CH..CKE
    let master = hs::extended_master_secret(&premaster, &session_hash);
    let (client_keys, server_keys) = hs::key_block(&master, &client_random, &cfg.server_random);

    // 5) client's CCS + Finished (verify)
    let (c1, _) = read_record(&mut t)?;
    if c1 != CHANGE_CIPHER_SPEC { return Err(HsError::Protocol); }
    let (c2, cfin_rec) = read_record(&mut t)?;
    if c2 != HANDSHAKE { return Err(HsError::Protocol); }
    let cfin = open(&client_keys, 0, HANDSHAKE, &cfin_rec).map_err(HsError::Record)?;
    let (cht, cvd) = hs::handshake_body(&cfin).ok_or(HsError::Protocol)?;
    if cht != HT_FINISHED { return Err(HsError::Protocol); }
    let exp_c = hs::finished_verify_data(&master, b"client finished", &tr.hash());
    if cvd != exp_c { return Err(HsError::BadFinished); }
    tr.add(&cfin); // add the client's Finished for the server Finished

    // 6) server CCS + Finished
    write_record(&mut t, CHANGE_CIPHER_SPEC, &[0x01])?;
    let svd = hs::finished_verify_data(&master, b"server finished", &tr.hash());
    let sfin = hs::hs_wrap(HT_FINISHED, &svd);
    let sealed = seal(&server_keys, 0, HANDSHAKE, &sfin, &cfg.finished_iv);
    write_record(&mut t, HANDSHAKE, &sealed)?;

    Ok(Session { t, write_keys: server_keys, read_keys: client_keys, write_seq: 1, read_seq: 1 })
}
