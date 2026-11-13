//! The read path over a live connection: connect → authenticate → subscribe →
//! stream of [`DataElement`]s. Ties the TLS-SRP [`Session`] to the application
//! protocol in [`crate::dongle`].
//!
//! ```no_run
//! # use openplug_core::{link::DongleLink, transport::IoTransport, dongle::APP_SUBSCRIPTIONS};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let stream = std::net::TcpStream::connect("127.0.0.1:4455")?;
//! let mut link = DongleLink::connect(IoTransport(stream), "1234")?;
//! for (param, rate) in APP_SUBSCRIPTIONS {
//!     link.subscribe(param, rate)?;
//! }
//! loop {
//!     for el in link.next_response()?.elements {
//!         println!("{:#08x} = {}", el.url, el.value);
//!     }
//! }
//! # }
//! ```
//!
//! On Bluetooth Classic (RFCOMM/SPP) the TLS records travel directly on the serial
//! channel, so pass the raw transport. On BLE wrap it in
//! [`crate::dtp::DtpTransport`] first.

use crate::dongle::{self, Client, Deframer, Response};
use crate::groups::RFC5054_1536;
use crate::tls_client::{client_handshake, ClientConfig, HsError, Session};
use crate::transport::Transport;
use crate::SRP_IDENTITY_PIN;

/// An authenticated connection to the dongle.
pub struct DongleLink<T: Transport> {
    session: Session<T>,
    client: Client,
    deframer: Deframer,
}

impl<T: Transport> DongleLink<T> {
    /// Runs the TLS-SRP handshake over `transport`, authenticating with the PIN
    /// printed on the dongle's label.
    pub fn connect(transport: T, pin: &str) -> Result<Self, HsError> {
        let cfg = ClientConfig::random(&RFC5054_1536, SRP_IDENTITY_PIN.as_bytes(), pin.as_bytes())?;
        Ok(Self::from_session(client_handshake(transport, &cfg)?))
    }

    /// Wraps an already established session.
    pub fn from_session(session: Session<T>) -> Self {
        Self { session, client: Client::new(), deframer: Deframer::new() }
    }

    /// Subscribes to one diagnostic parameter; returns the transaction id used.
    pub fn subscribe(&mut self, param_id: u32, rate_ms: u32) -> Result<u32, HsError> {
        let txid = self.client.peek_txid();
        let req = self.client.subscribe(param_id, rate_ms);
        self.session.send_app(&req)?;
        Ok(txid)
    }

    /// Sends one of the empty-body getters (command `field`); returns the transaction id.
    pub fn get(&mut self, field: u32) -> Result<u32, HsError> {
        let txid = self.client.peek_txid();
        let req = self.client.get(field);
        self.session.send_app(&req)?;
        Ok(txid)
    }

    /// Blocks until the next complete message arrives; returns its raw protobuf
    /// (length prefix removed). Messages may span, or share, TLS records.
    pub fn next_message(&mut self) -> Result<Vec<u8>, HsError> {
        loop {
            if let Some(msg) = self.deframer.next_message() {
                return Ok(msg);
            }
            let data = self.session.recv_app()?;
            self.deframer.push(&data);
        }
    }

    /// Blocks until the next message arrives and decodes it.
    pub fn next_response(&mut self) -> Result<Response, HsError> {
        Ok(dongle::parse_response(&self.next_message()?))
    }

    /// Orderly shutdown (TLS `close_notify`).
    pub fn close(self) -> Result<(), HsError> {
        self.session.close()
    }
}
