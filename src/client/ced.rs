use hdmi_hal_async::scdc::ScdcTransport;

use crate::ScdcError;
use crate::{CedLanes, CedReading, RsCorrectionCount};
use culvert::codec;

use super::Scdc;

impl<T: ScdcTransport> Scdc<T> {
    /// Reads the per-lane character error counts: the `ERR_DET` block, 0x50–0x58 (lanes
    /// 0–2, the checksum at 0x56, lane 3), in one [`read_block`](ScdcTransport::read_block),
    /// and checks the checksum.
    ///
    /// `lanes` is the link's: [`CedLanes::Three`] for TMDS and 3-lane FRL, `Four` for 4-lane
    /// FRL (`CedLanes::from(rate)`). It decides what the checksum covers and whether lane 3
    /// is read as a count.
    ///
    /// `Err` only for a transport failure. What the read finds wrong is a
    /// [`CedWarning`](crate::CedWarning) in the [`CedReading`], with the counters decoded
    /// either way: a checksum mismatch (most likely a read torn by a counter update), or a
    /// count for lane 3 when it is not in use. culvert-async does not read again; whether
    /// to is the caller's decision, and a sink that clears the counters on read returns
    /// different counts the second time.
    ///
    /// On a transport that reads a block in one transaction, the counters cannot be torn
    /// between their bytes by the transport; one that reads a register at a time
    /// ([`read_block_bytewise`]) says so, and the checksum is then what catches most torn
    /// reads. [`CedReading`] says how the counts behave (saturation, clear on read, resets).
    ///
    /// [`read_block_bytewise`]: hdmi_hal_async::scdc::read_block_bytewise
    pub async fn read_ced(&mut self, lanes: CedLanes) -> Result<CedReading, ScdcError<T::Error>> {
        let mut block = [0; codec::CED_BLOCK_LEN];
        self.transport
            .read_block(codec::CED_BLOCK_START, &mut block)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_ced_block(&block, lanes))
    }

    /// Reads the Reed-Solomon correction count from `RS_Correction_L/H` (0x59/0x5A).
    ///
    /// Returns `None` if the high byte's validity bit (bit 7) is not set: unknown, not
    /// zero. The count is maintained by the sink in FRL mode.
    ///
    /// Both bytes are one [`read_block`](ScdcTransport::read_block), so on a transport that
    /// reads a block in one transaction a sink update cannot fall between them; one that
    /// reads a register at a time ([`read_block_bytewise`]) says so. The count has no
    /// checksum: the transaction is its only protection against a torn read.
    ///
    /// The count accumulates and saturates at 0x7FFF, so a saturated count means "at
    /// least". A sink may clear it when it is read, so a reading is a running total on
    /// some sinks and the count since the previous read on others; a reading lower than
    /// the previous one means a reset (clear on read, hot plug, an SCDC clear), and
    /// counting starts again from it. Count afresh after retraining, a rate change or a
    /// hot plug too.
    ///
    /// `UpdateFlags::rsed_update` is a latched hint: the sink sets it when corrections
    /// occur, and it stays set until the source clears it; a sink may never set it. Read
    /// the count regularly rather than only after the flag.
    ///
    /// [`read_block_bytewise`]: hdmi_hal_async::scdc::read_block_bytewise
    pub async fn read_rs_correction(
        &mut self,
    ) -> Result<Option<RsCorrectionCount>, ScdcError<T::Error>> {
        let mut bytes = [0; 2];
        self.transport
            .read_block(codec::RS_CORRECTION_L, &mut bytes)
            .await
            .map_err(ScdcError::Transport)?;
        let [low, high] = bytes;
        Ok(codec::decode_rs_correction(low, high))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::{Op, TestTransport};
    use crate::ScdcError;
    use crate::{CedCount, CedCounters, CedLanes, CedWarning, FrlRate};

    /// A transport whose CED block holds `block`, with the checksum at 0x56 made valid for
    /// `lanes`.
    fn with_block(mut block: [u8; 9], lanes: CedLanes) -> TestTransport {
        let covered = match lanes {
            CedLanes::Three => 7,
            CedLanes::Four => 9,
        };
        block[6] = 0;
        let sum = block[..covered]
            .iter()
            .fold(0u8, |sum, &b| sum.wrapping_add(b));
        block[6] = sum.wrapping_neg();
        let mut sim = TestTransport::new();
        for (reg, byte) in (0x50..).zip(block) {
            sim.set(reg, byte);
        }
        sim
    }

    #[pollster::test]
    async fn read_ced_is_one_block_read_of_0x50_to_0x58() {
        for lanes in [CedLanes::Three, CedLanes::Four] {
            let mut scdc = Scdc::new(TestTransport::new());
            scdc.read_ced(lanes).await.unwrap();
            assert_eq!(
                scdc.into_transport().log()[..2],
                [Some(Op::ReadBlock(0x50, 9)), None]
            );
        }
    }

    #[pollster::test]
    async fn ced_validity_bit_required() {
        // High byte with bit 7 clear → None regardless of counter value.
        let sim = with_block([0xFF, 0x7F, 0, 0, 0, 0, 0, 0, 0], CedLanes::Three);
        let reading = Scdc::new(sim).read_ced(CedLanes::Three).await.unwrap();
        assert_eq!(reading.counters.lane0, None);
        assert_eq!(reading.warnings(), []);
    }

    #[pollster::test]
    async fn ced_counter_validity_bit_stripped() {
        // High byte: bit 7 set (valid), counter bits = 0x01; low byte = 0x23.
        let sim = with_block([0x23, 0x81, 0, 0, 0, 0, 0, 0, 0], CedLanes::Three);
        let reading = Scdc::new(sim).read_ced(CedLanes::Three).await.unwrap();
        assert_eq!(reading.counters.lane0, Some(CedCount::new(0x0123)));
    }

    #[pollster::test]
    async fn ced_each_lane_from_its_own_registers() {
        // A distinct, asymmetric low/high pair per lane, so a swapped byte or lane shows.
        let block = [0x02, 0x83, 0x04, 0x85, 0x06, 0x87, 0, 0x08, 0x89];
        let reading = Scdc::new(with_block(block, CedLanes::Four))
            .read_ced(CedLanes::Four)
            .await
            .unwrap();
        assert_eq!(
            reading.counters,
            CedCounters::new(
                Some(CedCount::new(0x0302)),
                Some(CedCount::new(0x0504)),
                Some(CedCount::new(0x0706)),
                Some(CedCount::new(0x0908)),
            )
        );
        assert_eq!(reading.warnings(), []);
    }

    #[pollster::test]
    async fn ced_lane3_is_not_a_count_in_3_lane_mode() {
        // The checksum at 0x56 is not lane 3; lane 3 is reported, but not in use.
        let block = [0, 0, 0, 0, 0, 0, 0, 0x01, 0x80];
        let reading = Scdc::new(with_block(block, CedLanes::Three))
            .read_ced(CedLanes::Three)
            .await
            .unwrap();
        assert_eq!(reading.counters.lane3, None);
        // The variants are `#[non_exhaustive]`: outside culvert they are matched, as a
        // caller does, not built.
        assert!(matches!(
            reading.warnings(),
            [CedWarning::UnusedLaneReported { count, .. }] if *count == CedCount::new(1)
        ));
    }

    #[pollster::test]
    async fn ced_checksum_mismatch_is_a_warning_with_the_counts() {
        let mut sim = with_block([0x34, 0x81, 0, 0, 0, 0, 0, 0, 0], CedLanes::Three);
        let checksum = sim.get(0x56);
        sim.set(0x56, checksum ^ 0x01);
        let reading = Scdc::new(sim).read_ced(CedLanes::Three).await.unwrap();
        assert_eq!(reading.counters.lane0, Some(CedCount::new(0x0134)));
        assert!(matches!(
            reading.warnings(),
            [CedWarning::ChecksumMismatch { expected, found, .. }]
                if *expected == checksum && *found == checksum ^ 0x01
        ));
    }

    #[pollster::test]
    async fn ced_lanes_from_the_link_rate() {
        let block = [0, 0, 0, 0, 0, 0, 0, 0x08, 0x89];
        let reading = Scdc::new(with_block(block, CedLanes::Four))
            .read_ced(FrlRate::Rate12Gbps4Lanes.into())
            .await
            .unwrap();
        assert_eq!(reading.counters.lane3, Some(CedCount::new(0x0908)));
        assert_eq!(reading.warnings(), []);
    }

    #[pollster::test]
    async fn ced_transport_error() {
        // The block is one read.
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_ced(CedLanes::Three)
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            Scdc::new(TestTransport::failing_after(1))
                .read_ced(CedLanes::Three)
                .await
                .is_ok()
        );
    }

    #[pollster::test]
    async fn read_rs_correction_reads_low_and_high_in_one_block() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.read_rs_correction().await.unwrap();
        assert_eq!(
            scdc.into_transport().log()[..2],
            [Some(Op::ReadBlock(0x59, 2)), None]
        );
    }

    #[pollster::test]
    async fn rs_correction_valid_and_invalid() {
        let mut sim = TestTransport::new();
        sim.set(0x59, 0x34);
        sim.set(0x5A, 0x81); // valid, upper counter bits = 0x01
        assert_eq!(
            Scdc::new(sim)
                .read_rs_correction()
                .await
                .unwrap()
                .map(|c| c.value()),
            Some(0x0134)
        );

        let mut sim = TestTransport::new();
        sim.set(0x59, 0xFF);
        sim.set(0x5A, 0x7F); // validity bit clear
        assert_eq!(Scdc::new(sim).read_rs_correction().await.unwrap(), None);
    }

    #[pollster::test]
    async fn rs_correction_transport_error() {
        // The count is one block read.
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_rs_correction()
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            Scdc::new(TestTransport::failing_after(1))
                .read_rs_correction()
                .await
                .is_ok()
        );
    }
}
