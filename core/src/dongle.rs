//! OpenPlug application protocol — `_TDongleReq` (requests) and `_TDongleRsp`
//! (responses), implemented from `research/notes/live_protocol.md` (the decoded
//! spec), not from any vendor code.
//!
//! Inside the TLS tunnel every message is length-delimited:
//! ```text
//!   varint(len) || protobuf
//! ```
//! A request carries a **transaction id in field 255**, echoed on its response.
//!
//! Request — Subscribe (field 16): `{ mode=0 (1), paramId (2), rateMs (3) }`.
//! Response — `{ seq (1), …, payload (3) }`, where the payload holds repeated
//! **DataElement** `{ url (1), state (2), value=float32-LE (5), precision (10) }`.
//! A high-entropy "secured" block (field 253) defeats strict protobuf parsing, so
//! responses are read by byte-scanning for the element pattern
//! `08<url> 10<state> 2d<f32> 50<prec>` rather than by decoding the container.

use crate::protobuf::{decode_varint, encode_varint, Builder};

// ---- field numbers (from live_protocol.md) ----
const F_SUBSCRIBE: u32 = 16;
const F_TRANSACTION_ID: u32 = 255;
const SUB_MODE: u32 = 1;
const SUB_PARAM: u32 = 2;
const SUB_RATE: u32 = 3;

// DataElement wire tags (field<<3 | wiretype)
const TAG_URL: u8 = (1 << 3) | 0; // 0x08  field 1, varint
const TAG_STATE: u8 = (2 << 3) | 0; // 0x10  field 2, varint
const TAG_VALUE: u8 = (5 << 3) | 5; // 0x2d  field 5, fixed32
const TAG_PRECISION: u8 = (10 << 3) | 0; // 0x50  field 10, varint

/// DataElement state values seen on the wire.
pub mod state {
    pub const OK: u32 = 0;
    pub const NODATA: u32 = 4;
}

// ---- length-delimited application framing ----

/// Wraps one protobuf message with its varint length prefix (ready for the TLS tunnel).
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 5);
    encode_varint(&mut out, payload.len() as u64);
    out.extend_from_slice(payload);
    out
}

/// Reassembles length-delimited messages from the (possibly chunked) app byte stream.
#[derive(Default)]
pub struct Deframer {
    buf: Vec<u8>,
}

impl Deframer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }
    /// Pops the next complete message, or None if one isn't fully buffered yet.
    pub fn next_message(&mut self) -> Option<Vec<u8>> {
        let mut p = 0usize;
        let len = decode_varint(&self.buf, &mut p).ok()? as usize;
        if self.buf.len() < p + len {
            return None;
        }
        let msg = self.buf[p..p + len].to_vec();
        self.buf.drain(..p + len);
        Some(msg)
    }
}

// ---- requests (_TDongleReq) ----

/// Builds a framed Subscribe request for one data-object `param_id` at `rate_ms`
/// (seen: 1000 ms or 10000 ms), with the given transaction id.
pub fn build_subscribe(param_id: u32, rate_ms: u32, txid: u32) -> Vec<u8> {
    let sub = Builder::default()
        .varint(SUB_MODE, 0)
        .varint(SUB_PARAM, param_id as u64)
        .varint(SUB_RATE, rate_ms as u64);
    let req = Builder::default()
        .message(F_SUBSCRIBE, sub)
        .varint(F_TRANSACTION_ID, txid as u64);
    frame(&req.build())
}

/// Builds a framed empty-body request (the simple getters live at fields 1/4/5/22),
/// with the given transaction id.
pub fn build_get(field: u32, txid: u32) -> Vec<u8> {
    let req = Builder::default()
        .bytes(field, &[])
        .varint(F_TRANSACTION_ID, txid as u64);
    frame(&req.build())
}

/// Stateful client that allocates incrementing transaction ids.
pub struct Client {
    next_txid: u32,
}

impl Default for Client {
    fn default() -> Self {
        Self { next_txid: 1 }
    }
}

impl Client {
    pub fn new() -> Self {
        Self::default()
    }
    /// The transaction id that will be used by the next request.
    pub fn peek_txid(&self) -> u32 {
        self.next_txid
    }
    fn next_tx(&mut self) -> u32 {
        let t = self.next_txid;
        self.next_txid = self.next_txid.wrapping_add(1);
        t
    }
    pub fn subscribe(&mut self, param_id: u32, rate_ms: u32) -> Vec<u8> {
        let tx = self.next_tx();
        build_subscribe(param_id, rate_ms, tx)
    }
    pub fn get(&mut self, field: u32) -> Vec<u8> {
        let tx = self.next_tx();
        build_get(field, tx)
    }
}

// ---- responses (_TDongleRsp) ----

/// One live value from the dongle.
#[derive(Debug, Clone, PartialEq)]
pub struct DataElement {
    /// Numeric object id. `url = (category << 16) | index` (e.g. 0x0005_0001 = battery V).
    pub url: u32,
    pub state: u32,
    pub value: f32,
    pub precision: Option<u32>,
}

/// A parsed response: the sequence counter, the matching transaction id (best-effort),
/// and the data elements it carried.
#[derive(Debug, Clone, Default)]
pub struct Response {
    pub seq: Option<u32>,
    pub transaction_id: Option<u32>,
    pub elements: Vec<DataElement>,
}

/// Parses a `_TDongleRsp` message body (the protobuf, length prefix already removed).
pub fn parse_response(msg: &[u8]) -> Response {
    Response {
        seq: read_seq(msg),
        transaction_id: find_transaction_id(msg),
        elements: scan_elements(msg),
    }
}

/// field 1 (seq) is the first field of the message.
fn read_seq(b: &[u8]) -> Option<u32> {
    if b.first() != Some(&TAG_URL) {
        return None;
    }
    let mut p = 1usize;
    decode_varint(b, &mut p).ok().map(|v| v as u32)
}

/// field 255 (transaction id) sits after the un-parseable block, so scan for its tag
/// bytes (0xF8 0x0F) and take the last plausible value. Best-effort / informational.
fn find_transaction_id(b: &[u8]) -> Option<u32> {
    let mut found = None;
    let mut i = 0usize;
    while i + 2 <= b.len() {
        if b[i] == 0xF8 && b[i + 1] == 0x0F {
            let mut p = i + 2;
            if let Ok(v) = decode_varint(b, &mut p) {
                found = Some(v as u32);
            }
        }
        i += 1;
    }
    found
}

/// Byte-scan for DataElements: `08<url> 10<state> 2d<f32-LE> [50<prec>]`.
/// Robust against the field-253 "secured" block that defeats strict parsing.
fn scan_elements(b: &[u8]) -> Vec<DataElement> {
    let mut out = Vec::new();
    let n = b.len();
    let mut i = 0usize;
    while i < n {
        if b[i] == TAG_URL {
            if let Some(el) = try_element(b, i) {
                let (elem, end) = el;
                out.push(elem);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Tries to read a DataElement starting at the `0x08` tag at `start`.
/// Returns the element and the index just past it, or None if it doesn't match.
fn try_element(b: &[u8], start: usize) -> Option<(DataElement, usize)> {
    let n = b.len();
    let mut j = start + 1;
    let url = decode_varint(b, &mut j).ok()?;
    if url == 0 || url >= (1 << 24) {
        return None; // implausible object id
    }
    let mut state = state::OK;
    if j < n && b[j] == TAG_STATE {
        j += 1;
        state = decode_varint(b, &mut j).ok()? as u32;
    }
    // the value is mandatory for a reading we care about
    if j + 5 > n || b[j] != TAG_VALUE {
        return None;
    }
    let value = f32::from_le_bytes([b[j + 1], b[j + 2], b[j + 3], b[j + 4]]);
    j += 5;
    let mut precision = None;
    if j < n && b[j] == TAG_PRECISION {
        j += 1;
        precision = Some(decode_varint(b, &mut j).ok()? as u32);
    }
    Some((
        DataElement {
            url: url as u32,
            state,
            value,
            precision,
        },
        j,
    ))
}
