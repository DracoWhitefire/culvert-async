//! Raw address constants for the SCDC register map (HDMI 2.1 spec §10.4).
//!
//! All SCDC-defined register addresses are listed here, including those not yet
//! wrapped by a typed method, so the full map is in one place.

// Version (§10.4.1)
pub(crate) const SINK_VERSION: u8 = 0x01;
pub(crate) const SOURCE_VERSION: u8 = 0x02;

// Update flags (§10.4.2)
pub(crate) const UPDATE_0: u8 = 0x10;
pub(crate) const UPDATE_1: u8 = 0x11;

// TMDS and scrambling (§10.4.3)
pub(crate) const TMDS_CONFIG: u8 = 0x20;
pub(crate) const SCRAMBLER_STATUS: u8 = 0x21;

// FRL configuration and status (§10.4.4)
pub(crate) const CONFIG_0: u8 = 0x30;
pub(crate) const STATUS_FLAGS_0: u8 = 0x40;
pub(crate) const STATUS_FLAGS_1: u8 = 0x41;

// Character Error Detection (§10.4.5)
pub(crate) const ERR_DET_0_L: u8 = 0x50;
pub(crate) const ERR_DET_0_H: u8 = 0x51;
pub(crate) const ERR_DET_1_L: u8 = 0x52;
pub(crate) const ERR_DET_1_H: u8 = 0x53;
pub(crate) const ERR_DET_2_L: u8 = 0x54;
pub(crate) const ERR_DET_2_H: u8 = 0x55;
pub(crate) const ERR_DET_3_L: u8 = 0x56;
pub(crate) const ERR_DET_3_H: u8 = 0x57;
