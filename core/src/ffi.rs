//! UniFFI facade: the read API ([`crate::link`]) as seen from Kotlin and Swift.
//!
//! The app implements [`ByteTransport`] on top of its Bluetooth channel (Android:
//! an RFCOMM `BluetoothSocket`), hands it to [`Dongle::connect`] with the label PIN,
//! subscribes, and then calls [`Dongle::next_readings`] in a loop on a background
//! thread. All calls block; none may run on the UI thread.

use std::sync::{Arc, Mutex};

use crate::dongle::{self, APP_SUBSCRIPTIONS};
use crate::link::DongleLink;
use crate::tls_client::HsError;
use crate::transport::Transport;

/// Why a transport call failed (raised by the app's [`ByteTransport`]).
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TransportError {
    #[error("Bluetooth I/O failed: {0}")]
    Io(String),
}

impl From<uniffi::UnexpectedUniFFICallbackError> for TransportError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Io(e.reason)
    }
}

/// The byte channel to the dongle, implemented by the app.
#[uniffi::export(foreign)]
pub trait ByteTransport: Send + Sync {
    /// Sends all of `data`.
    fn send(&self, data: Vec<u8>) -> Result<(), TransportError>;
    /// Blocks until at least one byte is available and returns up to `max_len`
    /// bytes. An empty result means the connection is closed.
    fn recv(&self, max_len: u32) -> Result<Vec<u8>, TransportError>;
}

struct Foreign(Arc<dyn ByteTransport>);

impl Transport for Foreign {
    fn send(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.0.send(data.to_vec()).map_err(std::io::Error::other)
    }
    fn recv(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let data = self.0.recv(buf.len() as u32).map_err(std::io::Error::other)?;
        if data.len() > buf.len() {
            return Err(std::io::Error::other("transport returned more than max_len"));
        }
        buf[..data.len()].copy_from_slice(&data);
        Ok(data.len())
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error, PartialEq)]
#[uniffi(flat_error)]
pub enum DongleError {
    #[error("Bluetooth connection failed")]
    Io,
    #[error("the dongle closed the connection")]
    Closed,
    #[error("the dongle rejected the PIN")]
    WrongPin,
    #[error("the dongle sent TLS alert {0}")]
    Alert(u8),
    #[error("unexpected data from the dongle: {0}")]
    Protocol(String),
}

impl From<HsError> for DongleError {
    fn from(e: HsError) -> Self {
        match e {
            HsError::Io => Self::Io,
            HsError::Closed => Self::Closed,
            HsError::Alert { description, .. } => Self::Alert(description),
            other => Self::Protocol(other.to_string()),
        }
    }
}

/// One live value, in `unit` when the signal is known.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Reading {
    /// Numeric data-object id (`category << 16 | index`).
    pub url: u32,
    /// False when the dongle reports no data for this object right now.
    pub available: bool,
    pub value: f32,
    /// Suggested number of decimals.
    pub precision: u32,
    /// Signal name and unit, when the object's meaning is known.
    pub name: Option<String>,
    pub unit: Option<String>,
}

impl From<dongle::DataElement> for Reading {
    fn from(e: dongle::DataElement) -> Self {
        let sig = dongle::signal(e.url);
        Self {
            url: e.url,
            available: e.state != dongle::state::NODATA,
            value: e.value,
            precision: e.precision.unwrap_or(1),
            name: sig.map(|s| s.name.to_string()),
            unit: sig.map(|s| s.unit.to_string()),
        }
    }
}

/// An authenticated connection to the dongle.
///
/// Calls are serialized: while one thread is blocked in [`Dongle::next_readings`],
/// other calls wait. Subscribe first, then read.
#[derive(uniffi::Object)]
pub struct Dongle {
    link: Mutex<DongleLink<Foreign>>,
}

#[uniffi::export]
impl Dongle {
    /// Authenticates with the PIN printed on the dongle's label (TLS-SRP).
    #[uniffi::constructor]
    pub fn connect(transport: Arc<dyn ByteTransport>, pin: String) -> Result<Arc<Self>, DongleError> {
        let link = DongleLink::connect(Foreign(transport), &pin).map_err(|e| match e {
            // bad_record_mac in reply to our Finished: the SRP secrets differ
            HsError::Alert { description: 20, .. } => DongleError::WrongPin,
            other => other.into(),
        })?;
        Ok(Arc::new(Self { link: Mutex::new(link) }))
    }

    /// Subscribes to one diagnostic parameter, updated every `rate_ms`.
    pub fn subscribe(&self, param_id: u32, rate_ms: u32) -> Result<(), DongleError> {
        self.link.lock().unwrap().subscribe(param_id, rate_ms)?;
        Ok(())
    }

    /// Subscribes to the same parameters, at the same rates, as the original app.
    pub fn subscribe_defaults(&self) -> Result<(), DongleError> {
        let mut link = self.link.lock().unwrap();
        for (param, rate) in APP_SUBSCRIPTIONS {
            link.subscribe(param, rate)?;
        }
        Ok(())
    }

    /// Blocks until the dongle sends its next message and returns the values in it
    /// (possibly none, for messages that carry no readings).
    pub fn next_readings(&self) -> Result<Vec<Reading>, DongleError> {
        let rsp = self.link.lock().unwrap().next_response()?;
        Ok(rsp.elements.into_iter().map(Reading::from).collect())
    }
}
