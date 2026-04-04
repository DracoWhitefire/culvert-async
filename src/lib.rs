//! Async typed access to the HDMI 2.1 SCDC (Status and Control Data Channel) register map.
//!
//! `culvert-async` is the async companion to [`culvert`], following the same design split
//! as `embedded-hal` / `embedded-hal-async`. It mirrors [`culvert::Scdc`] with `async fn`
//! methods, wrapping an [`hdmi_hal_async::scdc::ScdcTransport`] instead of a sync transport.
//!
//! All register types and error types are re-exported from `culvert` so that callers do not
//! need to depend on both crates.
//!
//! # Features
//!
//! - **`plumbob-async`** — implements [`plumbob_async::ScdcClient`] for [`Scdc<T>`], connecting
//!   culvert-async to the async FRL link training state machine.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod client;

pub use client::Scdc;
pub use culvert::{
    CedCount, CedCounters, FfeLevels, FrlConfig, FrlRate, LtpReq, ProtocolError, ScdcError,
    ScramblerStatus, StatusFlags, TmdsConfig, UpdateFlags,
};
