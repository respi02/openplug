//! Replays recorded drives into a fresh trip journal and reports what it makes of them.
//!
//!   cargo run --example replay -- drive-1.jsonl [drive-2.jsonl …] [--finish] [--signals]
//!                                 [--journal OUT.json]
//!
//! Recordings come from the apps ("Record values" switch) or from `live --record`.
//! Several files are replayed in order, as one continuous history. Prints the
//! timeline of what the journal did (trip started, paused, resumed, finished, refuel),
//! then the resulting trips. Times are UTC.
//!
//! `--finish`  finishes a trip still paused at the end, as the app would 15 min later.
//! `--signals` adds a per-signal summary (count, min, max, last) of everything recorded.
//! `--journal` writes the resulting journal file, as the app would have stored it.

use std::collections::BTreeMap;
use std::path::Path;

use openplug_core::dongle::{signal, state};
use openplug_core::journal::{Journal, Trip, TripState, MIN_TRIP_KM, TRIP_FINISH_AFTER_MS};
use openplug_core::recording::{self, Event};

/// `YYYY-MM-DD HH:MM:SS` in UTC (no date crate needed).
fn utc(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // civil-from-days, H. Hinnant
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn hms(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
}

fn describe(t: &Trip) -> String {
    let mut s = format!(
        "{:.2} km, {} moving, max {:.0} km/h, max {:.0} rpm",
        t.distance_km,
        hms(t.moving_s * 1000),
        t.max_speed_kmh,
        t.max_rpm
    );
    if let Some(f) = t.fuel_used_l.filter(|&f| f > 0.0) {
        s += &format!(", {f:.2} l");
    }
    if let Some(c) = t.consumption_l_per_100km() {
        s += &format!(" ({c:.1} l/100 km)");
    }
    if let Some(o) = t.end_odometer_km {
        s += &format!(", odometer {o:.0} km at the end");
    }
    s
}

#[derive(Default)]
struct Stat {
    n: u32,
    nodata: u32,
    min: f32,
    max: f32,
    last: f32,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned());
    let journal_out = opt("--journal");
    let files: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || args[i - 1] != "--journal"))
        .map(|(_, a)| a)
        .collect();
    if files.is_empty() {
        eprintln!("usage: replay RECORDING.jsonl… [--finish] [--signals] [--journal OUT.json]");
        std::process::exit(2);
    }

    // with --journal, replay straight into a new journal file
    let mut journal = match &journal_out {
        None => Journal::in_memory(),
        Some(out) => {
            let j = Journal::open(Path::new(out)).unwrap_or_else(|e| {
                eprintln!("{out}: {e}");
                std::process::exit(1);
            });
            if !j.trips().is_empty() || j.active_trip().is_some() || !j.refuels().is_empty() {
                eprintln!("{out}: already holds trips; give a new file");
                std::process::exit(1);
            }
            j
        }
    };
    let mut stats: BTreeMap<u32, Stat> = BTreeMap::new();
    let mut last_t = 0;
    for file in &files {
        let events = recording::read(Path::new(file)).unwrap_or_else(|e| {
            eprintln!("{file}: {e}");
            std::process::exit(1);
        });
        let (Some(first), Some(last)) = (events.first(), events.last()) else {
            println!("{file}: empty");
            continue;
        };
        let messages = events.iter().filter(|e| matches!(e, Event::Values { .. })).count();
        let connections = events.iter().filter(|e| matches!(e, Event::Connected { .. })).count();
        println!(
            "== {file}: {} → {} ({}), {connections} connection(s), {messages} messages",
            utc(first.time_ms()),
            utc(last.time_ms()),
            hms(last.time_ms() - first.time_ms())
        );

        for event in &events {
            let t = event.time_ms();
            let before = (journal.trip_state(), journal.trips().len(), journal.refuels().len());
            recording::apply(&mut journal, event);
            let after = (journal.trip_state(), journal.trips().len(), journal.refuels().len());
            last_t = t;

            match event {
                Event::Connected { .. } => println!("{}  connected", utc(t)),
                Event::Disconnected { .. } => println!("{}  disconnected", utc(t)),
                Event::Values { .. } => {
                    for e in event.elements() {
                        let s = stats.entry(e.url).or_insert(Stat { min: f32::MAX, max: f32::MIN, ..Stat::default() });
                        if e.state == state::OK {
                            s.n += 1;
                            s.min = s.min.min(e.value);
                            s.max = s.max.max(e.value);
                            s.last = e.value;
                        } else {
                            s.nodata += 1;
                        }
                    }
                }
                Event::Start { .. } => {}
            }
            if after.1 > before.1 {
                let trip = journal.trips().last().unwrap();
                println!("{}  trip finished (ended {}): {}", utc(t), utc(trip.end_ms), describe(trip));
            } else if before.0 != TripState::Idle && after.0 == TripState::Idle {
                println!("{}  trip dropped: shorter than {:.0} m", utc(t), MIN_TRIP_KM * 1000.0);
            }
            match (before.0, after.0) {
                (TripState::Idle, TripState::Running) => println!("{}  trip started", utc(t)),
                (TripState::Running, TripState::Paused) => println!("{}  trip paused", utc(t)),
                (TripState::Paused, TripState::Running) => println!("{}  trip resumed", utc(t)),
                _ => {}
            }
            if after.2 > before.2 {
                let r = journal.refuels().last().unwrap();
                println!("{}  refuel: +{:.1} l, {:.1} l in the tank", utc(t), r.added_l, r.fuel_after_l);
            }
        }
    }

    if args.iter().any(|a| a == "--finish") {
        if journal.trip_state() == TripState::Running {
            journal.on_disconnected(last_t);
        }
        if journal.trip_state() == TripState::Paused {
            match journal.tick(last_t + TRIP_FINISH_AFTER_MS) {
                Some(trip) => println!("(--finish) trip finished: {}", describe(&trip)),
                None => println!("(--finish) trip dropped: shorter than {:.0} m", MIN_TRIP_KM * 1000.0),
            }
        }
    }

    println!("\n== result");
    for (i, trip) in journal.trips().iter().enumerate() {
        println!("trip {}: {} → {}: {}", i + 1, utc(trip.start_ms), utc(trip.end_ms), describe(trip));
    }
    if let Some(trip) = journal.active_trip() {
        println!("in progress ({:?}): {}", journal.trip_state(), describe(&trip));
    }
    for r in journal.refuels() {
        println!("refuel {}: +{:.1} l", utc(r.time_ms), r.added_l);
    }
    if journal.trips().is_empty() && journal.active_trip().is_none() {
        println!("no trips");
    }

    if args.iter().any(|a| a == "--signals") {
        println!("\n== signals");
        println!("{:<10} {:<36} {:>6} {:>6} {:>12} {:>12} {:>12}", "url", "name", "n", "nodata", "min", "max", "last");
        for (url, s) in &stats {
            let name = signal(*url).map(|s| format!("{} ({})", s.name, s.unit)).unwrap_or_else(|| "?".into());
            let (min, max, last) = if s.n > 0 { (s.min, s.max, s.last) } else { (f32::NAN, f32::NAN, f32::NAN) };
            println!("{url:#010x} {name:<36} {:>6} {:>6} {min:>12.3} {max:>12.3} {last:>12.3}", s.n, s.nodata);
        }
    }

    if let Some(out) = journal_out {
        journal.flush();
        println!("\njournal written to {out}");
    }
}
