//! Torn CED reads: culvert's `tests/torn_reads.rs` with `async`/`.await`.
//!
//! A sink's lane 0 counter that ticks while it is read: read one register at a time, its
//! low byte comes from one value and its high byte from the next, so 0x00FF ticking to
//! 0x0100 reads back as 0x01FF, a value the sink never held. The checksum at 0x56, which
//! the sink keeps correct for its current value, catches it; a block read in one
//! transaction, during which the sink holds its registers still, avoids it.

use core::cell::Cell;
use core::convert::Infallible;

use culvert_async::{CedLanes, CedWarning, Scdc};
use hdmi_hal_async::scdc::{ScdcTransport, read_block_bytewise};

/// Lane 0's counter, the sink's CED registers derived from it: lanes 1–3 have no valid
/// count, and the checksum is correct for the counter's current value.
struct Counter(Cell<u16>);

impl Counter {
    fn register(&self, reg: u8) -> u8 {
        let count = self.0.get();
        let low = count as u8;
        let high = 0x80 | (count >> 8) as u8;
        match reg {
            0x50 => low,
            0x51 => high,
            0x56 => low.wrapping_add(high).wrapping_neg(),
            _ => 0,
        }
    }

    fn tick(&self) {
        self.0.set(self.0.get() + 1);
    }
}

/// A transport that reads one register at a time; the counter ticks after every byte.
struct PerByte(Counter);

impl ScdcTransport for PerByte {
    type Error = Infallible;

    async fn read(&self, reg: u8) -> Result<u8, Infallible> {
        let value = self.0.register(reg);
        self.0.tick();
        Ok(value)
    }

    async fn write(&mut self, _reg: u8, _value: u8) -> Result<(), Infallible> {
        Ok(())
    }

    async fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Infallible> {
        read_block_bytewise(self, reg, buf).await
    }
}

/// A transport that reads a block in one transaction, during which the sink holds its
/// registers still; the counter ticks after every transaction.
struct Block(Counter);

impl ScdcTransport for Block {
    type Error = Infallible;

    async fn read(&self, reg: u8) -> Result<u8, Infallible> {
        let value = self.0.register(reg);
        self.0.tick();
        Ok(value)
    }

    async fn write(&mut self, _reg: u8, _value: u8) -> Result<(), Infallible> {
        Ok(())
    }

    async fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Infallible> {
        for (offset, byte) in (0..).zip(buf.iter_mut()) {
            *byte = self.0.register(reg + offset);
        }
        self.0.tick();
        Ok(())
    }
}

fn counter(start: u16) -> Counter {
    Counter(Cell::new(start))
}

#[pollster::test]
async fn a_torn_read_one_register_at_a_time_is_caught_by_the_checksum() {
    let reading = Scdc::new(PerByte(counter(0x00FF)))
        .read_ced(CedLanes::Three)
        .await
        .unwrap();
    // The value the sink never held, still decoded, with the warning next to it.
    assert_eq!(reading.counters.lane0.map(|c| c.value()), Some(0x01FF));
    assert!(matches!(
        reading.warnings(),
        [CedWarning::ChecksumMismatch { .. }]
    ));
}

#[pollster::test]
async fn a_block_read_in_one_transaction_is_not_torn() {
    let reading = Scdc::new(Block(counter(0x00FF)))
        .read_ced(CedLanes::Three)
        .await
        .unwrap();
    assert_eq!(reading.counters.lane0.map(|c| c.value()), Some(0x00FF));
    assert_eq!(reading.warnings(), []);
}

#[pollster::test]
async fn a_counter_ticking_between_bytes_never_passes_the_checksum() {
    // Read a byte at a time, the bytes come from different values: low byte at `c`, high
    // byte at `c + 1`, checksum at `c + 6`. Their sum is −6 or −7 (mod 256), never zero,
    // so every such read warns, carry or not; a block read never does.
    for start in (0..0x7F00).step_by(0x3B) {
        let per_byte = Scdc::new(PerByte(counter(start)))
            .read_ced(CedLanes::Three)
            .await
            .unwrap();
        assert!(
            matches!(per_byte.warnings(), [CedWarning::ChecksumMismatch { .. }]),
            "per byte from {start:#06x}"
        );
        let block = Scdc::new(Block(counter(start)))
            .read_ced(CedLanes::Three)
            .await
            .unwrap();
        assert_eq!(block.counters.lane0.map(|c| c.value()), Some(start));
        assert_eq!(block.warnings(), [], "block from {start:#06x}");
    }
}
