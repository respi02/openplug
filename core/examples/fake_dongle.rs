//! A stand-in dongle on TCP, for developing clients without the car.
//!
//!   cargo run --example fake_dongle -- [--listen 127.0.0.1:4455] [--pin 1234]
//!
//! Speaks the real stack (TLS-SRP, cipher 0xC01D, length-delimited protobuf) and
//! answers Subscribe requests with a stream of values. The values are SYNTHETIC;
//! the wire format and the paramId → object grouping follow the real dongle.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use num_bigint::BigUint;
use openplug_core::dongle::{build_response, parse_request, state, DataElement, Deframer, Request};
use openplug_core::groups::RFC5054_1536;
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

fn serve(stream: TcpStream, pin: &str) -> Result<(), HsError> {
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
    let listener = TcpListener::bind(&addr)?;
    println!("fake dongle on {addr} (synthetic values)");
    for stream in listener.incoming() {
        let stream = stream?;
        println!("client {}", stream.peer_addr()?);
        match serve(stream, &pin) {
            Err(HsError::Closed) => println!("  client closed"),
            Err(e) => println!("  ended: {e}"),
            Ok(()) => {}
        }
    }
    Ok(())
}
