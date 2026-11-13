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
const TAG_URL: u8 = 1 << 3; // 0x08  field 1, varint
const TAG_STATE: u8 = 2 << 3; // 0x10  field 2, varint
const TAG_VALUE: u8 = (5 << 3) | 5; // 0x2d  field 5, fixed32
const TAG_PRECISION: u8 = 10 << 3; // 0x50  field 10, varint

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

// ---- the subscription set and signal catalog (from live_protocol.md) ----

/// The `(paramId, rateMs)` subscriptions the original app opens, in its order.
pub const APP_SUBSCRIPTIONS: [(u32, u32); 17] = [
    (14, 1000), (23, 1000), (16, 10000), (11, 10000), (1, 10000), (7, 10000),
    (4, 10000), (8, 10000), (20, 10000), (44, 10000), (6, 10000), (9, 10000),
    (28, 1000), (19, 10000), (17, 1000), (30, 1000), (34, 1000),
];

/// A labelled data object: display name and unit (empty when dimensionless/unknown).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Signal {
    pub name: &'static str,
    pub unit: &'static str,
}

/// Looks up a data object's meaning. Covers the objects our test car streams
/// (see the catalog in live_protocol.md); other urls return None.
pub fn signal(url: u32) -> Option<Signal> {
    let (name, unit) = match url {
        0x0002_0000 => ("vehicle speed", "km/h"),
        0x0002_0001 => ("fuel level", "%"),
        0x0002_0002 => ("fuel in tank", "l"),
        0x0002_0004 => ("odometer", "km"),
        0x0002_0005 => ("range", "km"),
        0x0002_0006 => ("outside temperature", "°C"),
        0x0002_0009 => ("fuel consumption, instantaneous", "l/h"),
        0x0002_001a => ("fuel consumption, average", "l/100 km"),
        0x0002_0030 => ("oil temperature (dashboard)", "°C"),
        0x0004_0000 => ("engine speed", "rpm"),
        0x0004_0001 => ("coolant temperature", "°C"),
        0x0004_0005 => ("engine load", "%"),
        0x0004_000c => ("intake air pressure", "kPa"),
        0x0004_000d => ("intake air temperature", "°C"),
        0x0004_000e => ("intake air mass flow", "g/s"),
        0x0004_000f => ("oil temperature", "°C"),
        0x0004_0026 => ("throttle position, absolute", "%"),
        0x0004_0027 => ("throttle position, relative", "%"),
        0x0005_0001 => ("battery voltage", "V"),
        0x0014_0001 => ("battery voltage (dashboard)", "V"),
        0x0007_0009 => ("time since last service", "days"),
        0x0007_0012 => ("distance since last service", "km"),
        _ => return None,
    };
    Some(Signal { name, unit })
}

/// Name of a subscribable parameter group (`paramId`), for the ones the app uses.
pub fn param_name(param_id: u32) -> Option<&'static str> {
    Some(match param_id {
        1 => "fuel consumption",
        4 => "odometer",
        6 => "range",
        7 => "fuel level",
        8 => "service",
        9 => "service interval status",
        11 => "dashboard indicators",
        14 => "engine speed",
        16 => "coolant temperature",
        17 => "boost pressure",
        19 => "oil temperature",
        20 => "battery voltage",
        23 => "vehicle speed",
        28 => "intake air",
        30 => "throttle position",
        34 => "catalyst temperature",
        44 => "AdBlue",
        _ => return None,
    })
}

// ---- the dongle's side (for tests and the fake dongle; mirrors the observed shape) ----

/// A decoded request, as far as the protocol is understood.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Subscribe { param_id: u32, rate_ms: u32, txid: u32 },
    /// Any other command field (the getters), with its field number.
    Other { field: u32, txid: u32 },
}

/// Parses a `_TDongleReq` message body (length prefix already removed).
pub fn parse_request(msg: &[u8]) -> Option<Request> {
    use crate::protobuf::{decode, Value};
    let fields = decode(msg).ok()?;
    let txid = fields.iter().find_map(|f| match (f.number, &f.value) {
        (F_TRANSACTION_ID, Value::Varint(v)) => Some(*v as u32),
        _ => None,
    })?;
    let cmd = fields.iter().find(|f| f.number != F_TRANSACTION_ID)?;
    if let (F_SUBSCRIBE, Value::Bytes(sub)) = (cmd.number, &cmd.value) {
        let sub = decode(sub).ok()?;
        let get = |n: u32| sub.iter().find_map(|f| match (f.number, &f.value) {
            (num, Value::Varint(v)) if num == n => Some(*v as u32),
            _ => None,
        });
        return Some(Request::Subscribe { param_id: get(SUB_PARAM)?, rate_ms: get(SUB_RATE)?, txid });
    }
    Some(Request::Other { field: cmd.number, txid })
}

/// Builds a framed data response in the shape the dongle uses:
/// `{ seq(1), len(2), inner(3) }` where `inner` is itself length-prefixed and holds
/// `{ 15: { 1: { paramId(1), repeated DataElement(2) } }, txid(255) }`.
pub fn build_response(seq: u32, param_id: u32, txid: u32, elements: &[DataElement]) -> Vec<u8> {
    let mut group = Builder::default().varint(1, param_id as u64);
    for e in elements {
        let mut el = Builder::default()
            .varint(1, e.url as u64)
            .varint(2, e.state as u64)
            .fixed32(5, e.value.to_bits());
        if let Some(p) = e.precision {
            el = el.varint(10, p as u64);
        }
        group = group.message(2, el);
    }
    let inner = Builder::default()
        .message(15, Builder::default().message(1, group))
        .varint(F_TRANSACTION_ID, txid as u64)
        .build();
    let inner = frame(&inner);
    let outer = Builder::default()
        .varint(1, seq as u64)
        .varint(2, inner.len() as u64)
        .bytes(3, &inner);
    frame(&outer.build())
}
