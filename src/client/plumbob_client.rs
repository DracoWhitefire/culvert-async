//! `plumbob_async::ScdcClient` implementation for [`Scdc`].
//!
//! Each trait method awaits one culvert-async method. The conversions from culvert's
//! register types to plumbob's are culvert's own (its `plumbob` feature, which this
//! feature enables), so the sync and async adapters convert identically.

use hdmi_hal_async::scdc::ScdcTransport;
use plumbob_async::ScdcClient;

use crate::{Config0, ScdcError};

use super::Scdc;

/// Typed SCDC access for plumbob-async's link training.
///
/// `Scdc` does not wait between calls. plumbob's state machine polls `read_flt_ready` and
/// `read_update_flags` in a loop whose limits assume one poll every 2 ms by default; await
/// that interval inside those two methods, in an `ScdcClient` of your own that wraps
/// `Scdc<T>` and forwards the other six (`doc/architecture.md` shows one). A deadline or
/// cancel signal belongs there too, following plumbob's "Stopping early" rule. Not in the
/// transport: there a wait applies to every register access, and a transport cannot tell
/// the polled reads from LTS:L's.
#[cfg_attr(docsrs, doc(cfg(feature = "plumbob-async")))]
impl<T: ScdcTransport> ScdcClient for Scdc<T> {
    type Error = ScdcError<T::Error>;

    async fn read_flt_ready(&mut self) -> Result<bool, Self::Error> {
        Ok(Scdc::read_status_flags(self).await?.flt_ready)
    }

    async fn read_update_flags(&mut self) -> Result<plumbob_async::UpdateFlags, Self::Error> {
        Scdc::read_update_flags(self).await.map(Into::into)
    }

    async fn clear_update_flags(
        &mut self,
        flags: plumbob_async::ClearableUpdateFlags,
    ) -> Result<(), Self::Error> {
        Scdc::clear_update_flags(self, flags.into()).await
    }

    async fn read_ltp_requests(&mut self) -> Result<plumbob_async::LtpRequests, Self::Error> {
        Scdc::read_ltp_requests(self).await.map(Into::into)
    }

    async fn read_source_test_config(
        &mut self,
    ) -> Result<plumbob_async::SourceTestConfig, Self::Error> {
        Scdc::read_source_test_config(self).await.map(Into::into)
    }

    async fn write_config_0_defaults(&mut self) -> Result<(), Self::Error> {
        Scdc::write_config_0(self, Config0::default()).await
    }

    async fn write_frl_config(
        &mut self,
        config: plumbob_async::FrlConfig,
    ) -> Result<(), Self::Error> {
        Scdc::write_frl_config(self, config.into()).await
    }

    async fn read_ced(&mut self) -> Result<plumbob_async::CedCounters, Self::Error> {
        Scdc::read_ced(self).await.map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::TestTransport;
    use crate::{ProtocolError, ScdcError};
    use display_types::HdmiForumFrl;
    use plumbob_async::{
        ClearableUpdateFlags, FfeLevels, FrlConfig, LtpReq, LtpRequests, ScdcClient,
        SourceTestConfig, UpdateFlags,
    };

    fn scdc_with(regs: &[(u8, u8)]) -> Scdc<TestTransport> {
        let mut t = TestTransport::new();
        for &(addr, value) in regs {
            t.set(addr, value);
        }
        Scdc::new(t)
    }

    // --- read_flt_ready: Status_Flags_0 (0x40) bit 6

    #[pollster::test]
    async fn read_flt_ready_reads_status_flags_0_bit_6() {
        assert!(
            ScdcClient::read_flt_ready(&mut scdc_with(&[(0x40, 0x40)]))
                .await
                .unwrap()
        );
        assert!(
            !ScdcClient::read_flt_ready(&mut scdc_with(&[(0x40, 0xBF)]))
                .await
                .unwrap()
        );
    }

    // --- read_update_flags: Update_0 (0x10), all seven flags

    #[pollster::test]
    async fn read_update_flags_maps_every_flag() {
        let read = |byte| {
            pollster::block_on(ScdcClient::read_update_flags(&mut scdc_with(&[(
                0x10, byte,
            )])))
            .unwrap()
        };
        let flag = |n: usize| {
            let b = |i| i == n;
            UpdateFlags::new(b(0), b(1), b(2), b(3), b(4), b(5), b(6))
        };
        for n in 0..7 {
            assert_eq!(read(1 << n), flag(n), "bit {n}");
        }
        assert_eq!(
            read(0x7F),
            UpdateFlags::new(true, true, true, true, true, true, true)
        );
        // Bit 7 is reserved.
        assert_eq!(read(0x80), UpdateFlags::default());
    }

    // --- clear_update_flags: write 1 to clear, only the given flags

    #[pollster::test]
    async fn clear_update_flags_writes_each_given_bit() {
        let clear = |flags| {
            let mut scdc = Scdc::new(TestTransport::new());
            pollster::block_on(ScdcClient::clear_update_flags(&mut scdc, flags)).unwrap();
            scdc.into_transport().get(0x10)
        };
        // Bit order without RR_Test (bit 2), which has no field.
        for (n, bit) in [0, 1, 3, 4, 5, 6].into_iter().enumerate() {
            let b = |i| i == n;
            let flags = ClearableUpdateFlags::new(b(0), b(1), b(2), b(3), b(4), b(5));
            assert_eq!(clear(flags), 1 << bit, "bit {bit}");
        }
        assert_eq!(clear(ClearableUpdateFlags::default()), 0x00);
    }

    // --- read_ltp_requests: Status_Flags_1/2 (0x41/0x42), one nibble per lane

    #[pollster::test]
    async fn read_ltp_requests_maps_each_lane() {
        let mut scdc = scdc_with(&[(0x41, 0x65), (0x42, 0x87)]);
        assert_eq!(
            ScdcClient::read_ltp_requests(&mut scdc).await.unwrap(),
            LtpRequests::new(LtpReq::Lfsr0, LtpReq::Lfsr1, LtpReq::Lfsr2, LtpReq::Lfsr3)
        );
    }

    #[pollster::test]
    async fn read_ltp_requests_maps_every_request_value() {
        let cases = [
            (0x0, LtpReq::None),
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
        ];
        for (nibble, expected) in cases {
            let requests = ScdcClient::read_ltp_requests(&mut scdc_with(&[(0x41, nibble)]))
                .await
                .unwrap();
            assert_eq!(requests.lane0, expected, "{nibble:#x}");
        }
    }

    #[pollster::test]
    async fn read_ltp_requests_passes_undefined_values_on() {
        let requests = ScdcClient::read_ltp_requests(&mut scdc_with(&[(0x42, 0x90)]))
            .await
            .unwrap();
        assert_eq!(requests.lane3, LtpReq::Reserved(0x9));
        assert_eq!(requests.lane2, LtpReq::None);
    }

    // --- read_source_test_config: Source_Test_Configuration (0x35), all six bits

    #[pollster::test]
    async fn read_source_test_config_maps_every_bit() {
        let read = |byte| {
            pollster::block_on(ScdcClient::read_source_test_config(&mut scdc_with(&[(
                0x35, byte,
            )])))
            .unwrap()
        };
        // Bits 1–3 and 5–7, in the order of the fields; bits 0 and 4 are reserved.
        for (n, bit) in [1, 2, 3, 5, 6, 7].into_iter().enumerate() {
            let b = |i| i == n;
            let expected = SourceTestConfig::new(b(0), b(1), b(2), b(3), b(4), b(5));
            assert_eq!(read(1 << bit), expected, "bit {bit}");
        }
        assert_eq!(read(0x11), SourceTestConfig::default());
    }

    // --- write_config_0_defaults: Config_0 (0x30) cleared

    #[pollster::test]
    async fn write_config_0_defaults_clears_config_0() {
        let mut scdc = scdc_with(&[(0x30, 0xFF)]);
        ScdcClient::write_config_0_defaults(&mut scdc)
            .await
            .unwrap();
        assert_eq!(scdc.into_transport().get(0x30), 0x00);
    }

    // --- write_frl_config: Config_1 (0x31), rate in bits 3:0, FFE levels in bits 7:4

    #[pollster::test]
    async fn write_frl_config_encodes_rate_and_ffe_levels() {
        let mut scdc = Scdc::new(TestTransport::new());
        ScdcClient::write_frl_config(
            &mut scdc,
            FrlConfig::new(HdmiForumFrl::Rate12Gbps4Lanes, FfeLevels::new(3).unwrap()),
        )
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x31), 0x36);
    }

    #[pollster::test]
    async fn write_frl_config_turns_frl_off() {
        let mut scdc = scdc_with(&[(0x31, 0x36)]);
        ScdcClient::write_frl_config(
            &mut scdc,
            FrlConfig::new(HdmiForumFrl::NotSupported, FfeLevels::default()),
        )
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x31), 0x00);
    }

    #[pollster::test]
    async fn write_frl_config_keeps_culverts_rate_check() {
        // plumbob limits levels per rate itself; culvert still rejects a level above it.
        let mut scdc = Scdc::new(TestTransport::new());
        let result = ScdcClient::write_frl_config(
            &mut scdc,
            FrlConfig::new(HdmiForumFrl::Rate6Gbps4Lanes, FfeLevels::MAX),
        )
        .await;
        assert!(matches!(
            result,
            Err(ScdcError::Protocol(ProtocolError::FfeLevelsOutOfRange {
                levels: 7,
                ..
            }))
        ));
    }

    // --- read_ced: ERR_DET, validity bit 7 of each high byte

    #[pollster::test]
    async fn read_ced_maps_each_lane() {
        let mut scdc = scdc_with(&[
            (0x50, 0x34),
            (0x51, 0x92),
            (0x52, 0x01),
            (0x53, 0x00),
            (0x54, 0xFF),
            (0x55, 0xFF),
            (0x57, 0x00),
            (0x58, 0x80),
        ]);
        let ced = ScdcClient::read_ced(&mut scdc).await.unwrap();
        assert_eq!(ced.lane0.map(|c| c.value()), Some(0x1234));
        assert_eq!(ced.lane1, None);
        assert_eq!(ced.lane2.map(|c| c.value()), Some(0x7FFF));
        assert_eq!(ced.lane3.map(|c| c.value()), Some(0));
    }

    // --- transport errors

    #[pollster::test]
    async fn transport_errors_are_returned() {
        let fails = || Scdc::new(TestTransport::failing_after(0));
        assert!(matches!(
            ScdcClient::read_flt_ready(&mut fails()).await,
            Err(ScdcError::Transport(()))
        ));
        assert!(
            ScdcClient::read_update_flags(&mut fails())
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            ScdcClient::clear_update_flags(&mut fails(), ClearableUpdateFlags::default())
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            ScdcClient::read_ltp_requests(&mut fails())
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            ScdcClient::read_source_test_config(&mut fails())
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            ScdcClient::write_config_0_defaults(&mut fails())
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            ScdcClient::write_frl_config(
                &mut fails(),
                FrlConfig::new(HdmiForumFrl::Rate6Gbps4Lanes, FfeLevels::default())
            )
            .await
            .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            ScdcClient::read_ced(&mut fails())
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
    }
}
