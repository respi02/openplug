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
pub mod testpipe {
    use super::Transport;
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{Arc, Mutex};

    type Shared = Arc<Mutex<VecDeque<u8>>>;

    pub struct Half {
        pub inbox: Shared,  // where I read from
        pub outbox: Shared, // where I write to
    }

    /// Creates a linked pair (a, b): what a sends, b reads, and vice versa.
    pub fn pair() -> (Half, Half) {
        let x: Shared = Arc::new(Mutex::new(VecDeque::new()));
        let y: Shared = Arc::new(Mutex::new(VecDeque::new()));
        (
            Half { inbox: x.clone(), outbox: y.clone() },
            Half { inbox: y, outbox: x },
        )
    }

    impl Transport for Half {
        fn send(&mut self, data: &[u8]) -> io::Result<()> {
            self.outbox.lock().unwrap().extend(data.iter().copied());
            Ok(())
        }
        fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut q = self.inbox.lock().unwrap();
            let n = buf.len().min(q.len());
            for b in buf.iter_mut().take(n) { *b = q.pop_front().unwrap(); }
            Ok(n)
        }
    }
}
