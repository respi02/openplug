//! Orchestrace TLS 1.2 TLS-SRP handshake (RFC 5246 + 5054 + 7366 EtM + 7627 EMS).
//! Klient (= OpenPlug) i server (= pro loopback test / dokumentaci protokolu).
//! Náhodné hodnoty jsou injektované kvůli determinismu; produkční kód dodá CSPRNG.

use num_bigint::BigUint;

use crate::content_type::{APPLICATION_DATA, CHANGE_CIPHER_SPEC, HANDSHAKE};
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
    #[error("neočekávaná zpráva / formát")] Protocol,
    #[error("server nevybral cipher 0xC01D")] BadCipher,
    #[error("server Finished nesouhlasí")] BadFinished,
    #[error("record: {0}")] Record(crate::tls_record::RecordError),
}

// ---- nízkoúrovňové TLS record rámce přes Transport ----

fn read_exact<T: Transport>(t: &mut T, n: usize) -> Result<Vec<u8>, HsError> {
    let mut buf = vec![0u8; n];
    let mut filled = 0;
    while filled < n {
        let got = t.recv(&mut buf[filled..]).map_err(|_| HsError::Io)?;
        if got == 0 { std::thread::yield_now(); continue; }
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

fn write_record<T: Transport>(t: &mut T, ctype: u8, payload: &[u8]) -> Result<(), HsError> {
    let mut rec = vec![ctype, 0x03, 0x03, (payload.len() >> 8) as u8, payload.len() as u8];
    rec.extend_from_slice(payload);
    t.send(&rec).map_err(|_| HsError::Io)
}

/// Navázaná session: šifrovaný obousměrný kanál pro aplikační data (EtM).
pub struct Session<'a, T: Transport> {
    t: &'a mut T,
    write_keys: DirKeys,
    read_keys: DirKeys,
    write_seq: u64,
    read_seq: u64,
    iv_ctr: u8,
}

impl<'a, T: Transport> Session<'a, T> {
    fn next_iv(&mut self) -> [u8; 16] { self.iv_ctr = self.iv_ctr.wrapping_add(1); [self.iv_ctr; 16] }

    /// Odešle aplikační data (jeden šifrovaný record).
    pub fn send_app(&mut self, data: &[u8]) -> Result<(), HsError> {
        let iv = self.next_iv();
        let rec = seal(&self.write_keys, self.write_seq, APPLICATION_DATA, data, &iv);
        self.write_seq += 1;
        write_record(self.t, APPLICATION_DATA, &rec)
    }

    /// Přečte jeden aplikační record a dešifruje.
    pub fn recv_app(&mut self) -> Result<Vec<u8>, HsError> {
        let (ct, payload) = read_record(self.t)?;
        if ct != APPLICATION_DATA { return Err(HsError::Protocol); }
        let pt = open(&self.read_keys, self.read_seq, APPLICATION_DATA, &payload).map_err(HsError::Record)?;
        self.read_seq += 1;
        Ok(pt)
    }
}

/// Parametry injektované kvůli determinismu testů.
pub struct ClientConfig<'a> {
    pub group: &'a SrpGroup,
    pub identity: &'a [u8],
    pub password: &'a [u8],
    pub client_random: [u8; 32],
    pub a: BigUint,       // SRP efemérní privát klienta
    pub finished_iv: [u8; 16],
}

/// Provede klientský handshake přes `t` a vrátí navázanou `Session`.
pub fn client_handshake<'a, T: Transport>(t: &'a mut T, cfg: &ClientConfig) -> Result<Session<'a, T>, HsError> {
    let srp = Srp::new(cfg.group);
    let mut tr = hs::Transcript::default();

    // 1) ClientHello
    let ch = hs::hs_wrap(HT_CLIENT_HELLO, &hs::build_client_hello(&cfg.client_random, cfg.identity));
    tr.add(&ch);
    write_record(t, HANDSHAKE, &ch)?;

    // 2) čti handshake zprávy (SH, SKE, SHD) — bufferuj napříč recordy
    let mut hbuf: Vec<u8> = Vec::new();
    let mut server_hello: Option<hs::ServerHello> = None;
    let mut srp_params: Option<hs::ServerSrpParams> = None;
    loop {
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
                HT_SERVER_HELLO_DONE => { hbuf.drain(..consumed); break; }
                _ => {}
            }
            hbuf.drain(..consumed);
        }
        if server_hello.is_some() && srp_params.is_some() && hbuf.is_empty() { break; }
        let (ct, payload) = read_record(t)?;
        if ct != HANDSHAKE { return Err(HsError::Protocol); }
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
    let cke = hs::build_client_key_exchange(&a_pub); // už obsahuje hs hlavičku
    tr.add(&cke);
    write_record(t, HANDSHAKE, &cke)?;

    // 5) klíče (EMS, protože dongle vyjednal extended_master_secret)
    let session_hash = tr.hash(); // CH..CKE
    let master = if sh.extended_master_secret {
        hs::extended_master_secret(&premaster, &session_hash)
    } else {
        hs::master_secret(&premaster, &cfg.client_random, &sh.server_random)
    };
    let (client_keys, server_keys) = hs::key_block(&master, &cfg.client_random, &sh.server_random);

    // 6) ChangeCipherSpec + (šifrovaná) Finished
    write_record(t, CHANGE_CIPHER_SPEC, &[0x01])?;
    let cfin_vd = hs::finished_verify_data(&master, b"client finished", &tr.hash());
    let cfin = hs::hs_wrap(HT_FINISHED, &cfin_vd);
    tr.add(&cfin); // Finished je součást transcriptu pro server Finished
    let sealed = seal(&client_keys, 0, HANDSHAKE, &cfin, &cfg.finished_iv);
    write_record(t, HANDSHAKE, &sealed)?;

    // 7) server ChangeCipherSpec + Finished
    let (ct1, _) = read_record(t)?;
    if ct1 != CHANGE_CIPHER_SPEC { return Err(HsError::Protocol); }
    let (ct2, sfin_rec) = read_record(t)?;
    if ct2 != HANDSHAKE { return Err(HsError::Protocol); }
    let sfin = open(&server_keys, 0, HANDSHAKE, &sfin_rec).map_err(HsError::Record)?;
    let (ht, vd, _) = take_handshake(&sfin).ok_or(HsError::Protocol)?;
    if ht != HT_FINISHED { return Err(HsError::Protocol); }
    let exp = hs::finished_verify_data(&master, b"server finished", &tr.hash());
    if vd != exp { return Err(HsError::BadFinished); }

    Ok(Session { t, write_keys: client_keys, read_keys: server_keys, write_seq: 1, read_seq: 1, iv_ctr: 0x40 })
}

/// Vyřízne jednu kompletní handshake zprávu z bufferu: (typ, tělo, spotřebováno).
fn take_handshake(buf: &[u8]) -> Option<(u8, &[u8], usize)> {
    if buf.len() < 4 { return None; }
    let len = ((buf[1] as usize) << 16) | ((buf[2] as usize) << 8) | buf[3] as usize;
    if 4 + len > buf.len() { return None; }
    Some((buf[0], &buf[4..4 + len], 4 + len))
}

/// Serverová konfigurace (loopback / dokumentace). V reálu je server = dongle.
pub struct ServerConfig<'a> {
    pub group: &'a SrpGroup,
    pub password: &'a [u8],
    pub salt: Vec<u8>,
    pub server_random: [u8; 32],
    pub b: BigUint,            // SRP efemérní privát serveru
    pub session_id: Vec<u8>,
    pub finished_iv: [u8; 16],
}

/// Serverová strana handshake (zrcadlo klienta). Vrací navázanou Session.
pub fn server_handshake<'a, T: Transport>(t: &'a mut T, cfg: &ServerConfig) -> Result<Session<'a, T>, HsError> {
    let srp = Srp::new(cfg.group);
    let mut tr = hs::Transcript::default();

    // 1) ClientHello
    let (ct, ch_payload) = read_record(t)?;
    if ct != HANDSHAKE { return Err(HsError::Protocol); }
    let (ht, ch_body, consumed) = take_handshake(&ch_payload).ok_or(HsError::Protocol)?;
    if ht != HT_CLIENT_HELLO { return Err(HsError::Protocol); }
    tr.add(&ch_payload[..consumed]);
    let (client_random, identity) = hs::parse_client_hello(ch_body).ok_or(HsError::Protocol)?;

    // 2) verifier, B; SH + SKE + SHD (jeden record)
    let x = srp.x(&cfg.salt, &identity, cfg.password);
    let v = srp.verifier(&x);
    let b_pub = srp.server_public(&v, &cfg.b);

    let sh = hs::hs_wrap(HT_SERVER_HELLO, &hs::build_server_hello(&cfg.server_random, &cfg.session_id));
    let ske = hs::hs_wrap(HT_SERVER_KEY_EXCHANGE,
        &hs::build_server_key_exchange(&srp.n, &srp.g, &cfg.salt, &b_pub));
    let shd = hs::hs_wrap(HT_SERVER_HELLO_DONE, &[]);
    for m in [&sh, &ske, &shd] { tr.add(m); }
    let mut flight = Vec::new();
    flight.extend_from_slice(&sh); flight.extend_from_slice(&ske); flight.extend_from_slice(&shd);
    write_record(t, HANDSHAKE, &flight)?;

    // 3) ClientKeyExchange → A
    let (ct2, cke_payload) = read_record(t)?;
    if ct2 != HANDSHAKE { return Err(HsError::Protocol); }
    let (ht2, cke_body, consumed2) = take_handshake(&cke_payload).ok_or(HsError::Protocol)?;
    if ht2 != HT_CLIENT_KEY_EXCHANGE { return Err(HsError::Protocol); }
    tr.add(&cke_payload[..consumed2]);
    if cke_body.len() < 2 { return Err(HsError::Protocol); }
    let alen = u16::from_be_bytes([cke_body[0], cke_body[1]]) as usize;
    let a_pub = BigUint::from_bytes_be(&cke_body[2..2 + alen]);

    // 4) premaster (server) + klíče (EMS)
    let u = srp.u(&a_pub, &b_pub);
    let premaster = srp.server_premaster(&a_pub, &v, &u, &cfg.b).to_bytes_be();
    let session_hash = tr.hash(); // CH..CKE
    let master = hs::extended_master_secret(&premaster, &session_hash);
    let (client_keys, server_keys) = hs::key_block(&master, &client_random, &cfg.server_random);

    // 5) klientův CCS + Finished (ověř)
    let (c1, _) = read_record(t)?;
    if c1 != CHANGE_CIPHER_SPEC { return Err(HsError::Protocol); }
    let (c2, cfin_rec) = read_record(t)?;
    if c2 != HANDSHAKE { return Err(HsError::Protocol); }
    let cfin = open(&client_keys, 0, HANDSHAKE, &cfin_rec).map_err(HsError::Record)?;
    let (cht, cvd, cconsumed) = take_handshake(&cfin).ok_or(HsError::Protocol)?;
    if cht != HT_FINISHED { return Err(HsError::Protocol); }
    let exp_c = hs::finished_verify_data(&master, b"client finished", &tr.hash());
    if cvd != exp_c { return Err(HsError::BadFinished); }
    tr.add(&cfin[..cconsumed]); // přidej klientův Finished pro server Finished

    // 6) server CCS + Finished
    write_record(t, CHANGE_CIPHER_SPEC, &[0x01])?;
    let svd = hs::finished_verify_data(&master, b"server finished", &tr.hash());
    let sfin = hs::hs_wrap(HT_FINISHED, &svd);
    let sealed = seal(&server_keys, 0, HANDSHAKE, &sfin, &cfg.finished_iv);
    write_record(t, HANDSHAKE, &sealed)?;

    Ok(Session { t, write_keys: server_keys, read_keys: client_keys, write_seq: 1, read_seq: 1, iv_ctr: 0x80 })
}
