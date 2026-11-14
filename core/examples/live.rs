//! Desktop test harness: connect to a dongle and print its live values.
//!
//!   cargo run --example live -- --serial /dev/cu.DP_2_1_XXXX   # paired dongle (SPP)
//!   cargo run --example live -- --tcp 127.0.0.1:4455           # fake_dongle / a bridge
//!
//! Options: `--raw` also dumps every message's decoded protobuf;
//!          `--getters` first sends the app's empty-body getters (fields 22, 4, 1, 5);
//!          `--seconds N` stops after N seconds;
//!          `--record FILE` appends the value stream to FILE (replay it with the
//!          `replay` example or `fake_dongle --replay`).
//! The PIN (printed on the dongle's label) is read from `OPENPLUG_PIN`, or prompted.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use openplug_core::dongle::{parse_response, signal, state, APP_SUBSCRIPTIONS};
use openplug_core::link::DongleLink;
use openplug_core::protobuf::inspect;
use openplug_core::recording::Recorder;
use openplug_core::tls_client::HsError;
use openplug_core::transport::{IoTransport, Transport};

fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn opt(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// Opens a serial device in raw mode (no echo, no translation, blocking reads).
#[cfg(unix)]
fn open_serial(path: &str) -> io::Result<std::fs::File> {
    use std::os::unix::{fs::OpenOptionsExt, io::AsRawFd};
    let file = std::fs::OpenOptions::new().read(true).write(true).custom_flags(libc::O_NOCTTY).open(path)?;
    // SAFETY: plain termios calls on a file descriptor we own.
    unsafe {
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(file.as_raw_fd(), &mut tio) != 0 {
            return Err(io::Error::last_os_error());
        }
        libc::cfmakeraw(&mut tio);
        tio.c_cflag |= libc::CLOCAL | libc::CREAD;
        tio.c_cc[libc::VMIN] = 1;
        tio.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(file.as_raw_fd(), libc::TCSANOW, &tio) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(file)
}

fn pin() -> io::Result<String> {
    if let Ok(p) = std::env::var("OPENPLUG_PIN") {
        return Ok(p);
    }
    eprint!("PIN from the dongle label: ");
    io::stderr().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn run<T: Transport>(transport: T, pin: &str, mut recorder: Option<Recorder>) -> Result<(), HsError> {
    let raw = flag("--raw");
    let deadline = opt("--seconds").and_then(|s| s.parse().ok()).map(|s| Instant::now() + Duration::from_secs(s));

    eprintln!("handshake …");
    let mut link = DongleLink::connect(transport, pin)?;
    eprintln!("TLS-SRP session established");

    if flag("--getters") {
        for field in [22, 4, 1, 5] {
            link.get(field)?;
        }
    }
    for (param, rate) in APP_SUBSCRIPTIONS {
        link.subscribe(param, rate)?;
    }
    eprintln!("subscribed to {} parameters; streaming (Ctrl-C to stop)", APP_SUBSCRIPTIONS.len());
    let record = |recorder: &mut Option<Recorder>, f: &dyn Fn(&mut Recorder) -> io::Result<()>| {
        if let Some(r) = recorder {
            if let Err(e) = f(r) {
                eprintln!("recording stopped: {e}");
                *recorder = None;
            }
        }
    };
    record(&mut recorder, &|r| r.connected(now_ms()));

    while deadline.is_none_or(|d| Instant::now() < d) {
        let msg = match link.next_message() {
            Ok(m) => m,
            Err(e) => {
                record(&mut recorder, &|r| r.disconnected(now_ms()));
                return Err(e);
            }
        };
        let rsp = parse_response(&msg);
        record(&mut recorder, &|r| r.values(&rsp.elements, now_ms()));
        if raw || rsp.elements.is_empty() {
            println!("--- message, {} B (seq {:?}, tx {:?})\n{}", msg.len(), rsp.seq, rsp.transaction_id, inspect(&msg));
        }
        for el in rsp.elements {
            let value = match (el.state, signal(el.url)) {
                (state::NODATA, _) => "no data".to_string(),
                (_, Some(s)) => format!("{:.*} {}  ({})", el.precision.unwrap_or(1) as usize, el.value, s.unit, s.name),
                (_, None) => format!("{}", el.value),
            };
            println!("{:#010x}  {value}", el.url);
        }
    }
    record(&mut recorder, &|r| r.disconnected(now_ms()));
    link.close()
}

fn main() {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let recorder = match opt("--record") {
            Some(path) => Some(Recorder::open(std::path::Path::new(&path), now_ms())?),
            None => None,
        };
        if let Some(addr) = opt("--tcp") {
            let stream = std::net::TcpStream::connect(&addr)?;
            stream.set_nodelay(true)?;
            run(IoTransport(stream), &pin()?, recorder)?;
        } else if let Some(path) = opt("--serial") {
            #[cfg(unix)]
            run(IoTransport(open_serial(&path)?), &pin()?, recorder)?;
            #[cfg(not(unix))]
            return Err(format!("--serial {path}: only supported on Unix").into());
        } else {
            return Err("usage: live (--tcp HOST:PORT | --serial DEVICE) [--raw] [--getters] [--seconds N] [--record FILE]".into());
        }
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("error: {e}");
        if e.downcast_ref::<HsError>() == Some(&HsError::Alert { level: 2, description: 20 }) {
            eprintln!("hint: the dongle rejected our Finished — most likely a wrong PIN");
        }
        std::process::exit(1);
    }
}
