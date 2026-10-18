//! Test-only async transport backed by a 256-byte register array.
//!
//! [`TestTransport`] is the single transport type used across all client unit
//! tests. A single generic instantiation avoids the dead-code coverage regions
//! that arise from LLVM counting per-monomorphization `?` Err branches.

use core::cell::{Cell, RefCell};
use hdmi_hal_async::scdc::ScdcTransport;

/// How many operations [`TestTransport`] records.
const LOG_LEN: usize = 16;

/// One successful transport operation, as [`TestTransport::log`] records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Read(u8),
    Write(u8, u8),
}

/// Async in-memory transport that succeeds for the first `fail_after` operations
/// then returns `Err(())`.
///
/// Construct with [`TestTransport::new`] for happy-path tests (never fails) or
/// [`TestTransport::failing_after`] to exercise error branches.
pub struct TestTransport {
    pub regs: [u8; 256],
    fail_after: usize,
    ops: Cell<usize>,
    log: RefCell<[Option<Op>; LOG_LEN]>,
}

impl TestTransport {
    pub fn new() -> Self {
        Self {
            regs: [0u8; 256],
            fail_after: usize::MAX,
            ops: Cell::new(0),
            log: RefCell::new([None; LOG_LEN]),
        }
    }

    pub fn failing_after(n: usize) -> Self {
        Self {
            regs: [0u8; 256],
            fail_after: n,
            ops: Cell::new(0),
            log: RefCell::new([None; LOG_LEN]),
        }
    }

    pub fn set(&mut self, addr: u8, val: u8) {
        self.regs[addr as usize] = val;
    }

    pub fn get(&self, addr: u8) -> u8 {
        self.regs[addr as usize]
    }

    /// The first operations, in order: the reads and writes a method made, so a test can
    /// pin their order and count. Unused entries are `None`.
    pub fn log(&self) -> [Option<Op>; LOG_LEN] {
        *self.log.borrow()
    }

    fn record(&self, op: Op) {
        let n = self.ops.get();
        if n < LOG_LEN {
            self.log.borrow_mut()[n] = Some(op);
        }
        self.ops.set(n + 1);
    }
}

impl ScdcTransport for TestTransport {
    type Error = ();

    async fn read(&self, reg: u8) -> Result<u8, ()> {
        if self.ops.get() >= self.fail_after {
            return Err(());
        }
        self.record(Op::Read(reg));
        Ok(self.regs[reg as usize])
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), ()> {
        if self.ops.get() >= self.fail_after {
            return Err(());
        }
        self.record(Op::Write(reg, value));
        self.regs[reg as usize] = value;
        Ok(())
    }
}

#[pollster::test]
async fn the_log_keeps_the_first_operations() {
    let transport = TestTransport::new();
    for reg in 0..=LOG_LEN as u8 {
        transport.read(reg).await.unwrap();
    }
    let log = transport.log();
    assert_eq!(log[0], Some(Op::Read(0)));
    assert_eq!(log[LOG_LEN - 1], Some(Op::Read(LOG_LEN as u8 - 1)));
}
