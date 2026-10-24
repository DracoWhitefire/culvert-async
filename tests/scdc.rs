//! Integration tests for the `Scdc` client against a simulated transport, through the
//! crate's public API: culvert's `tests/scdc.rs` with `async`/`.await`.
//!
//! `SimulatedScdc` backs the transport with a `[u8; 256]` register array.
//! Tests pre-load register state, run client operations, and assert on both
//! returned values and register contents.

use core::convert::Infallible;
use culvert_async::{
    CedLanes, ClearableUpdateFlags, Config0, FfeLevels, FrlConfig, FrlRate, LtpReq, Scdc,
    ScdcTransport, SendScdcTransport, TmdsConfig,
};
use hdmi_hal_async::scdc::read_block_bytewise;

// ── simulated transport ───────────────────────────────────────────────────────

struct SimulatedScdc {
    regs: [u8; 256],
}

impl SimulatedScdc {
    fn new() -> Self {
        Self { regs: [0u8; 256] }
    }

    fn set(&mut self, addr: u8, val: u8) {
        self.regs[addr as usize] = val;
    }

    fn get(&self, addr: u8) -> u8 {
        self.regs[addr as usize]
    }
}

impl ScdcTransport for SimulatedScdc {
    type Error = Infallible;

    async fn read(&self, reg: u8) -> Result<u8, Infallible> {
        Ok(self.regs[reg as usize])
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), Infallible> {
        self.regs[reg as usize] = value;
        Ok(())
    }

    async fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Infallible> {
        // A register array has no bus transaction to read in one go: one `read` per byte.
        read_block_bytewise(self, reg, buf).await
    }
}

// ── version ───────────────────────────────────────────────────────────────────

#[pollster::test]
async fn read_sink_version() {
    let mut transport = SimulatedScdc::new();
    transport.set(0x01, 0xA5);
    let mut scdc = Scdc::new(transport);

    assert_eq!(scdc.read_sink_version().await.unwrap(), 0xA5);
}

#[pollster::test]
async fn write_source_version() {
    let mut scdc = Scdc::new(SimulatedScdc::new());
    scdc.write_source_version(0x5A).await.unwrap();

    assert_eq!(scdc.into_transport().get(0x02), 0x5A);
}

// ── scrambling ────────────────────────────────────────────────────────────────

#[pollster::test]
async fn write_tmds_config_encodes_bits() {
    let mut scdc = Scdc::new(SimulatedScdc::new());
    scdc.write_tmds_config(TmdsConfig {
        scrambling_enable: true,
        high_tmds_clock_ratio: true,
    })
    .await
    .unwrap();

    // scrambling_enable = bit 0, high_tmds_clock_ratio = bit 1
    assert_eq!(scdc.into_transport().get(0x20), 0x03);
}

#[pollster::test]
async fn read_scrambler_status_active() {
    let mut transport = SimulatedScdc::new();
    transport.set(0x21, 0x01);
    let mut scdc = Scdc::new(transport);

    assert!(
        scdc.read_scrambler_status()
            .await
            .unwrap()
            .scrambling_active
    );
}

#[pollster::test]
async fn read_scrambler_status_inactive() {
    let mut scdc = Scdc::new(SimulatedScdc::new());
    assert!(
        !scdc
            .read_scrambler_status()
            .await
            .unwrap()
            .scrambling_active
    );
}

// ── FRL training ─────────────────────────────────────────────────────────────

#[pollster::test]
async fn write_frl_config_encodes_bits() {
    let mut scdc = Scdc::new(SimulatedScdc::new());
    scdc.write_frl_config(FrlConfig::new(
        FrlRate::Rate10Gbps4Lanes,  // discriminant 5 → bits[3:0]
        FfeLevels::new(3).unwrap(), // → bits[7:4]
    ))
    .await
    .unwrap();
    let transport = scdc.into_transport();
    assert_eq!(transport.get(0x31), 0x35);
    assert_eq!(transport.get(0x30), 0x00); // Config_0 is a different register
}

#[pollster::test]
async fn write_config_0_and_read_source_test_config() {
    let mut transport = SimulatedScdc::new();
    transport.set(0x35, 0x60); // FLT_No_Timeout (bit 5) | DSC_FRL_Max (bit 6)
    let mut scdc = Scdc::new(transport);

    scdc.write_config_0(Config0 {
        rr_enable: false,
        flt_no_retrain: true,
    })
    .await
    .unwrap();
    let test_config = scdc.read_source_test_config().await.unwrap();
    assert!(test_config.flt_no_timeout);
    assert!(test_config.dsc_frl_max);
    assert_eq!(scdc.into_transport().get(0x30), 0x02);
}

#[pollster::test]
async fn read_status_flags_decodes_registers() {
    let mut transport = SimulatedScdc::new();
    // clock_detected (bit 0) | ch0_locked (bit 1) | ch2_locked (bit 3) | flt_ready (bit 6)
    // → 0b0100_1011 = 0x4B
    transport.set(0x40, 0x4B);
    let mut scdc = Scdc::new(transport);

    let flags = scdc.read_status_flags().await.unwrap();
    assert!(flags.clock_detected);
    assert!(flags.ch0_locked);
    assert!(!flags.ch1_locked);
    assert!(flags.ch2_locked);
    assert!(!flags.ln3_locked);
    assert!(flags.flt_ready);
    assert!(!flags.dsc_decode_fail);
}

#[pollster::test]
async fn read_ltp_requests_decodes_lanes() {
    let mut transport = SimulatedScdc::new();
    transport.set(0x41, 0x76); // lane0 = LFSR1 (0x6), lane1 = LFSR2 (0x7)
    transport.set(0x42, 0xE0); // lane2 = none, lane3 = FFE change (0xE)
    let mut scdc = Scdc::new(transport);

    let requests = scdc.read_ltp_requests().await.unwrap();
    assert_eq!(requests.lane0, LtpReq::Lfsr1);
    assert_eq!(requests.lane1, LtpReq::Lfsr2);
    assert_eq!(requests.lane2, LtpReq::None);
    assert_eq!(requests.lane3, LtpReq::FfeChange);
}

#[pollster::test]
async fn read_ltp_requests_undefined_value() {
    let mut transport = SimulatedScdc::new();
    transport.set(0x42, 0xA0); // lane 3 = 0xA, undefined
    let mut scdc = Scdc::new(transport);

    assert_eq!(
        scdc.read_ltp_requests().await.unwrap().lane3,
        LtpReq::Reserved(0xA)
    );
}

#[pollster::test]
async fn read_update_flags_decodes_registers() {
    let mut transport = SimulatedScdc::new();
    transport.set(0x10, 0x33); // status_update | ced_update | frl_start | flt_update
    let mut scdc = Scdc::new(transport);

    let flags = scdc.read_update_flags().await.unwrap();
    assert!(flags.status_update);
    assert!(flags.ced_update);
    assert!(!flags.rr_test);
    assert!(!flags.source_test_update);
    assert!(flags.frl_start);
    assert!(flags.flt_update);
    assert!(!flags.rsed_update);
}

#[pollster::test]
async fn clear_update_flags_writes_w1c() {
    let mut scdc = Scdc::new(SimulatedScdc::new());

    // Clear only flt_update and frl_start.
    scdc.clear_update_flags(ClearableUpdateFlags::new(
        false, false, false, true, true, false,
    ))
    .await
    .unwrap();

    let transport = scdc.into_transport();
    assert_eq!(transport.get(0x10), 0x30); // frl_start (bit 4) | flt_update (bit 5)
    assert_eq!(transport.get(0x11), 0x00); // Update_1 untouched
}

// ── CED ───────────────────────────────────────────────────────────────────────

#[pollster::test]
async fn read_ced_decodes_valid_and_invalid_lanes() {
    let mut transport = SimulatedScdc::new();
    // Lane 0: valid, count = 0x0134
    transport.set(0x50, 0x34); // l0
    transport.set(0x51, 0x81); // h0: valid bit set, upper bits = 0x01
    // Lane 1: validity bit not set → None
    transport.set(0x52, 0xFF);
    transport.set(0x53, 0x00);
    // Lane 2: valid, count = 0x7FFF (max)
    transport.set(0x54, 0xFF); // l2
    transport.set(0x55, 0xFF); // h2: valid + all counter bits set
    // CED checksum sits between lane 2 and lane 3: 0x50–0x56 sum to zero.
    transport.set(0x56, 0x4E);
    // Lane 3: validity bit not set → None
    transport.set(0x57, 0x00);
    transport.set(0x58, 0x00);
    let mut scdc = Scdc::new(transport);

    let reading = scdc.read_ced(CedLanes::Three).await.unwrap();

    let ced = reading.counters;
    assert_eq!(ced.lane0.map(|c| c.value()), Some(0x0134));
    assert_eq!(ced.lane1, None);
    assert_eq!(ced.lane2.map(|c| c.value()), Some(0x7FFF));
    assert_eq!(ced.lane3, None);
    assert_eq!(reading.warnings(), []);
}

#[pollster::test]
async fn read_ced_all_invalid() {
    let mut scdc = Scdc::new(SimulatedScdc::new());
    let reading = scdc.read_ced(CedLanes::Four).await.unwrap();
    // All zero: no counts, and a valid checksum.
    assert_eq!(reading.warnings(), []);
    let ced = reading.counters;
    assert!(ced.lane0.is_none());
    assert!(ced.lane1.is_none());
    assert!(ced.lane2.is_none());
    assert!(ced.lane3.is_none());
}

// ── public API ────────────────────────────────────────────────────────────────

#[pollster::test]
async fn every_type_is_re_exported_at_the_crate_root() {
    // Each line names one re-export, so removing or hiding one fails to compile.
    let _: Option<culvert_async::CedCount> = None;
    let _: Option<culvert_async::CedCounters> = None;
    let _: Option<culvert_async::CedLanes> = None;
    let _: Option<culvert_async::CedReading> = None;
    let _: Option<culvert_async::CedWarning> = None;
    let _: Option<culvert_async::ClearableUpdateFlags> = None;
    let _: Option<culvert_async::Config0> = None;
    let _: Option<culvert_async::FfeLevels> = None;
    let _: Option<culvert_async::FrlConfig> = None;
    let _: Option<culvert_async::FrlRate> = None;
    let _: Option<culvert_async::LtpReq> = None;
    let _: Option<culvert_async::LtpRequests> = None;
    let _: Option<culvert_async::ProtocolError> = None;
    let _: Option<culvert_async::RsCorrectionCount> = None;
    let _: Option<culvert_async::ScdcError<()>> = None;
    let _: Option<culvert_async::ScramblerStatus> = None;
    let _: Option<culvert_async::SourceTestConfig> = None;
    let _: Option<culvert_async::StatusFlags> = None;
    let _: Option<culvert_async::TmdsConfig> = None;
    let _: Option<culvert_async::UpdateFlags> = None;
    let _: Option<Scdc<SimulatedScdc>> = None;
    // The traits, `ScdcTransport` and `SendScdcTransport`, are imported at the top.
}

// ── `Send` futures ────────────────────────────────────────────────────────────

/// `SimulatedScdc` behind the transport's `Send` twin.
struct SendSimulatedScdc(SimulatedScdc);

impl SendScdcTransport for SendSimulatedScdc {
    type Error = Infallible;

    async fn read(&self, reg: u8) -> Result<u8, Infallible> {
        ScdcTransport::read(&self.0, reg).await
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), Infallible> {
        ScdcTransport::write(&mut self.0, reg, value).await
    }

    async fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Infallible> {
        ScdcTransport::read_block(&self.0, reg, buf).await
    }
}

/// Passes `future` through, requiring it to be `Send`.
fn send<F: Send>(future: F) -> F {
    future
}

/// Runs every operation, each future passed through `send`: generic over a
/// `SendScdcTransport`, this compiles only if every operation's future is `Send`, so any of
/// them can run on a multi-threaded executor.
async fn every_operation<T: SendScdcTransport>(scdc: &mut Scdc<T>) {
    let _ = send(scdc.read_sink_version()).await;
    let _ = send(scdc.write_source_version(0x01)).await;
    let _ = send(scdc.write_tmds_config(TmdsConfig {
        scrambling_enable: true,
        high_tmds_clock_ratio: false,
    }))
    .await;
    let _ = send(scdc.read_scrambler_status()).await;
    let _ = send(scdc.write_frl_config(FrlConfig::new(
        FrlRate::Rate6Gbps4Lanes,
        FfeLevels::default(),
    )))
    .await;
    let _ = send(scdc.write_config_0(Config0::default())).await;
    let _ = send(scdc.read_source_test_config()).await;
    let _ = send(scdc.read_status_flags()).await;
    let _ = send(scdc.read_ltp_requests()).await;
    let _ = send(scdc.read_update_flags()).await;
    let _ = send(scdc.clear_update_flags(ClearableUpdateFlags::default())).await;
    let _ = send(scdc.read_ced(CedLanes::Three)).await;
    let _ = send(scdc.read_rs_correction()).await;
}

#[pollster::test]
async fn every_operation_is_send_over_a_send_transport() {
    let mut scdc = Scdc::new(SendSimulatedScdc(SimulatedScdc::new()));
    every_operation(&mut scdc).await;
    // The writes reached the registers.
    let transport = scdc.into_transport().0;
    assert_eq!(transport.get(0x02), 0x01);
    assert_eq!(transport.get(0x20), 0x01);
    assert_eq!(transport.get(0x31), 0x03);
}
