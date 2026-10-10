use hdmi_hal_async::scdc::ScdcTransport;

use crate::ScdcError;
use crate::UpdateFlags;
use culvert::codec;

use super::Scdc;

impl<T: ScdcTransport> Scdc<T> {
    /// Reads update flags from `Update_0` (0x10).
    pub async fn read_update_flags(&mut self) -> Result<UpdateFlags, ScdcError<T::Error>> {
        let update_0 = self
            .transport
            .read(codec::UPDATE_0)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_update_flags(update_0))
    }

    /// Clears the specified update flags in `Update_0` (0x10).
    ///
    /// Each flag set to `true` in `flags` is cleared (write-1-to-clear). Flags
    /// set to `false` are left unchanged.
    ///
    /// Returns [`crate::ProtocolError::RrTestNotClearable`], writing nothing, if
    /// `flags.rr_test` is set: culvert treats Read Request Test as a flag the source must
    /// not clear.
    pub async fn clear_update_flags(
        &mut self,
        flags: UpdateFlags,
    ) -> Result<(), ScdcError<T::Error>> {
        let byte = codec::encode_clear_update_flags(flags).map_err(ScdcError::Protocol)?;
        self.transport
            .write(codec::UPDATE_0, byte)
            .await
            .map_err(ScdcError::Transport)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::TestTransport;
    use crate::{ProtocolError, ScdcError, UpdateFlags};

    fn read(byte: u8) -> UpdateFlags {
        let mut sim = TestTransport::new();
        sim.set(0x10, byte);
        pollster::block_on(Scdc::new(sim).read_update_flags()).unwrap()
    }

    #[pollster::test]
    async fn update_flags_individual_bits() {
        assert!(read(0x01).status_update);
        assert!(read(0x02).ced_update);
        assert!(read(0x04).rr_test);
        assert!(read(0x08).source_test_update);
        assert!(read(0x10).frl_start);
        assert!(read(0x20).flt_update);
        assert!(read(0x40).rsed_update);
        assert_eq!(
            read(0x00),
            UpdateFlags::new(false, false, false, false, false, false, false)
        );
    }

    #[pollster::test]
    async fn update_1_is_not_read() {
        let mut sim = TestTransport::new();
        sim.set(0x11, 0xFF);
        assert_eq!(
            Scdc::new(sim).read_update_flags().await.unwrap(),
            UpdateFlags::new(false, false, false, false, false, false, false)
        );
    }

    #[pollster::test]
    async fn clear_update_flags_w1c() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.clear_update_flags(UpdateFlags::new(true, true, false, true, true, true, true))
            .await
            .unwrap();
        let t = scdc.into_transport();
        assert_eq!(t.get(0x10), 0x7B); // every flag except RR_Test (bit 2)
        assert_eq!(t.get(0x11), 0x00);
    }

    #[pollster::test]
    async fn clear_update_flags_rejects_rr_test_and_writes_nothing() {
        let mut sim = TestTransport::new();
        sim.set(0x10, 0xAA); // a sentinel: any write would replace it
        let mut scdc = Scdc::new(sim);
        // Even with other flags to clear, nothing is written.
        let result = scdc
            .clear_update_flags(UpdateFlags::new(
                true, false, true, false, false, true, false,
            ))
            .await;
        assert!(matches!(
            result,
            Err(ScdcError::Protocol(ProtocolError::RrTestNotClearable))
        ));
        assert_eq!(scdc.into_transport().get(0x10), 0xAA);
    }

    #[pollster::test]
    async fn clear_update_flags_partial() {
        let mut scdc = Scdc::new(TestTransport::new());
        // Clear only flt_update, as the training loop does after reading LTP requests.
        scdc.clear_update_flags(UpdateFlags::new(
            false, false, false, false, false, true, false,
        ))
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x10), 0x20);
    }

    #[pollster::test]
    async fn transport_error_propagates() {
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_update_flags()
                .await
                .is_err()
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .clear_update_flags(UpdateFlags::new(
                    false, false, false, false, false, false, false
                ))
                .await
                .is_err()
        );
    }
}
