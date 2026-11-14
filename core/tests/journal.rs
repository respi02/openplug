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

#[test]
fn last_values_are_remembered_and_trips_export_as_csv() {
    let path = std::env::temp_dir().join(format!("openplug-status-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut t = 1_763_100_000_000;
    {
        let mut j = Journal::open(&path).unwrap();
        j.on_connected(t);
        j.on_values(&[el(ODO, 1000.0), el(FUEL, 40.0)], t);
        j.on_values(&[DataElement { url: RPM, state: state::NODATA, value: 0.0, precision: None }], t);
        drive(&mut j, &mut t, 60.0, 600);
        j.on_values(&[el(FUEL, 39.5)], t);
        j.on_disconnected(t);
        j.tick(t + TRIP_FINISH_AFTER_MS);
    }
    let j = Journal::open(&path).unwrap(); // survives a restart
    let fuel = j.last_values()[&FUEL];
    assert_eq!((fuel.value, fuel.time_ms), (39.5, t));
    assert_eq!(j.last_values()[&ODO].value, 1000.0);
    assert_eq!(j.last_values()[&RPM].value, 2000.0, "a no-data reading does not replace a good one");

    let csv = j.export_csv();
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].starts_with("start_utc,end_utc,distance_km"));
    assert!(rows[1].starts_with("2025-11-14T06:00:00Z,2025-11-14T06:09:59Z,9.98,10.0,60.0,60,2000,0.50,5.0,1000"), "{}", rows[1]);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn positions_become_a_route_and_a_parking_place() {
    let path = std::env::temp_dir().join(format!("openplug-route-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut t = 1_763_200_000_000;
    let start;
    {
        let mut j = Journal::open(&path).unwrap();
        j.on_connected(t);
        j.on_position(49.2000, 16.6000, t); // before the trip: no route yet
        assert!(j.active_trip().is_none());
        // drive north at 72 km/h = 20 m/s, a fix every second
        for i in 0..300u64 {
            j.on_values(&[el(SPEED, 72.0)], t);
            j.on_position(49.2 + i as f64 * 20.0 / 111_320.0, 16.6, t);
            t += 1000;
        }
        start = j.active_trip().unwrap().start_ms;
        let live = j.track(start);
        assert!((140..=150).contains(&live.len()), "thinned to one point per 2 s: {}", live.len());
        assert_eq!(live[5].3, 72.0, "points carry the speed");
        j.on_disconnected(t);
        assert!((j.parked().unwrap().pos.lat - 49.2537).abs() < 1e-3);
    }
    let mut j = Journal::open(&path).unwrap(); // the route survives a restart
    assert!(j.track(start).len() >= 140);
    let trip = j.tick(t + TRIP_FINISH_AFTER_MS).expect("finished");
    assert!((trip.start_pos.unwrap().lat - 49.2).abs() < 1e-3);
    assert!((trip.end_pos.unwrap().lat - 49.2537).abs() < 1e-3);
    assert!(j.track(start).len() >= 140, "and stays readable once the trip is finished");
    assert!(j.track(12345).is_empty());

    // a refuel on the next connection is placed where the car was parked
    t += 3_600_000;
    j.on_connected(t);
    j.on_values(&[el(FUEL, 20.0)], t - 1); // no earlier level: nothing to compare
    let mut j2 = Journal::in_memory();
    j2.on_connected(t);
    j2.on_values(&[el(FUEL, 20.0)], t);
    j2.on_position(50.0, 14.0, t);
    j2.on_disconnected(t + 1000);
    j2.on_connected(t + 600_000);
    j2.on_values(&[el(FUEL, 45.0)], t + 600_000);
    assert_eq!(j2.refuels()[0].pos.unwrap().lat, 50.0);

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(path.with_extension("tracks")).unwrap();
}

#[test]
fn a_standing_car_reporting_a_crawl_starts_no_trip() {
    let mut j = Journal::in_memory();
    let mut t = 1_000;
    j.on_connected(t);
    drive(&mut j, &mut t, 1.5, 120); // engine running, the speedometer creeping
    assert!(j.active_trip().is_none());
    // a single spike, or two, is not moving off either
    for v in [0.0, 8.0, 9.0, 0.0, 12.0, 0.0] {
        j.on_values(&[el(SPEED, v)], t);
        t += 1000;
    }
    assert!(j.active_trip().is_none());
}

#[test]
fn a_trip_starts_where_the_car_moved_off() {
    let mut j = Journal::in_memory();
    let mut t = 100_000;
    j.on_connected(t);
    j.on_position(49.2, 16.6, t);
    drive(&mut j, &mut t, 0.0, 5);
    let moved_off = t;
    drive(&mut j, &mut t, 4.0, 3); // not yet
    assert!(j.active_trip().is_none());
    drive(&mut j, &mut t, 30.0, 1); // three seconds at or above the threshold
    let trip = j.active_trip().expect("started");
    assert_eq!(trip.start_ms, moved_off);
    assert_eq!(trip.start_pos.unwrap().lat, 49.2, "the last fix marks the start");
    assert!(trip.distance_km > 0.0, "the samples before confirmation count");
    assert!(j.trip_state() == openplug_core::journal::TripState::Running);
}

#[test]
fn a_paused_trip_does_not_resume_on_a_crawl() {
    let mut j = Journal::in_memory();
    let mut t = 1_000;
    j.on_connected(t);
    drive(&mut j, &mut t, 50.0, 120);
    j.on_disconnected(t);
    t += 60_000;
    j.on_connected(t);
    drive(&mut j, &mut t, 1.0, 60);
    assert_eq!(j.trip_state(), openplug_core::journal::TripState::Paused);
    drive(&mut j, &mut t, 50.0, 10);
    assert_eq!(j.trip_state(), openplug_core::journal::TripState::Running);
}

#[test]
fn trips_can_be_filed_annotated_and_deleted() {
    use openplug_core::journal::TripKind;
    let path = std::env::temp_dir().join(format!("openplug-filing-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut t = 1_763_100_000_000;
    let (first, second);
    {
        let mut j = Journal::open(&path).unwrap();
        for _ in 0..2 {
            j.on_connected(t);
            j.on_position(49.2, 16.6, t);
            drive(&mut j, &mut t, 60.0, 120);
            j.on_position(49.21, 16.6, t);
            j.on_disconnected(t);
            t += TRIP_FINISH_AFTER_MS;
            j.tick(t);
        }
        first = j.trips()[0].start_ms;
        second = j.trips()[1].start_ms;
        assert!(j.set_trip_details(first, TripKind::Business, "  Client visit, \"Brno\"  "));
        assert!(j.set_trip_addresses(first, Some("Main St 1, Brno".into()), None));
        assert!(j.set_trip_addresses(first, None, Some("Hill Rd 2, Brno".into())));
        assert!(!j.set_trip_details(42, TripKind::Private, ""), "no such trip");
        assert!(!j.track(second).is_empty());
        assert!(j.delete_trip(second));
        assert!(!j.delete_trip(second));
        assert!(j.track(second).is_empty(), "the route goes with it");
    }
    let j = Journal::open(&path).unwrap();
    assert_eq!(j.trips().len(), 1);
    let trip = &j.trips()[0];
    assert_eq!((trip.kind, trip.note.as_str()), (TripKind::Business, "Client visit, \"Brno\""));
    assert_eq!(trip.start_address.as_deref(), Some("Main St 1, Brno"));
    assert_eq!(trip.end_address.as_deref(), Some("Hill Rd 2, Brno"));
    let csv = j.export_csv();
    assert!(csv.lines().next().unwrap().ends_with(",type,start_address,end_address,note"));
    assert!(
        csv.lines().nth(1).unwrap().ends_with(r#",business,"Main St 1, Brno","Hill Rd 2, Brno","Client visit, ""Brno""""#),
        "{csv}"
    );
    std::fs::remove_file(&path).unwrap();
    let _ = std::fs::remove_dir_all(path.with_extension("tracks"));
}

#[test]
fn the_tank_size_and_service_interval_give_a_level_and_a_reminder() {
    use openplug_core::journal::CarProfile;
    const DAYS_SINCE: u32 = 0x0007_0009;
    const KM_SINCE: u32 = 0x0007_0012;
    const DAY: u64 = 86_400_000;
    let mut j = Journal::in_memory();
    let t = 1_763_100_000_000;
    j.on_connected(t);
    j.on_values(&[el(FUEL, 37.5), el(DAYS_SINCE, 179.0), el(KM_SINCE, 6100.0)], t);
    assert_eq!(j.fuel_fraction(), None, "no tank size yet");
    assert_eq!(j.service_status(t), None, "no interval yet");

    j.set_profile(CarProfile { tank_l: Some(50.0), service_every_km: Some(15_000.0), service_every_days: Some(365) });
    assert_eq!(j.fuel_fraction(), Some(0.75));
    let s = j.service_status(t + 10 * DAY).unwrap();
    assert_eq!((s.km_left, s.days_left, s.read_ms), (Some(8_900.0), Some(176), t));
    assert!(!s.due_soon());
    assert!(j.service_status(t + 160 * DAY).unwrap().due_soon(), "the date comes first");

    j.set_profile(CarProfile { tank_l: Some(-3.0), service_every_km: Some(6_500.0), service_every_days: Some(0) });
    assert_eq!(j.profile(), CarProfile { tank_l: None, service_every_km: Some(6_500.0), service_every_days: None });
    let s = j.service_status(t).unwrap();
    assert_eq!((s.km_left, s.days_left), (Some(400.0), None));
    assert!(s.due_soon());
}
