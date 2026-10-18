use hdmi_hal_async::scdc::ScdcTransport;

use crate::ScdcError;
use crate::{CedCounters, RsCorrectionCount};
use culvert::codec;

use super::Scdc;

impl<T: ScdcTransport> Scdc<T> {
    /// Reads per-lane character error counts from `ERR_DET` registers (0x50–0x55 for
    /// lanes 0–2, 0x57–0x58 for lane 3; 0x56 is the CED checksum).
    ///
    /// Each lane's counter is decoded from a low/high byte pair. The high byte's
    /// bit 7 is a validity flag; if it is not set the lane's counter is `None`.
    ///
    /// Each byte is a separate read, so a counter the sink updates between its low and
    /// high byte can be torn, and the CED checksum (0x56) is not verified. Treat the
    /// counts as diagnostics.
    pub async fn read_ced(&mut self) -> Result<CedCounters, ScdcError<T::Error>> {
        let mut bytes = [0; 8];
        for (byte, reg) in bytes.iter_mut().zip(codec::CED_REGISTERS) {
            *byte = self
                .transport
                .read(reg)
                .await
                .map_err(ScdcError::Transport)?;
        }
        Ok(codec::decode_ced(bytes))
    }

    /// Reads the Reed-Solomon correction count from `RS_Correction_L/H` (0x59/0x5A).
    ///
    /// Returns `None` if the high byte's validity bit (bit 7) is not set. The count is
    /// maintained by the sink in FRL mode; `UpdateFlags::rsed_update` signals a change.
    ///
    /// The two bytes are separate reads, so a count the sink updates between them can be
    /// torn. Treat it as a diagnostic.
    pub async fn read_rs_correction(
        &mut self,
    ) -> Result<Option<RsCorrectionCount>, ScdcError<T::Error>> {
        let low = self
            .transport
            .read(codec::RS_CORRECTION_L)
            .await
            .map_err(ScdcError::Transport)?;
        let high = self
            .transport
            .read(codec::RS_CORRECTION_H)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_rs_correction(low, high))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::TestTransport;
    use crate::CedCount;

    #[pollster::test]
    async fn ced_validity_bit_required() {
        // High byte with bit 7 clear → None regardless of counter value.
        let mut sim = TestTransport::new();
        sim.set(0x50, 0xFF);
        sim.set(0x51, 0x7F); // valid bit clear
        let ced = Scdc::new(sim).read_ced().await.unwrap();
        assert_eq!(ced.lane0, None);
    }

    #[pollster::test]
    async fn ced_counter_validity_bit_stripped() {
        // High byte: bit 7 set (valid), counter bits = 0x01; low byte = 0x23.
        let mut sim = TestTransport::new();
        sim.set(0x50, 0x23);
        sim.set(0x51, 0x81);
        let ced = Scdc::new(sim).read_ced().await.unwrap();
        assert_eq!(ced.lane0, Some(CedCount::new(0x0123)));
    }

    #[pollster::test]
    async fn ced_lane3_independent() {
        let mut sim = TestTransport::new();
        sim.set(0x56, 0xAA); // CED checksum, must not be read as lane 3
        sim.set(0x57, 0x01);
        sim.set(0x58, 0x80); // lane3 valid, count = 1
        let ced = Scdc::new(sim).read_ced().await.unwrap();
        assert_eq!(ced.lane0, None);
        assert_eq!(ced.lane3.map(|c| c.value()), Some(0x0001));
    }

    #[pollster::test]
    async fn transport_error_on_any_read() {
        for n in 0..8 {
            assert!(
                Scdc::new(TestTransport::failing_after(n))
                    .read_ced()
                    .await
                    .is_err()
            );
        }
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
        for n in 0..2 {
            assert!(
                Scdc::new(TestTransport::failing_after(n))
                    .read_rs_correction()
                    .await
                    .is_err()
            );
        }
    }
}
