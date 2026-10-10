//! End-to-end tests for the `plumbob-async` feature: plumbob's link training state
//! machine, driven by plumbob-async, trains through `Scdc<T>` against a sink simulated at
//! the SCDC register level.
//!
//! The sink reacts to the registers the way a real one does, so these tests check the
//! whole chain — plumbob's sequencing, culvert's encoding and decoding, and the register
//! bits in between.
#![cfg(feature = "plumbob-async")]

use core::cell::RefCell;
use core::convert::Infallible;

use culvert_async::Scdc;
use display_types::cea861::hdmi_forum::HdmiForumFrl;
use hdmi_hal_async::phy::HdmiPhy;
use hdmi_hal_async::scdc::ScdcTransport;
use hdmi_hal_async::{EqParams, FrlOutput, LanePatterns, LtpPattern};
use plumbob_async::{FallbackReason, FfeLevels, FrlTrainer, TrainingConfig, TrainingOutcome};

const UPDATE_0: u8 = 0x10;
const CONFIG_0: u8 = 0x30;
const CONFIG_1: u8 = 0x31;
const SOURCE_TEST_CONFIG: u8 = 0x35;
const STATUS_FLAGS_0: u8 = 0x40;
const STATUS_FLAGS_1: u8 = 0x41;
const STATUS_FLAGS_2: u8 = 0x42;

const SOURCE_TEST_UPDATE: u8 = 0x08;
const FRL_START: u8 = 0x10;
const FLT_UPDATE: u8 = 0x20;
const FLT_READY: u8 = 0x40;

// ── simulated sink ────────────────────────────────────────────────────────────

/// One set of LTS:3 requests: the four lane nibbles, posted with `FLT_update` after
/// `after_polls` reads of `Update_0`.
struct Round {
    after_polls: u32,
    lanes: [u8; 4],
}

struct Sink {
    regs: [u8; 256],
    flt_ready_after: Option<u32>,
    flt_ready_polls: u32,
    rounds: Vec<Round>,
    next_round: usize,
    frl_start_after: Option<u32>,
    /// Training runs while `Config_1` holds an FRL rate.
    configured: bool,
    update_polls: u32,
    /// Every value written to `Config_1`, in order.
    config_1_writes: Vec<u8>,
}

impl Sink {
    fn new() -> Self {
        Self {
            regs: [0; 256],
            flt_ready_after: None,
            flt_ready_polls: 0,
            rounds: Vec::new(),
            next_round: 0,
            frl_start_after: None,
            configured: false,
            update_polls: 0,
            config_1_writes: Vec::new(),
        }
    }

    fn read(&mut self, reg: u8) -> u8 {
        match reg {
            STATUS_FLAGS_0 => {
                if self
                    .flt_ready_after
                    .is_some_and(|n| self.flt_ready_polls >= n)
                {
                    self.regs[STATUS_FLAGS_0 as usize] |= FLT_READY;
                }
                self.flt_ready_polls += 1;
            }
            UPDATE_0 if self.configured => self.poll_update_0(),
            _ => {}
        }
        self.regs[reg as usize]
    }

    /// Posts the next round's requests, or `FRL_start` once the rounds are used up.
    fn poll_update_0(&mut self) {
        let update_0 = self.regs[UPDATE_0 as usize];
        match self.rounds.get(self.next_round) {
            Some(round) if update_0 & FLT_UPDATE == 0 && self.update_polls >= round.after_polls => {
                let [l0, l1, l2, l3] = round.lanes;
                self.regs[STATUS_FLAGS_1 as usize] = l0 | (l1 << 4);
                self.regs[STATUS_FLAGS_2 as usize] = l2 | (l3 << 4);
                self.regs[UPDATE_0 as usize] |= FLT_UPDATE;
            }
            None if self.frl_start_after.is_some_and(|n| self.update_polls >= n) => {
                self.regs[UPDATE_0 as usize] |= FRL_START;
            }
            _ => {}
        }
        self.update_polls += 1;
    }

    fn write(&mut self, reg: u8, value: u8) {
        match reg {
            // Update_0 is write-1-to-clear.
            UPDATE_0 => {
                let set = self.regs[UPDATE_0 as usize];
                if value & set & FLT_UPDATE != 0 {
                    self.next_round += 1;
                    self.update_polls = 0;
                }
                if value & set & FRL_START != 0 {
                    // FRL_start is set once.
                    self.frl_start_after = None;
                }
                self.regs[UPDATE_0 as usize] = set & !value;
            }
            CONFIG_1 => {
                self.config_1_writes.push(value);
                self.configured = value & 0x0F != 0;
                self.update_polls = 0;
                self.regs[CONFIG_1 as usize] = value;
            }
            _ => self.regs[reg as usize] = value,
        }
    }
}

/// The sink behind an `ScdcTransport`. `read` takes `&self`, so the sink's state is in a
/// `RefCell`.
struct SinkTransport(RefCell<Sink>);

impl ScdcTransport for SinkTransport {
    type Error = Infallible;

    async fn read(&self, reg: u8) -> Result<u8, Infallible> {
        Ok(self.0.borrow_mut().read(reg))
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), Infallible> {
        self.0.get_mut().write(reg, value);
        Ok(())
    }
}

// ── recording PHY ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct Phy {
    rates: Vec<HdmiForumFrl>,
    patterns: Vec<LanePatterns>,
    eq: Vec<EqParams>,
    output: Vec<FrlOutput>,
}

impl HdmiPhy for Phy {
    type Error = Infallible;

    async fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Infallible> {
        self.rates.push(rate);
        Ok(())
    }

    async fn send_ltp(&mut self, patterns: LanePatterns) -> Result<(), Infallible> {
        self.patterns.push(patterns);
        Ok(())
    }

    async fn set_frl_output(&mut self, output: FrlOutput) -> Result<(), Infallible> {
        self.output.push(output);
        Ok(())
    }

    async fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Infallible> {
        self.eq.push(params);
        Ok(())
    }

    async fn set_scrambling(&mut self, _enabled: bool) -> Result<(), Infallible> {
        Ok(())
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

const R12: HdmiForumFrl = HdmiForumFrl::Rate12Gbps4Lanes;
const R10: HdmiForumFrl = HdmiForumFrl::Rate10Gbps4Lanes;

fn round(after_polls: u32, lanes: [u8; 4]) -> Round {
    Round { after_polls, lanes }
}

fn ffe_3() -> TrainingConfig {
    let mut config = TrainingConfig::default();
    config.ffe_levels = FfeLevels::new(3).unwrap();
    config
}

async fn train(
    sink: Sink,
    rates: &[HdmiForumFrl],
    config: &TrainingConfig,
) -> (TrainingOutcome, Sink, Phy) {
    let mut trainer = FrlTrainer::new(Scdc::new(SinkTransport(RefCell::new(sink))), Phy::default());
    let outcome = trainer.train(rates, config).await.unwrap().outcome;
    let (scdc, phy) = trainer.into_parts();
    (outcome, scdc.into_transport().0.into_inner(), phy)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[pollster::test]
async fn trains_through_scdc() {
    let mut sink = Sink::new();
    sink.regs[CONFIG_0 as usize] = 0xFF;
    sink.flt_ready_after = Some(2);
    sink.rounds = vec![
        round(1, [0x5, 0x6, 0x7, 0x8]), // LFSR 0–3, one per lane
        round(0, [0x0, 0xE, 0x0, 0x0]), // raise lane 1's TxFFE level
        round(2, [0x0, 0x0, 0x0, 0x0]), // every lane passes
    ];
    sink.frl_start_after = Some(1);

    let (outcome, sink, phy) = train(sink, &[R12], &ffe_3()).await;

    assert!(matches!(
        outcome,
        TrainingOutcome::Success { achieved_rate: R12 }
    ));
    // Config_0 cleared; Config_1 = 12 Gbps (6) with FFE levels 3 in bits 7:4.
    assert_eq!(sink.regs[CONFIG_0 as usize], 0x00);
    assert_eq!(sink.config_1_writes, [0x36]);
    // Every flag plumbob serviced was cleared.
    assert_eq!(sink.regs[UPDATE_0 as usize], 0x00);

    assert_eq!(phy.rates, [R12]);
    assert_eq!(
        phy.patterns[2],
        LanePatterns {
            lane0: Some(LtpPattern::Lfsr0),
            lane1: Some(LtpPattern::Lfsr1),
            lane2: Some(LtpPattern::Lfsr2),
            lane3: Some(LtpPattern::Lfsr3),
        }
    );
    let raised = phy.eq.last().unwrap();
    assert_eq!(raised.lane1.tx_ffe_level.value(), 1);
    assert_eq!(raised.lane0.tx_ffe_level.value(), 0);
    assert_eq!(phy.output.last(), Some(&FrlOutput::GapOnly));
}

#[pollster::test]
async fn steps_down_when_the_sink_asks_for_a_lower_rate() {
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![
        round(0, [0xF; 4]), // every lane asks for a lower rate
        round(3, [0x5; 4]),
        round(1, [0x0; 4]),
    ];
    sink.frl_start_after = Some(0);

    let (outcome, sink, phy) = train(sink, &[R12, R10], &ffe_3()).await;

    assert!(matches!(
        outcome,
        TrainingOutcome::Success { achieved_rate: R10 }
    ));
    // Config_1: 12 Gbps (6), then 10 Gbps (5), both with FFE levels 3.
    assert_eq!(sink.config_1_writes, [0x36, 0x35]);
    assert_eq!(phy.rates, [R12, R10]);
}

#[pollster::test]
async fn a_timeout_returns_the_sink_to_tmds() {
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    // The sink never posts requests: LTS:3 times out.
    let mut config = ffe_3();
    config.ltp_polls = 5;

    let (outcome, sink, phy) = train(sink, &[R12], &config).await;

    assert!(matches!(
        outcome,
        TrainingOutcome::FallbackRequired {
            reason: FallbackReason::TrainingTimeout
        }
    ));
    // Config_1 written with 12 Gbps, then with FRL off.
    assert_eq!(sink.config_1_writes, [0x36, 0x00]);
    assert_eq!(phy.rates, [R12, HdmiForumFrl::NotSupported]);
}

#[pollster::test]
async fn rates_exhausted_clears_the_pending_flt_update() {
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![round(0, [0xF; 4])];

    let (outcome, sink, _) = train(sink, &[R12], &ffe_3()).await;

    assert!(matches!(
        outcome,
        TrainingOutcome::FallbackRequired {
            reason: FallbackReason::RatesExhausted
        }
    ));
    assert_eq!(sink.config_1_writes, [0x36, 0x00]);
    assert_eq!(sink.regs[UPDATE_0 as usize] & FLT_UPDATE, 0);
}

#[pollster::test]
async fn flt_no_timeout_from_source_test_configuration() {
    let mut sink = Sink::new();
    // The sink sets FLT_no_timeout (bit 5) and Source_Test_Update before training.
    sink.regs[SOURCE_TEST_CONFIG as usize] = 0x20;
    sink.regs[UPDATE_0 as usize] = SOURCE_TEST_UPDATE;
    sink.flt_ready_after = Some(10);
    sink.rounds = vec![round(0, [0x0; 4])];
    sink.frl_start_after = Some(0);
    let mut config = ffe_3();
    config.flt_ready_polls = 2;

    let (outcome, sink, _) = train(sink, &[R12], &config).await;

    // FLT_ready arrives after the normal limit, which FLT_no_timeout suspends.
    assert!(matches!(
        outcome,
        TrainingOutcome::Success { achieved_rate: R12 }
    ));
    assert_eq!(sink.regs[UPDATE_0 as usize] & SOURCE_TEST_UPDATE, 0);
}

#[pollster::test]
async fn an_undefined_request_is_a_protocol_error() {
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![round(0, [0x9, 0x0, 0x0, 0x0])];

    let mut trainer = FrlTrainer::new(Scdc::new(SinkTransport(RefCell::new(sink))), Phy::default());
    let result = trainer.train(&[R12], &ffe_3()).await;

    assert!(matches!(
        result,
        Err(plumbob_async::TrainingError::Scdc {
            error: culvert_async::ScdcError::Protocol(culvert_async::ProtocolError::UnknownLtpReq(
                0x9
            )),
            exit: plumbob_async::TmdsExit::Exited,
        })
    ));
}
