//! Demonstrates culvert-async against a simulated SCDC transport.
//!
//! A `SimulatedScdc` backed by a 256-byte register array stands in for the
//! DDC/I²C link to a physical sink. Registers are pre-loaded with a plausible
//! 6 Gbps 3-lane FRL sink state and the example walks through the full read
//! and write paths.

use core::convert::Infallible;
use culvert_async::{ClearableUpdateFlags, FfeLevels, FrlConfig, FrlRate, Scdc, TmdsConfig};
use hdmi_hal_async::scdc::ScdcTransport;

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
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let mut transport = SimulatedScdc::new();

    // --- Pre-load plausible sink state ---

    // Sink_Version = 1 (HDMI Forum SCDC v1)
    transport.set(0x01, 0x01);

    // Scrambler_Status: scrambling active (bit 0)
    transport.set(0x21, 0x01);

    // Status_Flags_0:
    //   clock_detected (bit 0) | ch0–ch2 locked (bits 1–3) | flt_ready (bit 6)
    //   = 0b0100_1111 = 0x4F
    transport.set(0x40, 0x4F);

    // Status_Flags_1 / Status_Flags_2: per-lane LTP requests
    //   lanes 0–2 request LFSR 2 (0x7); lane 3 is unused in 3-lane FRL
    transport.set(0x41, 0x77);
    transport.set(0x42, 0x07);

    // Update_0: flt_update (bit 5) set — sink reports new link training pattern requests
    transport.set(0x10, 0x20);

    // ERR_DET lane 0: valid (bit 7 of high byte), count = 0x0002
    transport.set(0x50, 0x02); // low byte
    transport.set(0x51, 0x80); // high byte: valid, upper counter bits = 0

    // ERR_DET lane 1: valid, count = 0x014F
    transport.set(0x52, 0x4F); // low byte
    transport.set(0x53, 0x81); // high byte: valid, upper counter bits = 0x01

    // Lanes 2 and 3: validity bit not set → will decode as None
    transport.set(0x54, 0x00); // lane 2 low
    transport.set(0x55, 0x00); // lane 2 high
    transport.set(0x57, 0x00); // lane 3 low (0x56 is the CED checksum)
    transport.set(0x58, 0x00); // lane 3 high

    let mut scdc = Scdc::new(transport);

    // --- Read path ---

    let sink_version = scdc.read_sink_version().await.unwrap();
    println!("Sink_Version:       0x{sink_version:02X}");

    let scrambler = scdc.read_scrambler_status().await.unwrap();
    println!("Scrambling active:  {}", scrambler.scrambling_active);

    let flags = scdc.read_status_flags().await.unwrap();
    println!("Clock detected:     {}", flags.clock_detected);
    println!(
        "Lane lock:          ch0={} ch1={} ch2={} ln3={}",
        flags.ch0_locked, flags.ch1_locked, flags.ch2_locked, flags.ln3_locked
    );
    println!("FLT_Ready:          {}", flags.flt_ready);

    let requests = scdc.read_ltp_requests().await.unwrap();
    println!(
        "LTP requests:       {:?} {:?} {:?} {:?}",
        requests.lane0, requests.lane1, requests.lane2, requests.lane3
    );

    let updates = scdc.read_update_flags().await.unwrap();
    println!(
        "Update flags:       status={} ced={} frl_start={} flt_update={}",
        updates.status_update, updates.ced_update, updates.frl_start, updates.flt_update
    );

    let ced = scdc.read_ced().await.unwrap();
    println!("CED lane 0:         {:?}", ced.lane0.map(|c| c.value()));
    println!("CED lane 1:         {:?}", ced.lane1.map(|c| c.value()));
    println!("CED lane 2:         {:?}", ced.lane2);
    println!("CED lane 3:         {:?}", ced.lane3);

    // --- Write path ---

    // Source announces itself as SCDC v1.
    scdc.write_source_version(0x01).await.unwrap();
    println!("\nWrote Source_Version = 0x01");

    // Enable scrambling at the ×40 clock ratio (FRL mode).
    scdc.write_tmds_config(TmdsConfig {
        scrambling_enable: true,
        high_tmds_clock_ratio: true,
    })
    .await
    .unwrap();
    println!("Wrote TMDS_Config: scrambling_enable=true, high_tmds_clock_ratio=true");

    // Request 6 Gbps 3-lane FRL with 2 FFE levels.
    scdc.write_frl_config(FrlConfig::new(
        FrlRate::Rate6Gbps3Lanes,
        FfeLevels::new(2).unwrap(),
    ))
    .await
    .unwrap();
    println!("Wrote Config_1: frl_rate=6G/3L, ffe_levels=2");

    // Acknowledge the flt_update flag (write-1-to-clear).
    scdc.clear_update_flags(ClearableUpdateFlags::new(
        false, false, false, false, true, false,
    ))
    .await
    .unwrap();
    println!("Cleared flt_update flag");
}
