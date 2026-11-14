//! Trip and refuel journal: turns the stream of live values into a log of drives
//! and fill-ups, the way the original app did (see
//! research/notes/original_app_features.md), without any vendor code.
//!
//! Rules:
//! - A trip **starts** when the vehicle speed first rises above zero.
//! - It is **paused** when the link to the dongle drops (ignition off, out of range).
//! - A paused trip **resumes** if the car moves again within
//!   [`TRIP_FINISH_AFTER_MS`]; otherwise it is **finished** at the moment it paused.
//! - A **refuel** is recorded when the first fuel reading of a new connection is more
//!   than [`REFUEL_MIN_LITRES`] above the last one seen.
//!
//! Time is supplied by the caller (milliseconds since the Unix epoch), which keeps the
//! logic deterministic and testable. State is kept in one JSON file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dongle::{state, DataElement};

/// A paused trip is finished after this long without the car moving again.
pub const TRIP_FINISH_AFTER_MS: u64 = 15 * 60 * 1000;
/// Smallest rise of the fuel level between two connections that counts as a refuel.
pub const REFUEL_MIN_LITRES: f64 = 1.0;
/// Trips shorter than this are dropped (shuffling the car on a driveway).
pub const MIN_TRIP_KM: f64 = 0.05;
/// Longest gap between two speed samples that still counts as continuous driving.
const MAX_SAMPLE_GAP_MS: u64 = 10_000;
/// How often an active trip is written to disk.
const SAVE_EVERY_MS: u64 = 60_000;

const URL_SPEED: u32 = 0x0002_0000;
const URL_FUEL_L: u32 = 0x0002_0002;
const URL_ODOMETER: u32 = 0x0002_0004;
const URL_RPM: u32 = 0x0004_0000;

/// Whether a trip is in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripState {
    /// No trip in progress.
    Idle,
    /// The car is connected and a trip is being recorded.
    Running,
    /// The link dropped during a trip; it resumes if the car moves again in time.
    Paused,
}

/// One finished (or still running) drive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trip {
    pub start_ms: u64,
    pub end_ms: u64,
    pub distance_km: f64,
    /// Seconds with the car actually moving.
    pub moving_s: u64,
    pub max_speed_kmh: f32,
    pub max_rpm: f32,
    pub end_odometer_km: Option<f64>,
    /// Fuel burnt, from the tank level; absent if the level was unknown or the car
    /// was refuelled during the trip.
    pub fuel_used_l: Option<f64>,
}

impl Trip {
    /// Average speed over the time the car was moving.
    pub fn avg_speed_kmh(&self) -> f64 {
        if self.moving_s == 0 { 0.0 } else { self.distance_km / (self.moving_s as f64 / 3600.0) }
    }
    /// Average consumption in l/100 km, when both fuel and a meaningful distance are known.
    pub fn consumption_l_per_100km(&self) -> Option<f64> {
        let fuel = self.fuel_used_l?;
        (self.distance_km >= 1.0 && fuel > 0.0).then(|| fuel / self.distance_km * 100.0)
    }
}

/// One detected fill-up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refuel {
    pub time_ms: u64,
    pub added_l: f64,
    pub fuel_after_l: f64,
    pub odometer_km: Option<f64>,
}

/// Sums over all finished trips.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Totals {
    pub trips: u32,
    pub distance_km: f64,
    pub moving_s: u64,
    pub fuel_used_l: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Active {
    start_ms: u64,
    end_ms: u64,
    /// Distance from integrating speed over time.
    integrated_km: f64,
    moving_s_x1000: u64,
    max_speed_kmh: f32,
    max_rpm: f32,
    start_odometer_km: Option<f64>,
    end_odometer_km: Option<f64>,
    start_fuel_l: Option<f64>,
    end_fuel_l: Option<f64>,
    refuelled: bool,
    paused_at_ms: Option<u64>,
    #[serde(skip)]
    last_sample: Option<(u64, f32)>, // (time, speed) of the previous speed sample
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    trips: Vec<Trip>,
    #[serde(default)]
    refuels: Vec<Refuel>,
    #[serde(default)]
    active: Option<Active>,
    #[serde(default)]
    last_fuel_l: Option<f64>,
    #[serde(default)]
    last_odometer_km: Option<f64>,
}

pub struct Journal {
    path: Option<PathBuf>,
    data: Data,
    /// Whether this connection's first fuel reading has been compared yet.
    fuel_checked: bool,
    last_save_ms: u64,
}

impl Journal {
    /// A journal that lives only in memory.
    pub fn in_memory() -> Self {
        Self { path: None, data: Data::default(), fuel_checked: false, last_save_ms: 0 }
    }

    /// Opens (or creates) the journal stored in `path`. An unreadable file is an
    /// error rather than silently starting empty, so a log is never overwritten.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let data = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e),
        };
        Ok(Self { path: Some(path.to_path_buf()), data, fuel_checked: false, last_save_ms: 0 })
    }

    /// Finished trips, oldest first.
    pub fn trips(&self) -> &[Trip] {
        &self.data.trips
    }

    pub fn refuels(&self) -> &[Refuel] {
        &self.data.refuels
    }

    /// The trip in progress (running or paused), as it stands now.
    pub fn active_trip(&self) -> Option<Trip> {
        self.data.active.as_ref().map(summarize)
    }

    pub fn trip_state(&self) -> TripState {
        match &self.data.active {
            None => TripState::Idle,
            Some(a) if a.paused_at_ms.is_some() => TripState::Paused,
            Some(_) => TripState::Running,
        }
    }

    pub fn totals(&self) -> Totals {
        self.data.trips.iter().fold(Totals::default(), |mut t, trip| {
            t.trips += 1;
            t.distance_km += trip.distance_km;
            t.moving_s += trip.moving_s;
            t.fuel_used_l += trip.fuel_used_l.unwrap_or(0.0);
            t
        })
    }

    /// A connection to the dongle was established.
    pub fn on_connected(&mut self, now_ms: u64) {
        self.fuel_checked = false;
        self.tick(now_ms);
    }

    /// The connection was lost or closed: a running trip is paused.
    pub fn on_disconnected(&mut self, _now_ms: u64) {
        if let Some(a) = &mut self.data.active {
            if a.paused_at_ms.is_none() {
                a.paused_at_ms = Some(a.end_ms);
                a.last_sample = None;
            }
        }
        self.save();
    }

    /// Finishes a paused trip whose grace period is over. Call now and then while
    /// disconnected. Returns the trip if one was just finished and kept.
    pub fn tick(&mut self, now_ms: u64) -> Option<Trip> {
        let paused_at = self.data.active.as_ref()?.paused_at_ms?;
        if now_ms.saturating_sub(paused_at) < TRIP_FINISH_AFTER_MS {
            return None;
        }
        self.finish()
    }

    /// Feeds the values of one message from the dongle.
    pub fn on_values(&mut self, elements: &[DataElement], now_ms: u64) {
        let get = |url| elements.iter().find(|e| e.url == url && e.state == state::OK).map(|e| e.value);

        if let Some(odo) = get(URL_ODOMETER) {
            self.data.last_odometer_km = Some(odo as f64);
            if let Some(a) = &mut self.data.active {
                if a.paused_at_ms.is_none() {
                    a.start_odometer_km.get_or_insert(odo as f64);
                    a.end_odometer_km = Some(odo as f64);
                }
            }
        }
        if let Some(fuel) = get(URL_FUEL_L) {
            self.on_fuel(fuel as f64, now_ms);
        }
        if let Some(rpm) = get(URL_RPM) {
            if let Some(a) = self.data.active.as_mut().filter(|a| a.paused_at_ms.is_none()) {
                a.max_rpm = a.max_rpm.max(rpm);
            }
        }
        if let Some(speed) = get(URL_SPEED) {
            self.on_speed(speed, now_ms);
        }
        if self.data.active.is_some() && now_ms.saturating_sub(self.last_save_ms) >= SAVE_EVERY_MS {
            self.save_at(now_ms);
        }
    }

    fn on_fuel(&mut self, fuel: f64, now_ms: u64) {
        if !self.fuel_checked {
            self.fuel_checked = true;
            if let Some(last) = self.data.last_fuel_l {
                let added = fuel - last;
                if added > REFUEL_MIN_LITRES {
                    self.data.refuels.push(Refuel {
                        time_ms: now_ms,
                        added_l: added,
                        fuel_after_l: fuel,
                        odometer_km: self.data.last_odometer_km,
                    });
                    if let Some(a) = &mut self.data.active {
                        a.refuelled = true;
                    }
                    self.save_at(now_ms);
                }
            }
        }
        self.data.last_fuel_l = Some(fuel);
        if let Some(a) = self.data.active.as_mut().filter(|a| a.paused_at_ms.is_none()) {
            a.start_fuel_l.get_or_insert(fuel);
            a.end_fuel_l = Some(fuel);
        }
    }

    fn on_speed(&mut self, speed: f32, now_ms: u64) {
        if speed > 0.0 {
            // a paused trip either resumes or, if it sat too long, ends first
            self.tick(now_ms);
            match &mut self.data.active {
                Some(a) => a.paused_at_ms = None,
                None => {
                    self.data.active = Some(Active {
                        start_ms: now_ms,
                        end_ms: now_ms,
                        integrated_km: 0.0,
                        moving_s_x1000: 0,
                        max_speed_kmh: 0.0,
                        max_rpm: 0.0,
                        start_odometer_km: self.data.last_odometer_km,
                        end_odometer_km: self.data.last_odometer_km,
                        start_fuel_l: self.data.last_fuel_l,
                        end_fuel_l: self.data.last_fuel_l,
                        refuelled: false,
                        paused_at_ms: None,
                        last_sample: None,
                    });
                    self.save_at(now_ms);
                }
            }
        }
        let Some(a) = self.data.active.as_mut().filter(|a| a.paused_at_ms.is_none()) else { return };
        if let Some((t0, v0)) = a.last_sample {
            let dt = now_ms.saturating_sub(t0);
            if dt <= MAX_SAMPLE_GAP_MS {
                let mean = (v0 + speed) as f64 / 2.0;
                a.integrated_km += mean * dt as f64 / 3_600_000.0;
                if mean > 0.0 {
                    a.moving_s_x1000 += dt;
                }
            }
        }
        a.last_sample = Some((now_ms, speed));
        a.max_speed_kmh = a.max_speed_kmh.max(speed);
        a.end_ms = now_ms;
    }

    fn finish(&mut self) -> Option<Trip> {
        let a = self.data.active.take()?;
        let trip = summarize(&a);
        let kept = (trip.distance_km >= MIN_TRIP_KM).then(|| {
            self.data.trips.push(trip.clone());
            trip
        });
        self.save();
        kept
    }

    /// Writes the journal to its file now (it is otherwise saved on every change of
    /// state and once a minute during a trip).
    pub fn flush(&self) {
        self.save();
    }

    fn save_at(&mut self, now_ms: u64) {
        self.last_save_ms = now_ms;
        self.save();
    }

    /// Best effort: the journal keeps working in memory if the disk write fails.
    fn save(&self) {
        let Some(path) = &self.path else { return };
        let Ok(bytes) = serde_json::to_vec(&self.data) else { return };
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

fn summarize(a: &Active) -> Trip {
    // Two sources of distance: the odometer (authoritative, but whole kilometres) and
    // the speed integral (fine-grained, but it misses gaps and inherits the
    // speedometer's few-percent error).
    let odometer_delta = match (a.start_odometer_km, a.end_odometer_km) {
        (Some(s), Some(e)) if e >= s => Some(e - s),
        _ => None,
    };
    let integral = a.integrated_km;
    let distance_km = match odometer_delta {
        // they agree to within the odometer's resolution: keep the finer value
        Some(d) if (d - integral).abs() <= 1.5 => integral,
        // the odometer fell far behind the integral: it is not being updated
        Some(d) if d < integral - (0.1 * integral).max(1.5) => integral,
        // otherwise the integral missed some driving or drifted: trust the odometer
        Some(d) => d,
        None => integral,
    };
    let fuel_used_l = match (a.start_fuel_l, a.end_fuel_l) {
        (Some(start), Some(end)) if !a.refuelled && start >= end => Some(start - end),
        _ => None,
    };
    Trip {
        start_ms: a.start_ms,
        end_ms: a.paused_at_ms.unwrap_or(a.end_ms),
        distance_km,
        moving_s: a.moving_s_x1000 / 1000,
        max_speed_kmh: a.max_speed_kmh,
        max_rpm: a.max_rpm,
        end_odometer_km: a.end_odometer_km,
        fuel_used_l,
    }
}
