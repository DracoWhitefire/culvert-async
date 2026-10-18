//! End-to-end tests for the `plumbob-async` feature: plumbob's link training state
//! machine, driven by plumbob-async, trains through `Scdc<T>` against a sink simulated at
//! the SCDC register level.
//!
//! The sink reacts to the registers the way a real one does, so these tests check the
//! whole chain — plumbob's sequencing, culvert's encoding and decoding, and the register
//! bits in between.
#![cfg(feature = "plumbob-async")]

use core::cell::{Cell, RefCell};
use core::convert::Infallible;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU32, Ordering};
use core::task::{Context, Poll};
use std::sync::Mutex;

use culvert_async::{Scdc, ScdcError};
use display_types::cea861::hdmi_forum::HdmiForumFrl;
use hdmi_hal_async::phy::{HdmiPhy, SendHdmiPhy};
use hdmi_hal_async::scdc::{ScdcTransport, SendScdcTransport};
use hdmi_hal_async::{EqParams, FrlOutput, LanePatterns, LtpPattern};
use plumbob_async::{
    FallbackReason, FfeLevels, FrlTrainer, TmdsExit, TrainingConfig, TrainingError, TrainingEvent,
    TrainingOutcome, TrainingWarning,
};

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
    /// Raises `FRL_start` together with `FLT_update` when this round is posted.
    frl_start_with_round: Option<usize>,
    /// Training runs while `Config_1` holds an FRL rate.
    configured: bool,
    update_polls: u32,
    /// Every value written to `Config_1`, in order.
    config_1_writes: Vec<u8>,
    /// The next read of this register fails (a DDC NACK), once.
    nack_read_of: Option<u8>,
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
            frl_start_with_round: None,
            configured: false,
            update_polls: 0,
            config_1_writes: Vec::new(),
            nack_read_of: None,
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
                if self.frl_start_with_round == Some(self.next_round) {
                    self.regs[UPDATE_0 as usize] |= FRL_START;
                }
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
                if value & set & FRL_START != 0 && self.next_round >= self.rounds.len() {
                    // FRL_start is set once; one raised with a round does not use it up.
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

/// A future that is `Pending` once and wakes itself before completing, as a transport
/// that waits on the bus would be. It counts each `Pending` it returns in `pending`, so a
/// test can check that the executor really saw the transport wait.
struct YieldOnce<'a> {
    yielded: bool,
    pending: &'a AtomicU32,
}

impl<'a> YieldOnce<'a> {
    fn new(pending: &'a AtomicU32) -> Self {
        Self {
            yielded: false,
            pending,
        }
    }
}

impl Future for YieldOnce<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            self.pending.fetch_add(1, Ordering::Relaxed);
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// The sink behind an `ScdcTransport`. `read` takes `&self`, so the sink's state is in a
/// `RefCell`. Every other operation waits once, so training runs over I/O that really
/// suspends, not only futures that are ready when first polled.
struct SinkTransport {
    sink: RefCell<Sink>,
    operations: Cell<u32>,
    pending: AtomicU32,
}

impl SinkTransport {
    fn new(sink: Sink) -> Self {
        Self {
            sink: RefCell::new(sink),
            operations: Cell::new(0),
            pending: AtomicU32::new(0),
        }
    }

    async fn maybe_wait(&self) {
        let n = self.operations.get();
        self.operations.set(n + 1);
        if n % 2 == 1 {
            YieldOnce::new(&self.pending).await;
        }
    }
}

/// A DDC NACK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Nack;

impl ScdcTransport for SinkTransport {
    type Error = Nack;

    async fn read(&self, reg: u8) -> Result<u8, Nack> {
        self.maybe_wait().await;
        let mut sink = self.sink.borrow_mut();
        if sink.nack_read_of == Some(reg) {
            sink.nack_read_of = None;
            return Err(Nack);
        }
        Ok(sink.read(reg))
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), Nack> {
        self.maybe_wait().await;
        self.sink.get_mut().write(reg, value);
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
    let mut trainer = FrlTrainer::new(Scdc::new(SinkTransport::new(sink)), Phy::default());
    let outcome = trainer.train(rates, config).await.unwrap().outcome;
    let (scdc, phy) = trainer.into_parts();
    let transport = scdc.into_transport();
    assert!(
        transport.pending.into_inner() > 0,
        "the transport waited during training"
    );
    (outcome, transport.sink.into_inner(), phy)
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
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
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
        TrainingOutcome::Success {
            achieved_rate: R10,
            ..
        }
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
            reason: FallbackReason::TrainingTimeout,
            ..
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
            reason: FallbackReason::RatesExhausted,
            ..
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
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert_eq!(sink.regs[UPDATE_0 as usize] & SOURCE_TEST_UPDATE, 0);
}

/// Trains `sink` over `rates`, returning the warnings with the outcome.
async fn train_with_warnings(
    sink: Sink,
    rates: &[HdmiForumFrl],
) -> (TrainingOutcome, Vec<TrainingWarning>) {
    let mut trainer = FrlTrainer::new(Scdc::new(SinkTransport::new(sink)), Phy::default());
    let trained = trainer.train(rates, &ffe_3()).await.unwrap();
    let warnings = trained.iter_warnings().copied().collect();
    (trained.outcome, warnings)
}

#[pollster::test]
async fn an_undefined_request_is_ignored_and_reported() {
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![round(0, [0x5, 0x9, 0x5, 0x5]), round(0, [0x0; 4])];
    sink.frl_start_after = Some(0);

    let (outcome, warnings) = train_with_warnings(sink, &[R12]).await;

    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert!(matches!(
        warnings.as_slice(),
        [TrainingWarning::UndefinedLtpRequest {
            lane: 1,
            value: 0x9,
            count: 1,
            in_use: true,
            ..
        }]
    ));
}

#[pollster::test]
async fn a_three_lane_link_ignores_an_undefined_lane_3_nibble() {
    // Lane 3 is not in use at a 3-lane rate; an undefined value there used to end
    // training with a protocol error.
    let rate = HdmiForumFrl::Rate6Gbps3Lanes;
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![
        round(0, [0x5, 0x5, 0x5, 0x9]),
        round(0, [0x0, 0x0, 0x0, 0x9]),
    ];
    sink.frl_start_after = Some(0);

    let (outcome, warnings) = train_with_warnings(sink, &[rate]).await;

    assert!(matches!(
        outcome,
        TrainingOutcome::Success { achieved_rate, .. } if achieved_rate == rate
    ));
    assert!(matches!(
        warnings.as_slice(),
        [TrainingWarning::UndefinedLtpRequest {
            lane: 3,
            value: 0x9,
            count: 2,
            in_use: false,
            ..
        }]
    ));
}

#[pollster::test]
async fn frl_start_together_with_flt_update_retrains() {
    // LTS:P sees both flags at once: the retrain wins, and the second pass starts.
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![
        round(0, [0x5; 4]),
        round(0, [0x0; 4]),
        round(0, [0x6; 4]),
        round(0, [0x0; 4]),
    ];
    sink.frl_start_with_round = Some(2);
    sink.frl_start_after = Some(0);
    let mut trainer = FrlTrainer::new(Scdc::new(SinkTransport::new(sink)), Phy::default());
    let mut events = Vec::new();
    let outcome = trainer
        .train_with_events(&[R12], &ffe_3(), &mut |event| events.push(event))
        .await
        .unwrap()
        .outcome;
    let (scdc, phy) = trainer.into_parts();
    let sink = scdc.into_transport().sink.into_inner();
    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert!(events.contains(&TrainingEvent::FrlStartWithRetrain));
    assert_eq!(
        sink.regs[UPDATE_0 as usize], 0,
        "every flag plumbob saw is cleared"
    );
    let lfsr1 = Some(LtpPattern::Lfsr1);
    assert!(phy.patterns.contains(&LanePatterns {
        lane0: lfsr1,
        lane1: lfsr1,
        lane2: lfsr1,
        lane3: lfsr1,
    }));
}

#[pollster::test]
async fn an_io_error_mid_training_exits_to_tmds() {
    // A DDC NACK reading the lane requests in LTS:3: plumbob still runs LTS:L, which turns
    // FRL off in Config_1, returns the PHY to TMDS and clears the pending FLT_update.
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![round(0, [0x5; 4])];
    sink.nack_read_of = Some(STATUS_FLAGS_1);
    let mut trainer = FrlTrainer::new(Scdc::new(SinkTransport::new(sink)), Phy::default());
    let result = trainer.train(&[R12], &ffe_3()).await;
    assert!(
        matches!(
            result,
            Err(TrainingError::Scdc {
                error: ScdcError::Transport(Nack),
                exit: TmdsExit::Exited,
                ..
            })
        ),
        "{result:?}"
    );
    let (scdc, phy) = trainer.into_parts();
    let sink = scdc.into_transport().sink.into_inner();
    assert_eq!(sink.config_1_writes, [0x36, 0x00]);
    assert_eq!(phy.rates, [R12, HdmiForumFrl::NotSupported]);
    assert_eq!(sink.regs[UPDATE_0 as usize] & FLT_UPDATE, 0);
}

#[pollster::test]
async fn a_retrain_resumes_from_no_pattern_at_register_level() {
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![
        round(0, [0x5; 4]),
        round(0, [0x0; 4]),
        // The retrain: only lane 0 asks for a pattern (LFSR 1).
        round(1, [0x6, 0x0, 0x0, 0x0]),
        round(0, [0x0; 4]),
    ];
    sink.frl_start_after = Some(0);
    let (outcome, sink, phy) = train(sink, &[R12], &ffe_3()).await;
    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert_eq!(sink.regs[UPDATE_0 as usize], 0);
    assert!(phy.patterns.contains(&LanePatterns {
        lane0: Some(LtpPattern::Lfsr1),
        lane1: None,
        lane2: None,
        lane3: None,
    }));
}

#[pollster::test]
async fn update_0_flags_plumbob_does_not_handle_are_left_set() {
    let mut sink = Sink::new();
    // Status_Update, CED_Update, RR_Test and RSED_Update.
    sink.regs[UPDATE_0 as usize] = 0x01 | 0x02 | 0x04 | 0x40;
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![round(0, [0x5; 4]), round(0, [0x0; 4])];
    sink.frl_start_after = Some(0);
    let (outcome, sink, _) = train(sink, &[R12], &ffe_3()).await;
    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert_eq!(sink.regs[UPDATE_0 as usize], 0x47);
}

#[pollster::test]
async fn the_ffe_maximum_is_limited_per_rate_through_culvert() {
    // FfeLevels::MAX is 7; up to 12 Gbps a sink allows 3.
    for (rate, config_1) in [(R12, 0x36u8), (HdmiForumFrl::Rate3Gbps3Lanes, 0x31)] {
        let mut sink = Sink::new();
        sink.flt_ready_after = Some(0);
        sink.rounds = vec![round(0, [0x0; 4])];
        sink.frl_start_after = Some(0);
        let mut config = TrainingConfig::default();
        config.ffe_levels = FfeLevels::MAX;
        let (outcome, sink, _) = train(sink, &[rate], &config).await;
        assert!(matches!(
            outcome,
            TrainingOutcome::Success { achieved_rate, .. } if achieved_rate == rate
        ));
        assert_eq!(sink.config_1_writes, [config_1]);
    }
}

// ── `Send` futures ────────────────────────────────────────────────────────────

/// The sink behind a `SendScdcTransport`. The twin is `Sync` (`read` takes `&self`), so
/// the sink's state is behind a `Mutex`, locked only after the wait: no guard is held
/// across an `.await`. Every other operation waits once, as `SinkTransport`'s do.
struct SendSinkTransport {
    sink: Mutex<Sink>,
    operations: AtomicU32,
    pending: AtomicU32,
}

impl SendSinkTransport {
    async fn maybe_wait(&self) {
        if self.operations.fetch_add(1, Ordering::Relaxed) % 2 == 1 {
            YieldOnce::new(&self.pending).await;
        }
    }
}

impl SendScdcTransport for SendSinkTransport {
    type Error = Nack;

    async fn read(&self, reg: u8) -> Result<u8, Nack> {
        self.maybe_wait().await;
        Ok(self.sink.lock().unwrap().read(reg))
    }

    async fn write(&mut self, reg: u8, value: u8) -> Result<(), Nack> {
        self.maybe_wait().await;
        self.sink.get_mut().unwrap().write(reg, value);
        Ok(())
    }
}

/// The recording PHY behind hdmi-hal-async's `Send` twin.
struct SendPhy(Phy);

impl SendHdmiPhy for SendPhy {
    type Error = Infallible;

    async fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Infallible> {
        HdmiPhy::set_frl_rate(&mut self.0, rate).await
    }

    async fn send_ltp(&mut self, patterns: LanePatterns) -> Result<(), Infallible> {
        HdmiPhy::send_ltp(&mut self.0, patterns).await
    }

    async fn set_frl_output(&mut self, output: FrlOutput) -> Result<(), Infallible> {
        HdmiPhy::set_frl_output(&mut self.0, output).await
    }

    async fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Infallible> {
        HdmiPhy::adjust_equalization(&mut self.0, params).await
    }

    async fn set_scrambling(&mut self, enabled: bool) -> Result<(), Infallible> {
        HdmiPhy::set_scrambling(&mut self.0, enabled).await
    }
}

/// Trains, exits and trains again with events: the outcome of the first attempt, whether
/// LTS:L succeeded, and how many events the second attempt reported.
async fn train_exit_train<T: ScdcTransport, P: HdmiPhy>(
    trainer: &mut FrlTrainer<Scdc<T>, P>,
    config: &TrainingConfig,
) -> (Option<TrainingOutcome>, bool, usize) {
    let outcome = trainer
        .train(&[R12], config)
        .await
        .ok()
        .map(|trained| trained.outcome);
    let exited = trainer.exit_to_tmds().await.is_ok();
    let mut events = 0;
    let _ = trainer
        .train_with_events(&[R12], config, &mut |_| events += 1)
        .await;
    (outcome, exited, events)
}

/// [`train_exit_train`] through the plain `Scdc<T>`, generic over the transport's and the
/// PHY's `Send` twins, as a future that must be `Send`: this compiles only if `Scdc<T>`'s
/// futures are `Send` over a `SendScdcTransport`, so training through it could be spawned
/// on a multi-threaded executor without a second client type.
fn spawnable<'a, T: SendScdcTransport, P: SendHdmiPhy>(
    trainer: &'a mut FrlTrainer<Scdc<T>, P>,
    config: &'a TrainingConfig,
) -> impl Future<Output = (Option<TrainingOutcome>, bool, usize)> + Send + 'a {
    train_exit_train(trainer, config)
}

#[pollster::test]
async fn training_through_scdc_is_send_over_send_twins() {
    let mut sink = Sink::new();
    sink.flt_ready_after = Some(0);
    sink.rounds = vec![round(0, [0x0; 4])];
    sink.frl_start_after = Some(0);
    let transport = SendSinkTransport {
        sink: Mutex::new(sink),
        operations: AtomicU32::new(0),
        pending: AtomicU32::new(0),
    };
    let mut trainer = FrlTrainer::new(Scdc::new(transport), SendPhy(Phy::default()));
    let (outcome, exited, events) = spawnable(&mut trainer, &ffe_3()).await;
    assert!(matches!(
        outcome,
        Some(TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        })
    ));
    assert!(exited);
    assert!(events > 0);
    let (scdc, SendPhy(phy)) = trainer.into_parts();
    assert!(scdc.into_transport().pending.into_inner() > 0);
    assert!(phy.rates.contains(&R12));
}
