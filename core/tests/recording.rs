//! Recording the value stream and replaying it into the journal.

use std::path::PathBuf;

use openplug_core::dongle::{state, DataElement};
use openplug_core::journal::{Journal, TripState, TRIP_FINISH_AFTER_MS};
use openplug_core::recording::{self, Event, Recorder, Sample};

const SPEED: u32 = 0x0002_0000;
const FUEL: u32 = 0x0002_0002;
const ODO: u32 = 0x0002_0004;

fn el(url: u32, value: f32) -> DataElement {
    DataElement { url, state: state::OK, value, precision: Some(0) }
}

fn temp_path(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("openplug-test-{}-{nanos}", std::process::id())).join(name)
}

#[test]
fn a_recording_reads_back_exactly() {
    let path = temp_path("drive.jsonl");
    let odd = [0.1f32, 14.2, 37.4, 123_456.0, 1.0e-7, -40.0, f32::MAX];
    {
        let mut rec = Recorder::open(&path, 1_000).unwrap();
        rec.connected(1_500).unwrap();
        rec.values(&[], 1_600).unwrap(); // not recorded
        let els: Vec<DataElement> = odd.iter().map(|&v| el(SPEED, v)).collect();
        rec.values(&els, 2_000).unwrap();
        rec.values(&[DataElement { url: FUEL, state: state::NODATA, value: 0.0, precision: None }], 2_100).unwrap();
        rec.disconnected(3_000).unwrap();
    }
    {
        // a second session appends to the same file
        let mut rec = Recorder::open(&path, 9_000).unwrap();
        rec.connected(9_100).unwrap();
    }
    let events = recording::read(&path).unwrap();
    assert_eq!(events.len(), 7, "{events:?}");
    assert_eq!(events[0], Event::Start { t: 1_000, format: recording::FORMAT });
    assert_eq!(events[1], Event::Connected { t: 1_500 });
    let values: Vec<f32> = events[2].elements().iter().map(|e| e.value).collect();
    assert_eq!(values, odd, "f32 values survive the text format bit for bit");
    assert_eq!(events[3], Event::Values { t: 2_100, v: vec![Sample(FUEL, state::NODATA, 0.0, None)] });
    assert_eq!(events[4], Event::Disconnected { t: 3_000 });
    assert_eq!(events[5], Event::Start { t: 9_000, format: recording::FORMAT });
    assert_eq!(events[6], Event::Connected { t: 9_100 });
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn a_cut_off_last_line_is_skipped_but_damage_elsewhere_is_an_error() {
    let good = "{\"e\":\"start\",\"t\":1,\"format\":1}\n{\"e\":\"connected\",\"t\":2}\n";
    let cut = format!("{good}{{\"e\":\"values\",\"t\":3,\"v\":[[131");
    assert_eq!(recording::parse(&cut).unwrap().len(), 2);

    let broken = format!("{good}garbage\n{{\"e\":\"disconnected\",\"t\":4}}\n");
    assert_eq!(recording::parse(&broken).unwrap_err().line, 3);

    let newer = "{\"e\":\"start\",\"t\":1,\"format\":99}\n";
    assert!(recording::parse(newer).is_err());
}

#[test]
fn a_recorded_drive_replays_into_the_same_trips() {
    // record two drives the way the app does: connect, stream at 1 Hz, disconnect
    let path = temp_path("two-drives.jsonl");
    let mut live = Journal::in_memory();
    let mut rec = Recorder::open(&path, 0).unwrap();
    let mut t = 1_700_000_000_000u64;
    let feed = |live: &mut Journal, rec: &mut Recorder, els: &[DataElement], t: u64| {
        live.on_values(els, t);
        rec.values(els, t).unwrap();
    };

    // drive 1: 10 min at 60 km/h = 10 km
    live.on_connected(t);
    rec.connected(t).unwrap();
    feed(&mut live, &mut rec, &[el(ODO, 5000.0), el(FUEL, 30.0), el(SPEED, 0.0)], t);
    for i in 0..600 {
        t += 1000;
        let odo = 5000.0 + (i as f32 / 60.0).floor();
        feed(&mut live, &mut rec, &[el(SPEED, 60.0), el(ODO, odo)], t);
    }
    feed(&mut live, &mut rec, &[el(SPEED, 0.0), el(ODO, 5010.0), el(FUEL, 29.4)], t);
    live.on_disconnected(t);
    rec.disconnected(t).unwrap();

    // an hour later, after a refuel: drive 2, 5 min at 36 km/h = 3 km
    t += 3_600_000;
    live.tick(t);
    live.on_connected(t);
    rec.connected(t).unwrap();
    feed(&mut live, &mut rec, &[el(FUEL, 50.0), el(ODO, 5010.0)], t);
    for _ in 0..300 {
        t += 1000;
        feed(&mut live, &mut rec, &[el(SPEED, 36.0)], t);
    }
    live.on_disconnected(t);
    rec.disconnected(t).unwrap();
    drop(rec);

    let events = recording::read(&path).unwrap();
    let mut replayed = Journal::in_memory();
    recording::replay(&mut replayed, &events);

    assert_eq!(replayed.trips(), live.trips());
    assert_eq!(replayed.refuels(), live.refuels());
    assert_eq!(replayed.active_trip(), live.active_trip());
    assert_eq!(replayed.trips().len(), 1, "the first drive finished during the hour parked");
    assert!((replayed.trips()[0].distance_km - 10.0).abs() < 0.05, "{:?}", replayed.trips());
    assert!((replayed.refuels()[0].added_l - 20.6).abs() < 1e-3);
    assert_eq!(replayed.trip_state(), TripState::Paused, "the second drive waits for its grace period");

    replayed.tick(t + TRIP_FINISH_AFTER_MS);
    assert_eq!(replayed.trips().len(), 2);
    assert!((replayed.trips()[1].distance_km - 3.0).abs() < 0.05);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn standing_still_with_the_engine_on_records_no_trip() {
    let mut j = Journal::in_memory();
    let mut events = vec![Event::Start { t: 0, format: 1 }, Event::Connected { t: 1_000 }];
    for i in 0..300u64 {
        events.push(Event::Values { t: 2_000 + i * 1000, v: vec![Sample(SPEED, state::OK, 0.0, Some(0))] });
    }
    events.push(Event::Disconnected { t: 400_000 });
    recording::replay(&mut j, &events);
    assert_eq!(j.trip_state(), TripState::Idle);
    assert!(j.trips().is_empty());
}
