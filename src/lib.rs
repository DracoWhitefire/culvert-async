//! Async typed access to the HDMI 2.1 SCDC (Status and Control Data Channel) register map.
//!
//! `culvert-async` is the async companion to [`culvert`]: [`culvert::Scdc`]'s methods as
//! `async fn`s over an [`hdmi_hal_async::scdc::ScdcTransport`]. The register map is
//! culvert's [`culvert::codec`]; this crate only performs the reads and writes, so the sync
//! and async clients always agree.
//!
//! All register types and error types are re-exported from `culvert` so that callers do not
//! need to depend on both crates.
//!
//! # Features
//!
//! - **`plumbob-async`** — implements `plumbob_async::ScdcClient` for [`Scdc<T>`], connecting
//!   culvert-async to the async FRL link training state machine.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod client;

pub use client::Scdc;
pub use culvert::{
    CedCount, CedCounters, ClearableUpdateFlags, Config0, FfeLevels, FrlConfig, FrlRate, LtpReq,
    LtpRequests, ProtocolError, RsCorrectionCount, ScdcError, ScramblerStatus, SourceTestConfig,
    StatusFlags, TmdsConfig, UpdateFlags,
};
/// The async transport `Scdc<T>` is generic over, re-exported from `hdmi-hal-async`.
pub use hdmi_hal_async::scdc::ScdcTransport;
