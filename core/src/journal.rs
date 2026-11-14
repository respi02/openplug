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

use std::collections::BTreeMap;
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
/// Route points are at least this far apart in time and distance.
pub const TRACK_MIN_INTERVAL_MS: u64 = 2_000;
pub const TRACK_MIN_DISTANCE_M: f64 = 10.0;
/// A fix older than this when the link drops says nothing about where the car is.
const PARKED_FIX_MAX_AGE_MS: u64 = 120_000;
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

/// A place on Earth, in degrees (WGS 84).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub lat: f64,
    pub lon: f64,
}

impl Position {
    /// Great-circle distance in metres.
    pub fn distance_m(&self, other: &Position) -> f64 {
        let (p1, p2) = (self.lat.to_radians(), other.lat.to_radians());
        let (dp, dl) = (p2 - p1, (other.lon - self.lon).to_radians());
        let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
        6_371_000.0 * 2.0 * a.sqrt().asin()
    }
}

/// One point of a trip's route: `[time_ms, lat, lon, speed_kmh]` in the track file.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrackPoint(pub u64, pub f64, pub f64, pub f32);

/// A position with the time it was taken.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    pub pos: Position,
    pub time_ms: u64,
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
    /// Where the trip began and ended, when the phone's position was being recorded.
    #[serde(default)]
    pub start_pos: Option<Position>,
    #[serde(default)]
    pub end_pos: Option<Position>,
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
    /// Where the car was parked when it was refuelled, if known.
    #[serde(default)]
    pub pos: Option<Position>,
}

/// The last reading of a data object, kept so the car's state can be shown while it
/// is out of reach.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LastValue {
    pub value: f32,
    pub precision: Option<u32>,
    pub time_ms: u64,
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
    #[serde(default)]
    start_pos: Option<Position>,
    #[serde(default)]
    end_pos: Option<Position>,
    /// The route so far; kept in its own file, not in the journal.
    #[serde(skip)]
    track: Vec<TrackPoint>,
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
    /// Latest good reading of every data object seen, by url.
    #[serde(default)]
    last_values: BTreeMap<u32, LastValue>,
    /// The latest position fix.
    #[serde(default)]
    last_fix: Option<Fix>,
    /// Where the car was when the link last dropped.
    #[serde(default)]
    parked: Option<Fix>,
}

pub struct Journal {
    path: Option<PathBuf>,
    data: Data,
    /// Whether this connection's first fuel reading has been compared yet.
    fuel_checked: bool,
    last_save_ms: u64,
    /// Routes of finished trips when there is no file to keep them in (tests).
    memory_tracks: BTreeMap<u64, Vec<TrackPoint>>,
}

impl Journal {
    /// A journal that lives only in memory.
    pub fn in_memory() -> Self {
        Self { path: None, data: Data::default(), fuel_checked: false, last_save_ms: 0, memory_tracks: BTreeMap::new() }
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
        let mut journal =
            Self { path: Some(path.to_path_buf()), data, fuel_checked: false, last_save_ms: 0, memory_tracks: BTreeMap::new() };
        if let Some(start) = journal.data.active.as_ref().map(|a| a.start_ms) {
            let track = journal.read_track(start);
            if let Some(a) = &mut journal.data.active {
                a.track = track;
            }
        }
        Ok(journal)
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

    /// The latest good reading of every data object seen so far, by url. Survives
    /// restarts, so the car's last known state can be shown while disconnected.
    pub fn last_values(&self) -> &BTreeMap<u32, LastValue> {
        &self.data.last_values
    }

    /// The finished trips as CSV (one row per trip, times in UTC, decimal points).
    pub fn export_csv(&self) -> String {
        let mut out = String::from(
            "start_utc,end_utc,distance_km,moving_min,avg_speed_kmh,max_speed_kmh,max_rpm,fuel_used_l,l_per_100km,end_odometer_km\n",
        );
        let opt = |v: Option<f64>, digits: usize| v.map(|v| format!("{v:.digits$}")).unwrap_or_default();
        for t in &self.data.trips {
            out += &format!(
                "{},{},{:.2},{:.1},{:.1},{:.0},{:.0},{},{},{}\n",
                iso_utc(t.start_ms),
                iso_utc(t.end_ms),
                t.distance_km,
                t.moving_s as f64 / 60.0,
                t.avg_speed_kmh(),
                t.max_speed_kmh,
                t.max_rpm,
                opt(t.fuel_used_l, 2),
                opt(t.consumption_l_per_100km(), 1),
                opt(t.end_odometer_km, 0),
            );
        }
        out
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
    pub fn on_disconnected(&mut self, now_ms: u64) {
        // the car stays where the phone last was while connected
        self.data.parked = self.data.last_fix.filter(|f| now_ms.saturating_sub(f.time_ms) <= PARKED_FIX_MAX_AGE_MS);
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
        for e in elements.iter().filter(|e| e.state == state::OK) {
            self.data.last_values.insert(e.url, LastValue { value: e.value, precision: e.precision, time_ms: now_ms });
        }

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
                        pos: self.data.parked.map(|f| f.pos),
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
                        start_pos: None,
                        end_pos: None,
                        track: Vec::new(),
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
        if kept.is_some() {
            self.write_track(a.start_ms, &a.track);
        } else if let Some(file) = self.track_file(a.start_ms) {
            let _ = std::fs::remove_file(file);
        }
        self.save();
        kept
    }

    /// Feeds a position fix from the phone. While a trip is running it extends the
    /// trip's route; the latest fix also becomes the parking position when the link
    /// drops. Fixes are thinned to one per [`TRACK_MIN_INTERVAL_MS`] and
    /// [`TRACK_MIN_DISTANCE_M`].
    pub fn on_position(&mut self, lat: f64, lon: f64, now_ms: u64) {
        if !(lat.is_finite() && lon.is_finite()) || lat.abs() > 90.0 || lon.abs() > 180.0 {
            return;
        }
        let round = |v: f64| (v * 1e6).round() / 1e6; // about 0.1 m; keeps the files small
        let pos = Position { lat: round(lat), lon: round(lon) };
        self.data.last_fix = Some(Fix { pos, time_ms: now_ms });
        let Some(a) = self.data.active.as_mut().filter(|a| a.paused_at_ms.is_none()) else { return };
        a.start_pos.get_or_insert(pos);
        a.end_pos = Some(pos);
        let far_enough = a.track.last().is_none_or(|last| {
            now_ms.saturating_sub(last.0) >= TRACK_MIN_INTERVAL_MS
                && Position { lat: last.1, lon: last.2 }.distance_m(&pos) >= TRACK_MIN_DISTANCE_M
        });
        if far_enough {
            let speed = a.last_sample.map_or(0.0, |(_, v)| v);
            a.track.push(TrackPoint(now_ms, pos.lat, pos.lon, speed));
        }
    }

    /// The route of the trip that started at `start_ms` (finished or in progress);
    /// empty when no position was recorded.
    pub fn track(&self, start_ms: u64) -> Vec<TrackPoint> {
        match &self.data.active {
            Some(a) if a.start_ms == start_ms => a.track.clone(),
            _ => self.read_track(start_ms),
        }
    }

    /// Where the car was when the link last dropped, with the time of that fix.
    pub fn parked(&self) -> Option<Fix> {
        self.data.parked
    }

    /// Routes live next to the journal, one small file per trip, so the journal
    /// itself stays quick to load and rewrite.
    fn track_file(&self, start_ms: u64) -> Option<PathBuf> {
        let path = self.path.as_ref()?;
        Some(path.with_extension("tracks").join(format!("{start_ms}.json")))
    }

    fn read_track(&self, start_ms: u64) -> Vec<TrackPoint> {
        match self.track_file(start_ms) {
            Some(file) => std::fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default(),
            None => self.memory_tracks.get(&start_ms).cloned().unwrap_or_default(),
        }
    }

    fn write_track(&mut self, start_ms: u64, track: &[TrackPoint]) {
        if track.is_empty() {
            return;
        }
        match self.track_file(start_ms) {
            Some(file) => {
                if let (Some(dir), Ok(bytes)) = (file.parent(), serde_json::to_vec(track)) {
                    let _ = std::fs::create_dir_all(dir);
                    let _ = std::fs::write(file, bytes);
                }
            }
            None => {
                self.memory_tracks.insert(start_ms, track.to_vec());
            }
        }
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
        if let Some(a) = self.data.active.as_ref().filter(|a| !a.track.is_empty()) {
            if let (Some(file), Ok(bytes)) = (self.track_file(a.start_ms), serde_json::to_vec(&a.track)) {
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(file, bytes);
            }
        }
        let Ok(bytes) = serde_json::to_vec(&self.data) else { return };
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a time in milliseconds since the Unix epoch.
pub fn iso_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // civil-from-days (H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
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
        start_pos: a.start_pos,
        end_pos: a.end_pos,
    }
}
