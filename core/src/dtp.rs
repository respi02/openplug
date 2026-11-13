//! DTP link framing — the layer the dongle puts between the raw Bluetooth
//! transport (BLE GATT write/notify, or Classic SPP) and the TLS tunnel.
//!
//! Reconstructed from the vendor's `libdongle.so` (functions `dtp_add_header`,
//! `dtp_from_net`, `dtp_needed_bytes`, `dtp_len`, `dtp_data`) and
//! `dat::DTPStream::send` in `libdongles.so`.
//!
//! Frame layout on the wire:
//! ```text
//!   +--------+--------+------+------+----------------- ... --------+
//!   | len hi | len lo | 0x44 | 0x5e |   payload (len bytes)        |
//!   +--------+--------+------+------+----------------- ... --------+
//!     \__ u16 big-endian __/  \__ marker __/
//! ```
//! `len` is the payload length (big-endian), not counting the 4-byte header.
//! `dat::DTPStream::send` emits exactly one frame per `send()` call (header +
//! payload handed to the lower stream in a single write). The receiver
//! (`dtp_needed_bytes`) reads the 2-byte big-endian length to know the total
//! frame size (`len + 4`) and reassembles across packets; it does not validate
//! the 2 marker bytes, so we emit the marker on send and ignore it on receive.

use crate::transport::Transport;
use std::io;

/// The 2 constant marker bytes that follow the length in a DTP header.
pub const DTP_MARKER: [u8; 2] = [0x44, 0x5e];
/// Header size in bytes (u16 length + 2 marker bytes).
pub const DTP_HEADER_LEN: usize = 4;

/// Wraps a payload in one DTP frame: `len_be(2) || 0x44 0x5e || payload`.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let n = payload.len() as u16; // dongle uses a 16-bit length
    let mut out = Vec::with_capacity(payload.len() + DTP_HEADER_LEN);
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&DTP_MARKER);
    out.extend_from_slice(payload);
    out
}

/// Incremental reassembler: feed raw bytes, pop complete payloads.
#[derive(Default)]
pub struct Deframer {
    raw: Vec<u8>,
}

impl Deframer {
    pub fn new() -> Self {
        Self { raw: Vec::new() }
    }

    /// Append freshly received wire bytes.
    pub fn push(&mut self, data: &[u8]) {
        self.raw.extend_from_slice(data);
    }

    /// Pop the next complete payload, or None if a full frame isn't buffered yet.
    pub fn next_payload(&mut self) -> Option<Vec<u8>> {
        if self.raw.len() < DTP_HEADER_LEN {
            return None;
        }
        let len = u16::from_be_bytes([self.raw[0], self.raw[1]]) as usize;
        let total = len + DTP_HEADER_LEN;
        if self.raw.len() < total {
            return None;
        }
        let payload = self.raw[DTP_HEADER_LEN..total].to_vec();
        self.raw.drain(..total);
        Some(payload)
    }
}

/// Transport adapter that adds/removes DTP framing around an inner transport.
/// Place it between the TLS `Session` and the raw Bluetooth transport:
/// `Session<DtpTransport<BleTransport>>`.
pub struct DtpTransport<T: Transport> {
    inner: T,
    deframer: Deframer,
    ready: Vec<u8>, // deframed payload bytes not yet handed to the caller
    rbuf: [u8; 512],
}

impl<T: Transport> DtpTransport<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, deframer: Deframer::new(), ready: Vec::new(), rbuf: [0u8; 512] }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: Transport> Transport for DtpTransport<T> {
    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.inner.send(&frame(data))
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Serve from any already-deframed payload first.
        while self.ready.is_empty() {
            if let Some(p) = self.deframer.next_payload() {
                self.ready = p;
                break;
            }
            let n = self.inner.recv(&mut self.rbuf)?;
            if n == 0 {
                return Ok(0); // closed
            }
            self.deframer.push(&self.rbuf[..n]);
        }
        let n = buf.len().min(self.ready.len());
        buf[..n].copy_from_slice(&self.ready[..n]);
        self.ready.drain(..n);
        Ok(n)
    }
}
