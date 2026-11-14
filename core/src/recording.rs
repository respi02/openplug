//! Recording the value stream of a drive, and replaying it.
//!
//! A recording holds exactly what the trip journal is fed: when the link to the dongle
//! came up and went down, and every batch of values with the time it arrived. That is
//! enough to replay a real drive at the desk — into a [`Journal`] (to check or tune the
//! trip rules against it) or out of the fake dongle (to drive an app with it).
//!
//! Format: JSON Lines, one event per line, times in milliseconds since the Unix epoch.
//!
//! ```text
//! {"e":"start","t":1763060000000,"format":1}
//! {"e":"connected","t":1763060003120}
//! {"e":"values","t":1763060003410,"v":[[131072,0,0.0,0],[131076,0,123456.0,0]]}
//! {"e":"disconnected","t":1763061800000}
//! ```
//!
//! Each value is `[url, state, value, precision]` (precision may be `null`). A
//! `start` line opens every recording session; several sessions may follow each other
//! in one file. A recording contains vehicle values only — no position and no VIN.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::dongle::DataElement;
use crate::journal::Journal;

/// Version of the line format written by [`Recorder`].
pub const FORMAT: u32 = 1;

/// One value as stored: `[url, state, value, precision]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample(pub u32, pub u32, pub f32, pub Option<u32>);

impl From<&DataElement> for Sample {
    fn from(e: &DataElement) -> Self {
        Self(e.url, e.state, e.value, e.precision)
    }
}

impl From<&Sample> for DataElement {
    fn from(s: &Sample) -> Self {
        Self { url: s.0, state: s.1, value: s.2, precision: s.3 }
    }
}

/// One line of a recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "e", rename_all = "snake_case")]
pub enum Event {
    /// A recording session began (the app started recording).
    Start { t: u64, format: u32 },
    /// The link to the dongle is up: authenticated and subscribed.
    Connected { t: u64 },
    /// The link was lost or closed.
    Disconnected { t: u64 },
    /// One message from the dongle with the values it carried.
    Values { t: u64, v: Vec<Sample> },
}

impl Event {
    pub fn time_ms(&self) -> u64 {
        match self {
            Self::Start { t, .. } | Self::Connected { t } | Self::Disconnected { t } | Self::Values { t, .. } => *t,
        }
    }

    /// The values of a `Values` event, as data elements (empty for other events).
    pub fn elements(&self) -> Vec<DataElement> {
        match self {
            Self::Values { v, .. } => v.iter().map(DataElement::from).collect(),
            _ => Vec::new(),
        }
    }
}

/// Appends events to a recording file. Every event is written (and handed to the OS)
/// as it happens, so a recording survives the app being killed mid-drive.
pub struct Recorder {
    file: File,
}

impl Recorder {
    /// Opens `path` for appending (creating it if needed) and writes a `start` line.
    pub fn open(path: &Path, now_ms: u64) -> io::Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let mut rec = Self { file };
        rec.write(&Event::Start { t: now_ms, format: FORMAT })?;
        Ok(rec)
    }

    pub fn connected(&mut self, now_ms: u64) -> io::Result<()> {
        self.write(&Event::Connected { t: now_ms })
    }

    pub fn disconnected(&mut self, now_ms: u64) -> io::Result<()> {
        self.write(&Event::Disconnected { t: now_ms })
    }

    /// Records one message's values; a message without values is not recorded.
    pub fn values(&mut self, elements: &[DataElement], now_ms: u64) -> io::Result<()> {
        if elements.is_empty() {
            return Ok(());
        }
        self.write(&Event::Values { t: now_ms, v: elements.iter().map(Sample::from).collect() })
    }

    pub fn write(&mut self, event: &Event) -> io::Result<()> {
        let mut line = serde_json::to_vec(event).map_err(io::Error::other)?;
        line.push(b'\n');
        self.file.write_all(&line) // one write per line: no interleaved partial lines
    }
}

/// A line of a recording that could not be read.
#[derive(Debug, thiserror::Error, PartialEq)]
#[error("line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

/// Reads a recording. Blank lines are skipped, and so is a broken *last* line (a write
/// cut short when the app was killed); a broken line anywhere else is an error.
pub fn parse(text: &str) -> Result<Vec<Event>, ParseError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut events = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Event>(line) {
            Ok(Event::Start { format, .. }) if format > FORMAT => {
                return Err(ParseError { line: i + 1, message: format!("recording format {format} is newer than this reader") });
            }
            Ok(ev) => events.push(ev),
            Err(_) if i + 1 == lines.len() && !text.ends_with('\n') => {} // truncated tail
            Err(e) => return Err(ParseError { line: i + 1, message: e.to_string() }),
        }
    }
    Ok(events)
}

/// Reads the recording stored in `path`.
pub fn read(path: &Path) -> io::Result<Vec<Event>> {
    let text = std::fs::read_to_string(path)?;
    parse(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Feeds one recorded event into a journal, the way the app did when it happened.
/// Paused trips whose grace period ran out are finished first, as the app's periodic
/// tick would have done.
pub fn apply(journal: &mut Journal, event: &Event) {
    let t = event.time_ms();
    journal.tick(t);
    match event {
        Event::Start { .. } => {}
        Event::Connected { .. } => journal.on_connected(t),
        Event::Disconnected { .. } => journal.on_disconnected(t),
        Event::Values { .. } => journal.on_values(&event.elements(), t),
    }
}

/// Replays a whole recording into a journal.
pub fn replay(journal: &mut Journal, events: &[Event]) {
    for event in events {
        apply(journal, event);
    }
}
