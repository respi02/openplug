//! Abstrakce transportu. Jádro čte/píše jen bajty; čím se přenášejí
//! (Android RFCOMM/SPP, iOS BLE GATT, desktop BT adaptér, test pipe)
//! řeší konkrétní implementace mimo jádro.

use std::io;

/// Obousměrný bajtový kanál k donglu. Blokující varianta pro jednoduchost;
/// async obal lze přidat později.
pub trait Transport {
    /// Odešle všechny bajty.
    fn send(&mut self, data: &[u8]) -> io::Result<()>;
    /// Přečte až `buf.len()` bajtů, vrátí počet (>0, nebo 0 při uzavření).
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

/// In-memory duplexní pipe pro offline testy (dvě propojené poloviny).
pub mod testpipe {
    use super::Transport;
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{Arc, Mutex};

    type Shared = Arc<Mutex<VecDeque<u8>>>;

    pub struct Half {
        pub inbox: Shared,  // odkud čtu
        pub outbox: Shared, // kam píšu
    }

    /// Vytvoří propojenou dvojici (a, b): co pošle a, přečte b a naopak.
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
