//! A stand-in dongle on TCP, for developing clients without the car.
//!
//!   cargo run --example fake_dongle -- [--listen 127.0.0.1:4455] [--pin 1234]
//!                                      [--replay RECORDING.jsonl [--speed N]]
//!
//! Speaks the real stack (TLS-SRP, cipher 0xC01D, length-delimited protobuf) and
//! answers Subscribe requests with a stream of values, to any number of clients at
//! once. By default the values are
//! SYNTHETIC; the wire format and the paramId → object grouping follow the real dongle.
//!
//! With `--replay` it plays back a recorded drive instead (see
//! `openplug_core::recording`): each recorded message is sent at its recorded time,
//! counted from the moment the first client connects, and where the recording lost
//! the link the fake drops the client and refuses connections until the recording
//! reconnects — so the app sees the same comings and goings as on the road.
//! `--speed N` plays N times faster; the app's journal then sees time compressed, so
//! its distances come out N times shorter (for exact numbers use the `replay` example).

use std::io;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use num_bigint::BigUint;
use openplug_core::dongle::{build_response, parse_request, state, DataElement, Deframer, Request};
use openplug_core::groups::RFC5054_1536;
use openplug_core::recording::{self, Event};
use openplug_core::tls_client::{server_handshake, HsError, ServerConfig};
use openplug_core::transport::IoTransport;

fn arg(name: &str, default: &str) -> String {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned()).unwrap_or_else(|| default.into())
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("OS RNG");
    b
}

/// Synthetic readings for a subscribed parameter at time `t` seconds. The
/// paramId → object grouping follows what a real dongle sends; the values are made up.
fn readings(param: u32, t: f32) -> Vec<DataElement> {
    let ok = |url, value, prec| DataElement { url, state: state::OK, value, precision: Some(prec) };
    let none = |url| DataElement { url, state: state::NODATA, value: 0.0, precision: Some(0) };
    let wave = |period: f32| (t / period).sin();
    match param {
        1 => vec![none(0x0002_0009), ok(0x0002_001a, 5.9, 1)],
        4 => vec![ok(0x0002_0004, 123_456.0, 0)],
        6 => vec![ok(0x0002_0005, 570.0, 0)],
        7 => vec![none(0x0002_0001), ok(0x0002_0002, 37.4, 1)],
        8 => vec![ok(0x0007_0009, 179.0, 0), ok(0x0007_0012, 6100.0, 0)],
        14 => vec![ok(0x0004_0000, (1500.0 + 700.0 * wave(3.0)).round(), 0)],
        16 => vec![ok(0x0004_0001, 88.0, 0)],
        19 => vec![ok(0x0002_0030, 92.0, 0), none(0x0004_000f)],
        20 => vec![ok(0x0005_0001, 14.2 + 0.2 * wave(5.0), 1), ok(0x0014_0001, 14.2, 1)],
        23 => vec![ok(0x0002_0000, (50.0 + 20.0 * wave(7.0)).round(), 0)],
        28 => vec![ok(0x0004_000c, 101.0, 0), ok(0x0004_000d, 21.0, 0), ok(0x0004_000e, 7.0 + wave(2.0), 2)],
        30 => vec![ok(0x0004_0026, 20.0 + 10.0 * wave(4.0), 1), ok(0x0004_0027, 12.0, 1)],
        _ => vec![], // groups our test car answers with nothing useful
    }
}

struct Sub { param: u32, rate: Duration, txid: u32, due: Instant }

/// Playback state of a recording, shared by all clients in turn.
struct Replay {
    events: Vec<Event>,
    next: usize,
    /// Recording time that corresponds to `started`.
    t0: u64,
    started: Option<Instant>,
    speed: f64,
    /// Whether the dongle is reachable at this point of the recording.
    online: bool,
}

/// What a replay step asks of the connection.
enum Step {
    Send(Vec<DataElement>),
    Drop,
}

impl Replay {
    fn load(path: &str, speed: f64) -> io::Result<Self> {
        let events: Vec<Event> = recording::read(std::path::Path::new(path))?
            .into_iter()
            .filter(|e| !matches!(e, Event::Start { .. }))
            .collect();
        // start where the recording first had the dongle
        let t0 = events.iter().find(|e| matches!(e, Event::Connected { .. })).map_or(0, Event::time_ms);
        let events: Vec<Event> = events.into_iter().filter(|e| e.time_ms() >= t0).collect();
        let span = events.last().map_or(0, |e| e.time_ms() - t0);
        println!("replaying {path}: {} events over {} s at {speed}× speed", events.len(), span / 1000);
        Ok(Self { events, next: 0, t0, started: None, speed, online: false })
    }

    fn start_clock(&mut self) {
        if self.started.is_none() {
            self.started = Some(Instant::now());
        }
    }

    fn finished(&self) -> bool {
        self.next >= self.events.len()
    }

    /// The next due step, if any; updates `online` as connection events go by.
    fn step(&mut self) -> Option<Step> {
        let started = self.started?;
        let now = self.t0 + (started.elapsed().as_secs_f64() * 1000.0 * self.speed) as u64;
        while let Some(ev) = self.events.get(self.next).filter(|e| e.time_ms() <= now) {
            self.next += 1;
            let at = (ev.time_ms() - self.t0) / 1000;
            match ev {
                Event::Connected { .. } => {
                    self.online = true;
                    println!("  [{at:>6} s] recording: dongle reachable");
                }
                Event::Disconnected { .. } => {
                    self.online = false;
                    println!("  [{at:>6} s] recording: link lost");
                    return Some(Step::Drop);
                }
                Event::Values { .. } if self.online => return Some(Step::Send(ev.elements())),
                _ => {}
            }
        }
        None
    }
}

fn serve(stream: TcpStream, pin: &str, mut replay: Option<&mut Replay>) -> Result<(), HsError> {
    let probe = stream.try_clone().map_err(|_| HsError::Io)?;
    let cfg = ServerConfig {
        group: &RFC5054_1536,
        password: pin.as_bytes(),
        salt: random::<32>().to_vec(),
        server_random: random(),
        b: BigUint::from_bytes_be(&random::<32>()),
        session_id: vec![],
        finished_iv: random(),
    };
    let mut sess = server_handshake(IoTransport(stream), &cfg)?;
    println!("  session established");

    let start = Instant::now();
    let (mut de, mut subs, mut seq) = (Deframer::new(), Vec::<Sub>::new(), 0u32);
    loop {
        // Wait briefly for a request without consuming it, so records stay intact.
        probe.set_read_timeout(Some(Duration::from_millis(50))).map_err(|_| HsError::Io)?;
        let pending = match probe.peek(&mut [0u8; 1]) {
            Ok(0) => return Err(HsError::Closed),
            Ok(_) => true,
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => false,
            Err(_) => return Err(HsError::Io),
        };
        probe.set_read_timeout(None).map_err(|_| HsError::Io)?;
        if pending {
            de.push(&sess.recv_app()?);
            while let Some(msg) = de.next_message() {
                match parse_request(&msg) {
                    Some(Request::Subscribe { param_id, rate_ms, txid }) => {
                        println!("  subscribe param {param_id} every {rate_ms} ms (tx {txid})");
                        subs.retain(|s| s.param != param_id);
                        subs.push(Sub { param: param_id, rate: Duration::from_millis(rate_ms.max(100) as u64), txid, due: Instant::now() });
                    }
                    Some(Request::Other { field, txid }) => println!("  request field {field} (tx {txid}) — ignored"),
                    None => println!("  unparseable request, {} B", msg.len()),
                }
            }
        }
        if let Some(replay) = replay.as_deref_mut() {
            while let Some(step) = replay.step() {
                match step {
                    Step::Drop => return Ok(()),
                    // the client reads values regardless of which subscription they answer
                    Step::Send(els) => {
                        if let Some(s) = subs.first() {
                            seq += 1;
                            sess.send_app(&build_response(seq, s.param, s.txid, &els))?;
                        }
                    }
                }
            }
            if replay.finished() {
                println!("  end of the recording");
                return Ok(());
            }
            continue;
        }
        let now = Instant::now();
        for s in subs.iter_mut().filter(|s| s.due <= now) {
            seq += 1;
            let t = start.elapsed().as_secs_f32();
            let els = readings(s.param, t);
            if !els.is_empty() {
                sess.send_app(&build_response(seq, s.param, s.txid, &els))?;
            }
            s.due = now + s.rate;
        }
    }
}

fn main() -> io::Result<()> {
    let (addr, pin) = (arg("--listen", "127.0.0.1:4455"), arg("--pin", "1234"));
    let mut replay = match std::env::args().position(|a| a == "--replay") {
        Some(_) => {
            let speed: f64 = arg("--speed", "1").parse().map_err(|_| io::Error::other("--speed takes a number"))?;
            Some(Replay::load(&arg("--replay", ""), speed)?)
        }
        None => None,
    };
    let listener = TcpListener::bind(&addr)?;
    println!("fake dongle on {addr} ({})", if replay.is_some() { "replaying a recording" } else { "synthetic values" });
    listener.set_nonblocking(replay.is_some())?;
    loop {
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // no client: the recording plays on regardless
                if let Some(r) = replay.as_mut() {
                    while r.step().is_some() {}
                    if r.finished() && r.started.is_some() {
                        println!("end of the recording");
                        return Ok(());
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) => return Err(e),
        };
        stream.set_nonblocking(false)?;
        let peer = stream.peer_addr()?;
        println!("client {peer}");
        if replay.is_none() {
            // Synthetic values: serve every client at once (say a phone and a simulator).
            // A replay has one timeline, so it serves one client at a time like the dongle.
            let pin = pin.clone();
            std::thread::spawn(move || match serve(stream, &pin, None) {
                Err(HsError::Closed) => println!("  client {peer} closed"),
                Err(e) => println!("  client {peer} ended: {e}"),
                Ok(()) => println!("  dropped client {peer}"),
            });
            continue;
        }
        if let Some(r) = replay.as_mut() {
            r.start_clock();
            while r.step().is_some() {} // catch up to now
            if !r.online {
                println!("  refused: the dongle is out of reach at this point of the recording");
                continue;
            }
        }
        match serve(stream, &pin, replay.as_mut()) {
            Err(HsError::Closed) => println!("  client closed"),
            Err(e) => println!("  ended: {e}"),
            Ok(()) => println!("  dropped the client"),
        }
    }
}
