use hdmi_hal_async::scdc::ScdcTransport;

use crate::{ScdcError, ScramblerStatus, TmdsConfig};

use super::Scdc;
use super::address;

impl<T: ScdcTransport> Scdc<T> {
    /// Writes scrambling configuration to `TMDS_Config` (0x20).
    ///
    /// Sets `Scrambling_Enable` (bit 0) and `TMDS_Bit_Clock_Ratio` (bit 1).
    pub async fn write_tmds_config(
        &mut self,
        config: TmdsConfig,
    ) -> Result<(), ScdcError<T::Error>> {
        let byte = (config.scrambling_enable as u8) | ((config.high_tmds_clock_ratio as u8) << 1);
        self.transport
            .write(address::TMDS_CONFIG, byte)
            .await
            .map_err(ScdcError::Transport)
    }

    /// Reads scrambler acknowledgement from `Scrambler_Status` (0x21).
    ///
    /// Returns [`ScramblerStatus::scrambling_active`] set when the sink confirms
    /// that TMDS scrambling is active (bit 0).
    pub async fn read_scrambler_status(&mut self) -> Result<ScramblerStatus, ScdcError<T::Error>> {
        let byte = self
            .transport
            .read(address::SCRAMBLER_STATUS)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(ScramblerStatus::new(byte & 0x01 != 0))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::TestTransport;
    use crate::{ScramblerStatus, TmdsConfig};

    #[pollster::test]
    async fn tmds_config_scrambling_only() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: true,
            high_tmds_clock_ratio: false,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x01);
    }

    #[pollster::test]
    async fn tmds_config_clock_ratio_only() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: true,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x02);
    }

    #[pollster::test]
    async fn tmds_config_both_clear() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: false,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x00);
    }

    #[pollster::test]
    async fn scrambler_status_bit0_set() {
        let mut sim = TestTransport::new();
        sim.set(0x21, 0xFF); // all bits set; only bit 0 is defined
        assert_eq!(
            Scdc::new(sim).read_scrambler_status().await.unwrap(),
            ScramblerStatus::new(true)
        );
    }

    #[pollster::test]
    async fn scrambler_status_bit0_clear() {
        let mut sim = TestTransport::new();
        sim.set(0x21, 0xFE); // bit 0 clear, other bits set (reserved)
        assert_eq!(
            Scdc::new(sim).read_scrambler_status().await.unwrap(),
            ScramblerStatus::new(false)
        );
    }

    #[pollster::test]
    async fn transport_error_propagates() {
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .write_tmds_config(TmdsConfig {
                    scrambling_enable: false,
                    high_tmds_clock_ratio: false
                })
                .await
                .is_err()
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_scrambler_status()
                .await
                .is_err()
        );
    }
}
