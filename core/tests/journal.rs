//! The trip and refuel journal, driven with synthetic value streams.

use openplug_core::dongle::{state, DataElement};
use openplug_core::journal::{Journal, TRIP_FINISH_AFTER_MS};

const SPEED: u32 = 0x0002_0000;
const FUEL: u32 = 0x0002_0002;
const ODO: u32 = 0x0002_0004;
const RPM: u32 = 0x0004_0000;

fn el(url: u32, value: f32) -> DataElement {
    DataElement { url, state: state::OK, value, precision: Some(1) }
}

/// Drives at `kmh` for `seconds`, one speed sample per second, starting at `t`.
fn drive(j: &mut Journal, t: &mut u64, kmh: f32, seconds: u64) {
    for _ in 0..seconds {
        j.on_values(&[el(SPEED, kmh), el(RPM, 2000.0)], *t);
        *t += 1000;
    }
}

#[test]
fn a_drive_becomes_a_trip_after_the_grace_period() {
    let mut j = Journal::in_memory();
    let mut t = 1_000_000;
    j.on_connected(t);
    j.on_values(&[el(ODO, 1000.0), el(FUEL, 40.0)], t);
    j.on_values(&[el(SPEED, 0.0)], t); // standing still starts nothing
    assert!(j.active_trip().is_none());

    drive(&mut j, &mut t, 72.0, 600); // 10 min at 72 km/h = 12 km
    j.on_values(&[el(ODO, 1012.0), el(FUEL, 39.2)], t);
    j.on_disconnected(t);
    let paused_at = t - 1000; // the last speed sample
    assert!(j.trips().is_empty(), "paused, not finished");
    assert!(j.tick(paused_at + TRIP_FINISH_AFTER_MS - 1).is_none());

    let trip = j.tick(paused_at + TRIP_FINISH_AFTER_MS).expect("finished");
    assert_eq!(j.trips(), std::slice::from_ref(&trip));
    assert!((trip.distance_km - 12.0).abs() < 0.05, "{}", trip.distance_km);
    assert_eq!(trip.end_ms, paused_at, "ends when it paused, not when it timed out");
    assert_eq!(trip.max_speed_kmh, 72.0);
    assert_eq!(trip.max_rpm, 2000.0);
    assert!((trip.fuel_used_l.unwrap() - 0.8).abs() < 1e-3);
    assert!((trip.avg_speed_kmh() - 72.0).abs() < 0.5);
    assert!((trip.consumption_l_per_100km().unwrap() - 6.67).abs() < 0.05);
    assert_eq!(j.totals().trips, 1);
}

#[test]
fn a_short_stop_resumes_the_same_trip() {
    let mut j = Journal::in_memory();
    let mut t = 5_000_000;
    j.on_connected(t);
    drive(&mut j, &mut t, 60.0, 300); // 5 km
    j.on_disconnected(t);
    t += 5 * 60 * 1000; // five minutes at a shop
    j.on_connected(t);
    drive(&mut j, &mut t, 60.0, 300); // 5 km more
    j.on_disconnected(t);
    let trip = j.tick(t + TRIP_FINISH_AFTER_MS).expect("finished");
    assert_eq!(j.trips().len(), 1);
    assert!((trip.distance_km - 10.0).abs() < 0.05, "{}", trip.distance_km);
    assert_eq!(trip.moving_s, 598, "the stop does not count as moving");
}

#[test]
fn a_long_stop_splits_trips() {
    let mut j = Journal::in_memory();
    let mut t = 9_000_000;
    j.on_connected(t);
    drive(&mut j, &mut t, 50.0, 120);
    j.on_disconnected(t);
    t += TRIP_FINISH_AFTER_MS + 60_000;
    j.on_connected(t); // finishes the first trip
    assert_eq!(j.trips().len(), 1);
    drive(&mut j, &mut t, 50.0, 120);
    j.on_disconnected(t);
    j.tick(t + TRIP_FINISH_AFTER_MS);
    assert_eq!(j.trips().len(), 2);
}

#[test]
fn the_odometer_wins_when_samples_are_missing() {
    let mut j = Journal::in_memory();
    let mut t = 1_000;
    j.on_connected(t);
    j.on_values(&[el(ODO, 500.0)], t);
    drive(&mut j, &mut t, 90.0, 10);
    t += 20 * 60 * 1000; // a long gap with no samples, still connected
    drive(&mut j, &mut t, 90.0, 10);
    j.on_values(&[el(ODO, 530.0)], t);
    j.on_disconnected(t);
    let trip = j.tick(t + TRIP_FINISH_AFTER_MS).unwrap();
    assert_eq!(trip.distance_km, 30.0);
}

#[test]
fn a_stuck_odometer_does_not_zero_the_distance() {
    let mut j = Journal::in_memory();
    let mut t = 1_000;
    j.on_connected(t);
    j.on_values(&[el(ODO, 500.0)], t);
    drive(&mut j, &mut t, 60.0, 600); // 10 km
    j.on_values(&[el(ODO, 500.0)], t); // never moved
    j.on_disconnected(t);
    let trip = j.tick(t + TRIP_FINISH_AFTER_MS).unwrap();
    assert!((trip.distance_km - 10.0).abs() < 0.05, "{}", trip.distance_km);
}

#[test]
fn a_refuel_is_detected_on_the_next_connection_only() {
    let mut j = Journal::in_memory();
    j.on_connected(1_000);
    j.on_values(&[el(FUEL, 12.0), el(ODO, 700.0)], 1_000);
    j.on_values(&[el(FUEL, 14.5)], 2_000); // a jump within one connection is sloshing
    assert!(j.refuels().is_empty());
    j.on_disconnected(3_000);

    j.on_connected(600_000);
    j.on_values(&[el(FUEL, 48.5)], 600_000);
    let r = &j.refuels()[0];
    assert_eq!((r.added_l, r.fuel_after_l, r.odometer_km), (34.0, 48.5, Some(700.0)));

    j.on_disconnected(601_000);
    j.on_connected(900_000);
    j.on_values(&[el(FUEL, 48.9)], 900_000); // within tolerance
    assert_eq!(j.refuels().len(), 1);
}

#[test]
fn tiny_movements_are_not_trips() {
    let mut j = Journal::in_memory();
    let mut t = 1_000;
    j.on_connected(t);
    drive(&mut j, &mut t, 5.0, 10); // ~14 m
    j.on_disconnected(t);
    assert!(j.tick(t + TRIP_FINISH_AFTER_MS).is_none());
    assert!(j.trips().is_empty() && j.active_trip().is_none());
}

#[test]
fn the_journal_survives_a_restart_mid_trip() {
    let dir = std::env::temp_dir().join(format!("openplug-journal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("journal.json");
    let _ = std::fs::remove_file(&path);

    let mut t = 1_000;
    {
        let mut j = Journal::open(&path).unwrap();
        j.on_connected(t);
        drive(&mut j, &mut t, 60.0, 120); // 2 km
        j.on_disconnected(t); // saves the paused trip
    }
    let mut j = Journal::open(&path).unwrap();
    assert!((j.active_trip().unwrap().distance_km - 2.0).abs() < 0.05);
    j.on_connected(t + 60_000);
    let mut t2 = t + 60_000;
    drive(&mut j, &mut t2, 60.0, 60); // 1 km more, same trip
    j.on_disconnected(t2);
    j.tick(t2 + TRIP_FINISH_AFTER_MS);
    drop(j);

    let j = Journal::open(&path).unwrap();
    assert_eq!(j.trips().len(), 1);
    assert!((j.trips()[0].distance_km - 3.0).abs() < 0.05);

    std::fs::write(&path, b"not json").unwrap();
    assert!(Journal::open(&path).is_err(), "a damaged log is reported, not overwritten");
    std::fs::remove_dir_all(&dir).unwrap();
}
