//! Writes a demo trip log: two weeks of made-up drives around Brno, with routes, a
//! refuel, addresses and filed trips, for showing the apps without a car.
//!
//! ```sh
//! cargo run --example demo_journal -- journal-demo.json
//! ```
//!
//! Its last values match the fake dongle's (odometer 123 456 km, 37.4 l in the tank),
//! so connecting to `fake_dongle` afterwards carries on from it without a jump. The
//! apps' debug builds open `journal-demo.json` while connected to the fake dongle.

use openplug_core::dongle::{state, DataElement};
use openplug_core::journal::{CarProfile, Journal, TripKind, TRIP_FINISH_AFTER_MS};
use std::path::PathBuf;

const SPEED: u32 = 0x0002_0000;
const FUEL: u32 = 0x0002_0002;
const ODO: u32 = 0x0002_0004;
const RANGE: u32 = 0x0002_0005;
const RPM: u32 = 0x0004_0000;
const COOLANT: u32 = 0x0004_0001;
const OIL: u32 = 0x0002_0030;
const BATTERY: u32 = 0x0005_0001;
const DAYS_SINCE_SERVICE: u32 = 0x0007_0009;
const KM_SINCE_SERVICE: u32 = 0x0007_0012;

/// Where the fake dongle stands, so a demo session continues from here.
const END_ODOMETER: f64 = 123_456.0;
const END_FUEL: f64 = 37.4;
const REFUEL_L: f64 = 30.0;
const L_PER_KM: f64 = 0.062;

const MINUTE: u64 = 60_000;
const DAY: u64 = 24 * 60 * MINUTE;

type Path = &'static [(f64, f64)];

// Rough routes along the main roads (lat, lon).
const HOME_OFFICE: Path = &[
    (49.2086, 16.5795), (49.2071, 16.5868), (49.2045, 16.5960), (49.1985, 16.5985),
    (49.1935, 16.6040), (49.1880, 16.6085), (49.1820, 16.6150), (49.1795, 16.6195),
];
const HOME_MALL: Path = &[
    (49.2086, 16.5795), (49.2045, 16.5960), (49.1935, 16.6040), (49.1820, 16.6150),
    (49.1700, 16.6120), (49.1600, 16.6100), (49.1400, 16.6180), (49.1290, 16.6355),
];
const OFFICE_MIKULOV: Path = &[
    (49.1795, 16.6195), (49.1600, 16.6100), (49.1300, 16.6050), (49.0900, 16.6000),
    (49.0300, 16.5600), (48.9800, 16.5300), (48.9100, 16.5800), (48.8500, 16.6200),
    (48.8050, 16.6370),
];
const OFFICE_LESNA: Path = &[
    (49.1795, 16.6195), (49.1880, 16.6085), (49.1950, 16.6100), (49.2050, 16.6180),
    (49.2150, 16.6220), (49.2240, 16.6260),
];
const HOME_LESNA: Path = &[
    (49.2086, 16.5795), (49.2120, 16.5900), (49.2150, 16.6050), (49.2200, 16.6180),
    (49.2240, 16.6260),
];

struct Drive {
    day: u64,
    hour: u64,
    minute: u64,
    path: Path,
    reverse: bool,
    /// Cruising speed on the open road (city parts are slower).
    cruise_kmh: f64,
    from: &'static str,
    to: &'static str,
    kind: TripKind,
    note: &'static str,
    refuel_before: bool,
}

const fn drive(day: u64, hour: u64, minute: u64, path: Path, reverse: bool, from: &'static str, to: &'static str) -> Drive {
    Drive { day, hour, minute, path, reverse, cruise_kmh: 50.0, from, to, kind: TripKind::Unset, note: "", refuel_before: false }
}

const HOME: &str = "Minská, Brno";
const OFFICE: &str = "Holandská, Brno";
const MALL: &str = "U Dálnice, Modřice";
const MIKULOV: &str = "Náměstí, Mikulov";
const LESNA: &str = "Haškova, Brno";

fn plan() -> Vec<Drive> {
    vec![
        drive(13, 7, 48, HOME_OFFICE, false, HOME, OFFICE),
        drive(13, 17, 21, HOME_OFFICE, true, OFFICE, HOME),
        drive(11, 8, 31, HOME_OFFICE, false, HOME, OFFICE),
        Drive {
            cruise_kmh: 95.0,
            kind: TripKind::Business,
            note: "Client visit, Mikulov",
            refuel_before: true,
            ..drive(11, 9, 5, OFFICE_MIKULOV, false, OFFICE, MIKULOV)
        },
        Drive {
            cruise_kmh: 95.0,
            kind: TripKind::Business,
            note: "Client visit, Mikulov",
            ..drive(11, 15, 32, OFFICE_MIKULOV, true, MIKULOV, OFFICE)
        },
        drive(11, 17, 6, HOME_OFFICE, true, OFFICE, HOME),
        Drive { kind: TripKind::Private, note: "Shopping", ..drive(9, 18, 2, HOME_MALL, false, HOME, MALL) },
        Drive { kind: TripKind::Private, ..drive(9, 19, 34, HOME_MALL, true, MALL, HOME) },
        drive(6, 7, 55, HOME_OFFICE, false, HOME, OFFICE),
        Drive { kind: TripKind::Business, note: "Meeting", ..drive(6, 12, 10, OFFICE_LESNA, false, OFFICE, LESNA) },
        Drive { kind: TripKind::Business, note: "Meeting", ..drive(6, 13, 25, OFFICE_LESNA, true, LESNA, OFFICE) },
        drive(6, 17, 40, HOME_OFFICE, true, OFFICE, HOME),
        Drive { kind: TripKind::Private, ..drive(2, 10, 0, HOME_LESNA, false, HOME, LESNA) },
        Drive { kind: TripKind::Private, ..drive(2, 11, 45, HOME_LESNA, true, LESNA, HOME) },
        drive(1, 7, 52, HOME_OFFICE, false, HOME, OFFICE),
        drive(1, 17, 15, HOME_OFFICE, true, OFFICE, HOME),
    ]
}

fn el(url: u32, value: f64, precision: u32) -> DataElement {
    DataElement { url, state: state::OK, value: value as f32, precision: Some(precision) }
}

fn km_between(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().asin()
}

fn points(d: &Drive) -> Vec<(f64, f64)> {
    let mut p = d.path.to_vec();
    if d.reverse {
        p.reverse();
    }
    p
}

fn length_km(p: &[(f64, f64)]) -> f64 {
    p.windows(2).map(|w| km_between(w[0], w[1])).sum()
}

/// Days back from today, at a time of day in Central European Time (UTC+1; close
/// enough for a demo).
fn start_ms(now: u64, d: &Drive) -> u64 {
    let today = now - now % DAY;
    today - d.day * DAY + (d.hour.saturating_sub(1)) * 60 * MINUTE + d.minute * MINUTE
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "journal-demo.json".into()));
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_dir_all(out.with_extension("tracks"));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;

    let plan = plan();
    let total_km: f64 = plan.iter().map(|d| length_km(&points(d))).sum();
    let mut odometer = END_ODOMETER - total_km;
    let mut fuel = END_FUEL + total_km * L_PER_KM - REFUEL_L;

    let mut j = Journal::open(&out).expect("open the demo journal");
    j.set_profile(CarProfile { tank_l: Some(50.0), service_every_km: Some(15_000.0), service_every_days: Some(730) });

    let mut last_end = 0;
    for d in &plan {
        let mut t = start_ms(now, d);
        if last_end > 0 {
            j.tick(last_end + TRIP_FINISH_AFTER_MS); // the previous drive is over
        }
        let path = points(d);
        j.on_connected(t);
        if d.refuel_before {
            fuel += REFUEL_L;
        }
        let tank = |fuel: f64, odo: f64| vec![el(ODO, odo.round(), 0), el(FUEL, fuel, 1), el(RANGE, fuel / L_PER_KM, 0)];
        j.on_position(path[0].0, path[0].1, t);
        j.on_values(&tank(fuel, odometer), t);
        for _ in 0..5 {
            j.on_values(&[el(SPEED, 0.0, 0), el(RPM, 800.0, 0)], t);
            t += 1000;
        }

        // one sample a second along the path; slower in town at both ends
        let length = length_km(&path);
        let (mut leg, mut along_leg, mut done_km, mut second) = (0usize, 0.0f64, 0.0f64, 0u64);
        while leg + 1 < path.len() {
            let town = done_km < 2.5 || length - done_km < 2.5;
            let base = if town { d.cruise_kmh.min(48.0) } else { d.cruise_kmh };
            let kmh = (base + 6.0 * ((second as f64) / 23.0).sin()).max(5.0);
            let step = kmh / 3600.0;
            along_leg += step;
            done_km += step;
            while leg + 1 < path.len() && along_leg >= km_between(path[leg], path[leg + 1]) {
                along_leg -= km_between(path[leg], path[leg + 1]);
                leg += 1;
            }
            let pos = if leg + 1 < path.len() {
                let f = along_leg / km_between(path[leg], path[leg + 1]).max(1e-9);
                (path[leg].0 + (path[leg + 1].0 - path[leg].0) * f, path[leg].1 + (path[leg + 1].1 - path[leg].1) * f)
            } else {
                path[path.len() - 1]
            };
            j.on_values(&[el(SPEED, kmh.round(), 0), el(RPM, (kmh * 38.0).max(900.0).round(), 0)], t);
            if second % 3 == 0 {
                j.on_position(pos.0, pos.1, t);
            }
            if second % 60 == 0 {
                j.on_values(&tank(fuel - done_km * L_PER_KM, odometer + done_km), t);
            }
            t += 1000;
            second += 1;
        }
        odometer += length;
        fuel -= length * L_PER_KM;
        for _ in 0..5 {
            j.on_values(&[el(SPEED, 0.0, 0), el(RPM, 800.0, 0)], t);
            t += 1000;
        }
        let end = path[path.len() - 1];
        j.on_position(end.0, end.1, t);
        let mut last = tank(fuel, odometer);
        last.extend([
            el(COOLANT, 88.0, 0),
            el(OIL, 92.0, 0),
            el(BATTERY, 14.2, 1),
            el(KM_SINCE_SERVICE, 11_800.0, 0),
            el(DAYS_SINCE_SERVICE, 600.0, 0),
        ]);
        j.on_values(&last, t);
        j.on_disconnected(t);
        last_end = t;
    }
    j.tick(now.max(last_end + TRIP_FINISH_AFTER_MS));

    for (d, trip) in plan.iter().zip(j.trips().to_vec()) {
        j.set_trip_addresses(trip.start_ms, Some(d.from.into()), Some(d.to.into()));
        if d.kind != TripKind::Unset || !d.note.is_empty() {
            j.set_trip_details(trip.start_ms, d.kind, d.note);
        }
    }
    j.flush();
    let totals = j.totals();
    println!(
        "wrote {}: {} trips, {:.1} km, {} refuel(s), odometer {:.0} km, {:.1} l in the tank",
        out.display(),
        totals.trips,
        totals.distance_km,
        j.refuels().len(),
        odometer,
        fuel
    );
}
