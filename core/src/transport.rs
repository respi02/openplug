//! Transport abstraction. The core only reads/writes bytes; how they travel
//! (Android RFCOMM/SPP, iOS BLE GATT, a desktop BT adapter, a test pipe) is
//! handled by concrete implementations outside the core.

use std::io;

/// Bidirectional byte channel to the dongle. Blocking for simplicity;
/// an async wrapper can be added later.
pub trait Transport {
    /// Send all bytes.
    fn send(&mut self, data: &[u8]) -> io::Result<()>;
    /// Read up to `buf.len()` bytes; returns the count (>0, or 0 when closed).
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

/// In-memory duplex pipe for offline tests (two linked halves).
///
/// Honors the [`Transport`] contract: `recv` blocks until data arrives and returns
/// 0 only once the peer half has been dropped and everything it sent was read.
pub mod testpipe {
    use super::Transport;
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{Arc, Condvar, Mutex};

    #[derive(Default)]
    struct Pipe {
        buf: VecDeque<u8>,
        closed: bool, // the writing half is gone
    }

    type Shared = Arc<(Mutex<Pipe>, Condvar)>;

    pub struct Half {
        inbox: Shared,  // where I read from
        outbox: Shared, // where I write to
    }

    /// Creates a linked pair (a, b): what a sends, b reads, and vice versa.
    pub fn pair() -> (Half, Half) {
        let x: Shared = Arc::default();
        let y: Shared = Arc::default();
        (
            Half { inbox: x.clone(), outbox: y.clone() },
            Half { inbox: y, outbox: x },
        )
    }

    impl Transport for Half {
        fn send(&mut self, data: &[u8]) -> io::Result<()> {
            let (lock, cv) = &*self.outbox;
            lock.lock().unwrap().buf.extend(data.iter().copied());
            cv.notify_all();
            Ok(())
        }
        fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let (lock, cv) = &*self.inbox;
            let mut p = lock.lock().unwrap();
            while p.buf.is_empty() && !p.closed {
                p = cv.wait(p).unwrap();
            }
            let n = buf.len().min(p.buf.len());
            for b in buf.iter_mut().take(n) {
                *b = p.buf.pop_front().unwrap();
            }
            Ok(n) // 0 only when closed and drained
        }
    }

    impl Drop for Half {
        fn drop(&mut self) {
            let (lock, cv) = &*self.outbox;
            lock.lock().unwrap().closed = true;
            cv.notify_all();
        }
    }
}
