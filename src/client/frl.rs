use hdmi_hal_async::scdc::ScdcTransport;

use crate::ScdcError;
use crate::{Config0, FrlConfig, LtpRequests, SourceTestConfig, StatusFlags};
use culvert::codec;

use super::Scdc;

impl<T: ScdcTransport> Scdc<T> {
    /// Writes FRL training configuration to `Config_1` (0x31).
    ///
    /// Encodes `FRL_Rate` into bits\[3:0\] and `FFE_Levels` into bits\[7:4\].
    ///
    /// Returns [`crate::ProtocolError::FfeLevelsOutOfRange`] without writing anything if
    /// the FFE levels exceed [`FfeLevels::max_for`](crate::FfeLevels::max_for) the
    /// requested rate.
    pub async fn write_frl_config(&mut self, config: FrlConfig) -> Result<(), ScdcError<T::Error>> {
        let byte = codec::encode_config_1(config).map_err(ScdcError::Protocol)?;
        self.transport
            .write(codec::CONFIG_1, byte)
            .await
            .map_err(ScdcError::Transport)
    }

    /// Writes `Config_0` (0x30): `RR_Enable` (bit 0) and `FLT_No_Retrain` (bit 1).
    pub async fn write_config_0(&mut self, config: Config0) -> Result<(), ScdcError<T::Error>> {
        self.transport
            .write(codec::CONFIG_0, codec::encode_config_0(config))
            .await
            .map_err(ScdcError::Transport)
    }

    /// Reads `Source_Test_Configuration` (0x35), written by the sink to instruct the
    /// source during compliance testing.
    pub async fn read_source_test_config(
        &mut self,
    ) -> Result<SourceTestConfig, ScdcError<T::Error>> {
        let byte = self
            .transport
            .read(codec::SOURCE_TEST_CONFIG)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_source_test_config(byte))
    }

    /// Reads `Status_Flags_0` (0x40): clock detection, lane lock, `FLT_Ready` and DSC
    /// decode failure.
    pub async fn read_status_flags(&mut self) -> Result<StatusFlags, ScdcError<T::Error>> {
        let flags0 = self
            .transport
            .read(codec::STATUS_FLAGS_0)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_status_flags(flags0))
    }

    /// Reads the per-lane link training pattern requests from `Status_Flags_1` (0x41,
    /// lanes 0–1) and `Status_Flags_2` (0x42, lanes 2–3).
    ///
    /// A value the HDMI 2.1 specification leaves undefined (0x9–0xD) is
    /// [`LtpReq::Reserved`](crate::LtpReq::Reserved), not an error.
    pub async fn read_ltp_requests(&mut self) -> Result<LtpRequests, ScdcError<T::Error>> {
        let flags1 = self
            .transport
            .read(codec::STATUS_FLAGS_1)
            .await
            .map_err(ScdcError::Transport)?;
        let flags2 = self
            .transport
            .read(codec::STATUS_FLAGS_2)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_ltp_requests(flags1, flags2))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::TestTransport;
    use crate::{Config0, FfeLevels, FrlConfig, LtpReq, SourceTestConfig, StatusFlags};
    use crate::{ProtocolError, ScdcError};
    use display_types::HdmiForumFrl;

    #[pollster::test]
    async fn frl_config_rate_field() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_frl_config(FrlConfig::new(
            HdmiForumFrl::Rate12Gbps4Lanes, // discriminant 6
            FfeLevels::default(),
        ))
        .await
        .unwrap();
        let t = scdc.into_transport();
        assert_eq!(t.get(0x31), 0x06);
        assert_eq!(t.get(0x30), 0x00); // Config_0 untouched
    }

    #[pollster::test]
    async fn frl_config_ffe_levels_field() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_frl_config(FrlConfig::new(
            HdmiForumFrl::Rate12Gbps4Lanes,
            FfeLevels::new(3).unwrap(), // → bits[7:4]
        ))
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x31), 0x36);
    }

    #[pollster::test]
    async fn ffe_levels_range() {
        assert_eq!(FfeLevels::new(7).map(FfeLevels::value), Some(7));
        assert_eq!(FfeLevels::new(8), None);
    }

    #[pollster::test]
    async fn ffe_levels_max_for_rates_up_to_12g() {
        for rate in [
            HdmiForumFrl::NotSupported,
            HdmiForumFrl::Rate3Gbps3Lanes,
            HdmiForumFrl::Rate6Gbps3Lanes,
            HdmiForumFrl::Rate6Gbps4Lanes,
            HdmiForumFrl::Rate8Gbps4Lanes,
            HdmiForumFrl::Rate10Gbps4Lanes,
            HdmiForumFrl::Rate12Gbps4Lanes,
        ] {
            assert_eq!(FfeLevels::max_for(rate).value(), 3, "{rate:?}");
        }
    }

    #[pollster::test]
    async fn frl_config_rejects_ffe_levels_above_rate_maximum() {
        let mut scdc = Scdc::new(TestTransport::new());
        let result = scdc
            .write_frl_config(FrlConfig::new(
                HdmiForumFrl::Rate12Gbps4Lanes,
                FfeLevels::new(4).unwrap(),
            ))
            .await;
        assert!(matches!(
            result,
            Err(ScdcError::Protocol(ProtocolError::FfeLevelsOutOfRange {
                rate: HdmiForumFrl::Rate12Gbps4Lanes,
                levels: 4
            }))
        ));
        // Nothing is written.
        assert_eq!(scdc.into_transport().get(0x31), 0x00);
    }

    #[pollster::test]
    async fn status_flags_all_zero() {
        let f = Scdc::new(TestTransport::new())
            .read_status_flags()
            .await
            .unwrap();
        assert_eq!(
            f,
            StatusFlags::new(false, false, false, false, false, false, false)
        );
    }

    #[pollster::test]
    async fn status_flags_individual_bits() {
        let read = |byte: u8| {
            let mut sim = TestTransport::new();
            sim.set(0x40, byte);
            pollster::block_on(Scdc::new(sim).read_status_flags()).unwrap()
        };
        assert!(read(0x01).clock_detected);
        assert!(read(0x02).ch0_locked);
        assert!(read(0x04).ch1_locked);
        assert!(read(0x08).ch2_locked);
        assert!(read(0x10).ln3_locked);
        assert!(read(0x40).flt_ready);
        assert!(read(0x80).dsc_decode_fail);
        // Bit 5 is not defined; setting it alone sets no field.
        assert_eq!(read(0x20), read(0x00));
    }

    #[pollster::test]
    async fn status_flags_reads_only_status_flags_0() {
        let mut sim = TestTransport::new();
        sim.set(0x41, 0xFF);
        sim.set(0x42, 0xFF);
        let f = Scdc::new(sim).read_status_flags().await.unwrap();
        assert!(!f.flt_ready && !f.clock_detected);
    }

    #[pollster::test]
    async fn ltp_requests_per_lane() {
        let mut sim = TestTransport::new();
        sim.set(0x41, 0x65); // lane0 = 0x5 (LFSR0), lane1 = 0x6 (LFSR1)
        sim.set(0x42, 0xF1); // lane2 = 0x1 (all ones), lane3 = 0xF (rate change)
        let r = Scdc::new(sim).read_ltp_requests().await.unwrap();
        assert_eq!(r.lane0, LtpReq::Lfsr0);
        assert_eq!(r.lane1, LtpReq::Lfsr1);
        assert_eq!(r.lane2, LtpReq::AllOnes);
        assert_eq!(r.lane3, LtpReq::RateChange);
        assert!(!r.all_trained());
    }

    #[pollster::test]
    async fn ltp_requests_all_defined_values() {
        for (nibble, req) in [
            (0x0u8, LtpReq::None),
            (0x1, LtpReq::AllOnes),
            (0x2, LtpReq::AllZeros),
            (0x3, LtpReq::NyquistClock),
            (0x4, LtpReq::DdeCompliance),
            (0x5, LtpReq::Lfsr0),
            (0x6, LtpReq::Lfsr1),
            (0x7, LtpReq::Lfsr2),
            (0x8, LtpReq::Lfsr3),
            (0xE, LtpReq::FfeChange),
            (0xF, LtpReq::RateChange),
        ] {
            let mut sim = TestTransport::new();
            sim.set(0x42, nibble << 4); // lane 3
            assert_eq!(Scdc::new(sim).read_ltp_requests().await.unwrap().lane3, req);
        }
    }

    #[pollster::test]
    async fn ltp_requests_undefined_values_are_reserved() {
        for nibble in 0x9u8..=0xD {
            let mut sim = TestTransport::new();
            sim.set(0x41, nibble); // lane 0
            let requests = Scdc::new(sim).read_ltp_requests().await.unwrap();
            assert_eq!(requests.lane0, LtpReq::Reserved(nibble));
            assert_eq!(requests.lane1, LtpReq::None);
        }
    }

    #[pollster::test]
    async fn ltp_requests_all_trained_when_zero() {
        assert!(
            Scdc::new(TestTransport::new())
                .read_ltp_requests()
                .await
                .unwrap()
                .all_trained()
        );
    }

    #[pollster::test]
    async fn config_0_bits() {
        for (config, expected) in [
            (Config0::default(), 0x00u8),
            (
                Config0 {
                    rr_enable: true,
                    flt_no_retrain: false,
                },
                0x01,
            ),
            (
                Config0 {
                    rr_enable: false,
                    flt_no_retrain: true,
                },
                0x02,
            ),
            (
                Config0 {
                    rr_enable: true,
                    flt_no_retrain: true,
                },
                0x03,
            ),
        ] {
            let mut scdc = Scdc::new(TestTransport::new());
            scdc.write_config_0(config).await.unwrap();
            assert_eq!(scdc.into_transport().get(0x30), expected, "{config:?}");
        }
    }

    #[pollster::test]
    async fn source_test_config_bits() {
        let read = |byte: u8| {
            let mut sim = TestTransport::new();
            sim.set(0x35, byte);
            pollster::block_on(Scdc::new(sim).read_source_test_config()).unwrap()
        };
        let fields = |c: SourceTestConfig| {
            [
                c.txffe_pre_shoot_only,
                c.txffe_de_emphasis_only,
                c.txffe_no_ffe,
                c.flt_no_timeout,
                c.dsc_frl_max,
                c.frl_max,
            ]
        };
        for (i, bit) in [1u8, 2, 3, 5, 6, 7].into_iter().enumerate() {
            let mut expected = [false; 6];
            expected[i] = true;
            assert_eq!(fields(read(1 << bit)), expected, "bit {bit}");
        }
        // Bits 0 and 4 are reserved and set no field.
        assert_eq!(read(0x11), read(0x00));
        assert_eq!(
            read(0x00),
            SourceTestConfig::new(false, false, false, false, false, false)
        );
    }

    #[pollster::test]
    async fn transport_error_propagates() {
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .write_frl_config(FrlConfig::new(
                    HdmiForumFrl::NotSupported,
                    FfeLevels::default(),
                ))
                .await
                .is_err()
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_status_flags()
                .await
                .is_err()
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .write_config_0(Config0::default())
                .await
                .is_err()
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_source_test_config()
                .await
                .is_err()
        );
        // First (Status_Flags_1) and second (Status_Flags_2) read of the LTP requests.
        for n in 0..2 {
            assert!(
                Scdc::new(TestTransport::failing_after(n))
                    .read_ltp_requests()
                    .await
                    .is_err()
            );
        }
    }
}
