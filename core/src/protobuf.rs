//! Generic protobuf codec (wire format, schema-less) — like `protoc --decode_raw`.
//! Used to decode `_TDongleReq/_TDongleRsp` from captured plaintext and to build
//! requests until we have the full `.proto`. Supports proto2/proto3 wire format.

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Varint(u64),
    Fixed64(u64),
    Bytes(Vec<u8>),
    Fixed32(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub number: u32,
    pub value: Value,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PbError {
    #[error("unexpected end of input")] Eof,
    #[error("varint overflowed 64 bits")] VarintOverflow,
    #[error("unsupported wire type {0}")] BadWireType(u8),
}

fn read_varint(b: &[u8], p: &mut usize) -> Result<u64, PbError> {
    let mut val = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *b.get(*p).ok_or(PbError::Eof)?; *p += 1;
        if shift >= 64 { return Err(PbError::VarintOverflow); }
        val |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 { break; }
        shift += 7;
    }
    Ok(val)
}

/// Decodes one protobuf message into a list of fields (no type interpretation).
pub fn decode(bytes: &[u8]) -> Result<Vec<Field>, PbError> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p < bytes.len() {
        let tag = read_varint(bytes, &mut p)?;
        let number = (tag >> 3) as u32;
        let wt = (tag & 7) as u8;
        let value = match wt {
            0 => Value::Varint(read_varint(bytes, &mut p)?),
            1 => {
                let s = bytes.get(p..p + 8).ok_or(PbError::Eof)?;
                p += 8;
                Value::Fixed64(u64::from_le_bytes(s.try_into().unwrap()))
            }
            2 => {
                let len = read_varint(bytes, &mut p)? as usize;
                let s = bytes.get(p..p + len).ok_or(PbError::Eof)?;
                p += len;
                Value::Bytes(s.to_vec())
            }
            5 => {
                let s = bytes.get(p..p + 4).ok_or(PbError::Eof)?;
                p += 4;
                Value::Fixed32(u32::from_le_bytes(s.try_into().unwrap()))
            }
            other => return Err(PbError::BadWireType(other)),
        };
        out.push(Field { number, value });
    }
    Ok(out)
}

/// Human-readable dump (recursively tries length-delimited fields as nested messages).
pub fn inspect(bytes: &[u8]) -> String {
    fn go(bytes: &[u8], depth: usize, out: &mut String) {
        let ind = "  ".repeat(depth);
        match decode(bytes) {
            Ok(fields) => {
                for f in fields {
                    match &f.value {
                        Value::Varint(v) => out.push_str(&format!("{ind}#{} varint {v}\n", f.number)),
                        Value::Fixed32(v) => out.push_str(&format!("{ind}#{} i32 {v}\n", f.number)),
                        Value::Fixed64(v) => out.push_str(&format!("{ind}#{} i64 {v}\n", f.number)),
                        Value::Bytes(b) => {
                            // try nested message
                            if !b.is_empty() && decode(b).is_ok() && looks_like_message(b) {
                                out.push_str(&format!("{ind}#{} msg {{\n", f.number));
                                go(b, depth + 1, out);
                                out.push_str(&format!("{ind}}}\n"));
                            } else if let Ok(s) = std::str::from_utf8(b) {
                                if s.chars().all(|c| !c.is_control() || c == '\n') {
                                    out.push_str(&format!("{ind}#{} str {s:?}\n", f.number));
                                } else {
                                    out.push_str(&format!("{ind}#{} bytes[{}] {}\n", f.number, b.len(), hex(b)));
                                }
                            } else {
                                out.push_str(&format!("{ind}#{} bytes[{}] {}\n", f.number, b.len(), hex(b)));
                            }
                        }
                    }
                }
            }
            Err(e) => out.push_str(&format!("{ind}<cannot decode: {e}>\n")),
        }
    }
    let mut s = String::new();
    go(bytes, 0, &mut s);
    s
}

fn looks_like_message(b: &[u8]) -> bool {
    // heuristic: parses fully and consumes exactly the whole input
    let mut p = 0usize;
    while p < b.len() {
        let tag = match read_varint(b, &mut p) { Ok(t) => t, Err(_) => return false };
        let wt = (tag & 7) as u8;
        match wt {
            0 => { if read_varint(b, &mut p).is_err() { return false; } }
            1 => { if p + 8 > b.len() { return false; } p += 8; }
            2 => {
                let len = match read_varint(b, &mut p) { Ok(l) => l as usize, Err(_) => return false };
                if p + len > b.len() { return false; } p += len;
            }
            5 => { if p + 4 > b.len() { return false; } p += 4; }
            _ => return false,
        }
    }
    true
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join("")
}

// ---- simple encoder (for building requests) ----

/// Protobuf message builder.
#[derive(Default)]
pub struct Builder(pub Vec<u8>);

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 { byte |= 0x80; }
        out.push(byte);
        if v == 0 { break; }
    }
}

impl Builder {
    pub fn varint(mut self, field: u32, v: u64) -> Self {
        write_varint(&mut self.0, (field as u64) << 3);
        write_varint(&mut self.0, v);
        self
    }
    pub fn bytes(mut self, field: u32, v: &[u8]) -> Self {
        write_varint(&mut self.0, ((field as u64) << 3) | 2);
        write_varint(&mut self.0, v.len() as u64);
        self.0.extend_from_slice(v);
        self
    }
    pub fn message(self, field: u32, sub: Builder) -> Self {
        self.bytes(field, &sub.0)
    }
    pub fn build(self) -> Vec<u8> { self.0 }
}
