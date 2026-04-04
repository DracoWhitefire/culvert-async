use hdmi_hal_async::scdc::ScdcTransport;

use crate::{FrlConfig, LtpReq, ProtocolError, ScdcError, StatusFlags};

use super::address;
use super::Scdc;

impl<T: ScdcTransport> Scdc<T> {
    /// Writes FRL training configuration to `Config_0` (0x30).
    ///
    /// Encodes `FRL_Rate` into bits\[3:0\], `DSC_FRL_Max` into bit\[4\], and
    /// `FFE_Levels` into bits\[7:5\].
    pub async fn write_frl_config(&mut self, config: FrlConfig) -> Result<(), ScdcError<T::Error>> {
        let byte = (config.frl_rate as u8)
            | ((config.dsc_frl_max as u8) << 4)
            | ((config.ffe_levels as u8) << 5);
        self.transport
            .write(address::CONFIG_0, byte)
            .await
            .map_err(ScdcError::Transport)
    }

    /// Reads FRL status from `Status_Flags_0` (0x40) and `Status_Flags_1` (0x41).
    ///
    /// Returns [`crate::ProtocolError::UnknownLtpReq`] if the sink reports an
    /// LTP request value not defined by the HDMI 2.1 specification.
    pub async fn read_status_flags(&mut self) -> Result<StatusFlags, ScdcError<T::Error>> {
        let flags0 = self
            .transport
            .read(address::STATUS_FLAGS_0)
            .await
            .map_err(ScdcError::Transport)?;
        let flags1 = self
            .transport
            .read(address::STATUS_FLAGS_1)
            .await
            .map_err(ScdcError::Transport)?;

        let ltp_req = match (flags1 >> 4) & 0x0F {
            0 => LtpReq::None,
            1 => LtpReq::Lfsr0,
            2 => LtpReq::Lfsr1,
            3 => LtpReq::Lfsr2,
            4 => LtpReq::Lfsr3,
            raw => return Err(ScdcError::Protocol(ProtocolError::UnknownLtpReq(raw))),
        };

        Ok(StatusFlags::new(
            flags0 & 0x01 != 0,
            flags0 & 0x02 != 0,
            flags0 & 0x04 != 0,
            flags0 & 0x08 != 0,
            flags0 & 0x10 != 0,
            flags0 & 0x20 != 0,
            flags0 & 0x40 != 0,
            flags1 & 0x01 != 0,
            ltp_req,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::TestTransport;
    use crate::{FfeLevels, FrlConfig, FrlRate, LtpReq, ProtocolError, ScdcError, StatusFlags};

    #[pollster::test]
    async fn frl_config_rate_field() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_frl_config(FrlConfig {
            frl_rate: FrlRate::Rate12Gbps4Lanes, // discriminant 6
            dsc_frl_max: false,
            ffe_levels: FfeLevels::Ffe0,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x30), 0x06);
    }

    #[pollster::test]
    async fn frl_config_dsc_frl_max_field() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_frl_config(FrlConfig {
            frl_rate: FrlRate::NotSupported,
            dsc_frl_max: true,
            ffe_levels: FfeLevels::Ffe0,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x30), 0x10);
    }

    #[pollster::test]
    async fn frl_config_ffe_levels_field() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_frl_config(FrlConfig {
            frl_rate: FrlRate::NotSupported,
            dsc_frl_max: false,
            ffe_levels: FfeLevels::Ffe7, // discriminant 7 → bits[7:5] = 0b111 = 0xE0
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x30), 0xE0);
    }

    #[pollster::test]
    async fn status_flags_all_zero() {
        let mut scdc = Scdc::new(TestTransport::new());
        assert_eq!(
            scdc.read_status_flags().await.unwrap(),
            StatusFlags::new(false, false, false, false, false, false, false, false, LtpReq::None)
        );
    }

    #[pollster::test]
    async fn status_flags_ltp_req_variants() {
        for (nibble, expected) in [
            (0u8, LtpReq::None),
            (1, LtpReq::Lfsr0),
            (2, LtpReq::Lfsr1),
            (3, LtpReq::Lfsr2),
            (4, LtpReq::Lfsr3),
        ] {
            let mut sim = TestTransport::new();
            sim.set(0x41, nibble << 4);
            assert_eq!(
                Scdc::new(sim).read_status_flags().await.unwrap().ltp_req,
                expected
            );
        }
    }

    #[pollster::test]
    async fn status_flags_unknown_ltp_req() {
        for nibble in 5u8..=15 {
            let mut sim = TestTransport::new();
            sim.set(0x41, nibble << 4);
            assert!(matches!(
                Scdc::new(sim).read_status_flags().await,
                Err(ScdcError::Protocol(ProtocolError::UnknownLtpReq(n))) if n == nibble
            ));
        }
    }

    #[pollster::test]
    async fn status_flags_all_flags0_bits_set() {
        let mut sim = TestTransport::new();
        sim.set(0x40, 0x7F); // clock_detected | cable_connected | ch0–ch3_locked | flt_ready
        sim.set(0x41, 0x01); // frl_start
        let f = Scdc::new(sim).read_status_flags().await.unwrap();
        assert!(f.clock_detected && f.cable_connected);
        assert!(f.ch0_locked && f.ch1_locked && f.ch2_locked && f.ch3_locked);
        assert!(f.flt_ready && f.frl_start);
        assert_eq!(f.ltp_req, LtpReq::None);
    }

    #[pollster::test]
    async fn transport_error_propagates() {
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .write_frl_config(FrlConfig {
                    frl_rate: FrlRate::NotSupported,
                    dsc_frl_max: false,
                    ffe_levels: FfeLevels::Ffe0,
                })
                .await
                .is_err()
        );
        // First read of status flags fails.
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_status_flags()
                .await
                .is_err()
        );
        // Second read (Status_Flags_1) fails.
        assert!(
            Scdc::new(TestTransport::failing_after(1))
                .read_status_flags()
                .await
                .is_err()
        );
    }
}
