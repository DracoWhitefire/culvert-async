//! Test-only async transport backed by a 256-byte register array.
//!
//! [`TestTransport`] is the single transport type used across all client unit
//! tests. A single generic instantiation avoids the dead-code coverage regions
//! that arise from LLVM counting per-monomorphization `?` Err branches.

use hdmi_hal_async::scdc::ScdcTransport;

/// Async in-memory transport that succeeds for the first `fail_after` operations
/// then returns `Err(())`.
///
/// Construct with [`TestTransport::new`] for happy-path tests (never fails) or
/// [`TestTransport::failing_after`] to exercise error branches.
pub struct TestTransport {
    pub regs: [u8; 256],
    fail_after: usize,
    ops: usize,
}

impl TestTransport {
    pub fn new() -> Self {
        Self {
            regs: [0u8; 256],
            fail_after: usize::MAX,
            ops: 0,
        }
    }

    pub fn failing_after(n: usize) -> Self {
        Self {
            regs: [0u8; 256],
            fail_after: n,
            ops: 0,
        }
    }

    pub fn set(&mut self, addr: u8, val: u8) {
        self.regs[addr as usize] = val;
    }

    pub fn get(&self, addr: u8) -> u8 {
        self.regs[addr as usize]
    }
}

impl ScdcTransport for TestTransport {
    type Error = ();

    async fn read(&mut self, reg: u8) -> Result<u8, ()> {
        if self.ops >= self.fail_after {
            return Err(());
        }
        self.ops += 1;
        Ok(self.regs[reg as usize])
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), ()> {
        if self.ops >= self.fail_after {
            return Err(());
        }
        self.ops += 1;
        self.regs[reg as usize] = value;
        Ok(())
    }
}
