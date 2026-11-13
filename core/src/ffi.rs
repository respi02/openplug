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

// ---- trip and refuel journal ----

use crate::journal::{Journal, Refuel, Totals, Trip};

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum JournalError {
    #[error("the trip log could not be read: {0}")]
    Unreadable(String),
}

/// A drive. Times are milliseconds since the Unix epoch.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TripRecord {
    pub start_ms: u64,
    pub end_ms: u64,
    pub distance_km: f64,
    /// Seconds with the car actually moving.
    pub moving_s: u64,
    pub avg_speed_kmh: f64,
    pub max_speed_kmh: f32,
    pub max_rpm: f32,
    pub end_odometer_km: Option<f64>,
    pub fuel_used_l: Option<f64>,
    pub consumption_l_per_100km: Option<f64>,
}

impl From<Trip> for TripRecord {
    fn from(t: Trip) -> Self {
        Self {
            avg_speed_kmh: t.avg_speed_kmh(),
            consumption_l_per_100km: t.consumption_l_per_100km(),
            start_ms: t.start_ms,
            end_ms: t.end_ms,
            distance_km: t.distance_km,
            moving_s: t.moving_s,
            max_speed_kmh: t.max_speed_kmh,
            max_rpm: t.max_rpm,
            end_odometer_km: t.end_odometer_km,
            fuel_used_l: t.fuel_used_l,
        }
    }
}

/// A detected fill-up.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RefuelRecord {
    pub time_ms: u64,
    pub added_l: f64,
    pub fuel_after_l: f64,
    pub odometer_km: Option<f64>,
}

impl From<&Refuel> for RefuelRecord {
    fn from(r: &Refuel) -> Self {
        Self { time_ms: r.time_ms, added_l: r.added_l, fuel_after_l: r.fuel_after_l, odometer_km: r.odometer_km }
    }
}

/// Sums over all finished trips.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct JournalTotals {
    pub trips: u32,
    pub distance_km: f64,
    pub moving_s: u64,
    pub fuel_used_l: f64,
}

impl From<Totals> for JournalTotals {
    fn from(t: Totals) -> Self {
        Self { trips: t.trips, distance_km: t.distance_km, moving_s: t.moving_s, fuel_used_l: t.fuel_used_l }
    }
}

/// The log of trips and refuels, fed from the live values. The app tells it when the
/// link comes and goes and passes every batch of readings with the current time
/// (milliseconds since the Unix epoch). See [`crate::journal`] for the rules.
#[derive(uniffi::Object)]
pub struct TripJournal {
    inner: Mutex<Journal>,
}

#[uniffi::export]
impl TripJournal {
    /// Opens (or creates) the journal file at `path`.
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Arc<Self>, JournalError> {
        let journal = Journal::open(std::path::Path::new(&path))
            .map_err(|e| JournalError::Unreadable(e.to_string()))?;
        Ok(Arc::new(Self { inner: Mutex::new(journal) }))
    }

    pub fn on_connected(&self, now_ms: u64) {
        self.inner.lock().unwrap().on_connected(now_ms);
    }

    pub fn on_disconnected(&self, now_ms: u64) {
        self.inner.lock().unwrap().on_disconnected(now_ms);
    }

    pub fn on_readings(&self, readings: Vec<Reading>, now_ms: u64) {
        let elements: Vec<dongle::DataElement> = readings
            .iter()
            .map(|r| dongle::DataElement {
                url: r.url,
                state: if r.available { dongle::state::OK } else { dongle::state::NODATA },
                value: r.value,
                precision: Some(r.precision),
            })
            .collect();
        self.inner.lock().unwrap().on_values(&elements, now_ms);
    }

    /// Finishes a paused trip whose grace period has passed; returns it if so.
    pub fn tick(&self, now_ms: u64) -> Option<TripRecord> {
        self.inner.lock().unwrap().tick(now_ms).map(TripRecord::from)
    }

    /// Finished trips, newest first.
    pub fn trips(&self) -> Vec<TripRecord> {
        self.inner.lock().unwrap().trips().iter().rev().cloned().map(TripRecord::from).collect()
    }

    /// The trip in progress (running or paused), if any.
    pub fn active_trip(&self) -> Option<TripRecord> {
        self.inner.lock().unwrap().active_trip().map(TripRecord::from)
    }

    /// Detected refuels, newest first.
    pub fn refuels(&self) -> Vec<RefuelRecord> {
        self.inner.lock().unwrap().refuels().iter().rev().map(RefuelRecord::from).collect()
    }

    pub fn totals(&self) -> JournalTotals {
        self.inner.lock().unwrap().totals().into()
    }
}
