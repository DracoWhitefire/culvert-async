# Architecture

## Role

`culvert-async` is the async companion to `culvert`. It mirrors `culvert`'s `Scdc<T>` client
with `async fn` methods, wrapping an `hdmi_hal_async::scdc::ScdcTransport` instead of a sync
transport. The split follows the same pattern as `embedded-hal` / `embedded-hal-async` and
`hdmi-hal` / `hdmi-hal-async`.

The crate also implements `plumbob_async::ScdcClient` for `Scdc<T>`, gated behind a
`plumbob-async` cargo feature. This is the async counterpart to the `plumbob` feature in
`culvert`, and the hook that connects `culvert-async` to `plumbob-async`'s training loop.

All register types, error types, and protocol constants are re-exported from `culvert` so
that callers do not need to depend on both crates.

---

## Scope

culvert-async covers:

- `Scdc<T>`: the central client type, wrapping an async `ScdcTransport` and exposing typed
  `async fn` methods for each register group,
- all methods from culvert, with `async fn` signatures:
  - version negotiation: `read_sink_version`, `write_source_version`,
  - scrambling control: `write_tmds_config`, `read_scrambler_status`,
  - FRL training primitives: `write_frl_config`, `read_status_flags`,
    `read_update_flags`, `clear_update_flags`,
  - CED reporting: `read_ced`,
- re-exports of all public types from `culvert` (`ScdcError`, `ProtocolError`,
  `TmdsConfig`, `ScramblerStatus`, `FrlConfig`, `FfeLevels`, `FrlRate`, `LtpReq`,
  `StatusFlags`, `UpdateFlags`, `CedCount`, `CedCounters`),
- `plumbob_async::ScdcClient` implementation for `Scdc<T>`, behind a `plumbob-async`
  cargo feature, mirroring the sync `plumbob` feature in `culvert`.

The following are out of scope:

- **Protocol logic** — bit decoding, register encoding, and error classification are
  implemented in `culvert` and re-used here. culvert-async adds no new protocol logic.
- **Sync API** — callers on sync transports use `culvert` directly.
- **Link training state machine** — `plumbob-async` owns that sequencing.
- **I²C / DDC transport** — platform backends implement `hdmi_hal_async::scdc::ScdcTransport`.
  culvert-async never touches I²C directly.
- **New register coverage** — registers that are on culvert's roadmap (RS correction
  counters, DSC status, manufacturer identification) are not added here ahead of culvert.
  When culvert gains a new method, culvert-async gains its async mirror in the same release.

---

## Dependencies

```
display-types  ─┐
hdmi-hal       ─┼─►  culvert          ─┐
                │                      │
hdmi-hal       ─┴─►  hdmi-hal-async  ─┴─►  culvert-async  ──►  plumbob-async
```

- `culvert` — all shared types, re-exported from here; no protocol logic is duplicated.
- `hdmi-hal-async` — for the async `ScdcTransport` trait.
- `plumbob-async` (optional) — for the `ScdcClient` trait that `Scdc<T>` implements.

culvert-async does not depend on `plumbob`. The `plumbob-async` feature pulls in only the
async training interface, not the sync one.

---

## The `Scdc<T>` Client

`Scdc<T>` is structurally identical to culvert's `Scdc<T>` — a thin, stateless client that
owns the transport — but the bound on `T` changes and every method becomes `async fn`:

```rust
pub struct Scdc<T> {
    transport: T,
}

impl<T: hdmi_hal_async::scdc::ScdcTransport> Scdc<T> {
    pub fn new(transport: T) -> Self;
    pub fn into_transport(self) -> T;

    // Version
    pub async fn read_sink_version(&mut self) -> Result<u8, ScdcError<T::Error>>;
    pub async fn write_source_version(&mut self, version: u8) -> Result<(), ScdcError<T::Error>>;

    // Scrambling
    pub async fn write_tmds_config(&mut self, config: TmdsConfig) -> Result<(), ScdcError<T::Error>>;
    pub async fn read_scrambler_status(&mut self) -> Result<ScramblerStatus, ScdcError<T::Error>>;

    // FRL training primitives
    pub async fn write_frl_config(&mut self, config: FrlConfig) -> Result<(), ScdcError<T::Error>>;
    pub async fn read_status_flags(&mut self) -> Result<StatusFlags, ScdcError<T::Error>>;
    pub async fn read_update_flags(&mut self) -> Result<UpdateFlags, ScdcError<T::Error>>;
    pub async fn clear_update_flags(&mut self, flags: UpdateFlags) -> Result<(), ScdcError<T::Error>>;

    // CED
    pub async fn read_ced(&mut self) -> Result<CedCounters, ScdcError<T::Error>>;
}
```

The return types are the same as culvert's: `ScdcError`, `TmdsConfig`, `ScramblerStatus`,
and so on are all re-exported from `culvert`. There is no parallel type hierarchy.

The semantics of each method are identical to culvert's. The only change is that the
register reads and writes yield to the executor rather than blocking.

---

## Register Addresses

culvert's address constants are `pub(crate)` and are not accessible from culvert-async.
culvert-async defines its own private `address` module containing the same constants,
sourced directly from the HDMI 2.1 spec (§10.4). These are fixed hardware constants — they
cannot change — so duplication is the right call here. Having them in one explicit location
within the crate is preferable to scattering raw hex literals through method bodies.

---

## The `plumbob-async` Feature

culvert-async implements `plumbob_async::ScdcClient` for `Scdc<T>`, gated behind a
`plumbob-async` cargo feature. This is the direct async counterpart to the `plumbob`
feature in `culvert`.

```toml
# Cargo.toml of a crate using both
culvert-async  = { version = "0.1", features = ["plumbob-async"] }
plumbob-async  = "0.1"
```

The type conversions are identical to culvert's sync impl: `culvert::StatusFlags` →
`plumbob::TrainingStatus`, `plumbob::FrlConfig` → `culvert::FrlConfig`, and
`culvert::CedCounters` → `plumbob::CedCounters`. The conversion functions are duplicated
from culvert's `plumbob_client.rs`; they are trivial match arms over spec-defined enums.

---

## Module Structure

```
src/
  lib.rs               — #![no_std], re-exports, module declarations, crate-level docs
  client/
    mod.rs             — Scdc<T> struct, new, into_transport
    address.rs         — private SCDC register address constants (HDMI 2.1 §10.4)
    version.rs         — read_sink_version, write_source_version
    scrambling.rs      — write_tmds_config, read_scrambler_status
    frl.rs             — write_frl_config, read_status_flags
    update.rs          — read_update_flags, clear_update_flags
    ced.rs             — read_ced
    plumbob_client.rs  — plumbob_async::ScdcClient impl (cfg feature = "plumbob-async")
    test_transport.rs  — async TestTransport (cfg test)
```

The module layout mirrors culvert's exactly. This is intentional: a reader familiar with
culvert can navigate culvert-async without friction, and the two codebases remain easy to
diff and keep in sync.

---

## Test Transport

culvert-async defines an async `TestTransport` in `client/test_transport.rs` for use in
`#[cfg(test)]` blocks. It implements `hdmi_hal_async::scdc::ScdcTransport` with a 256-byte
register array, a `failing_after(n)` constructor for error-path coverage, and `set`/`get`
helpers — a direct async mirror of culvert's sync `TestTransport`.

Tests run under `#[pollster::test]`, keeping the executor dependency out of the library and
avoiding any runtime choice. Every test that exists in culvert has an async counterpart here.

---

## `no_std` Compatibility

culvert-async is `#![no_std]`. The async transport trait and all return types require no
allocator. `Scdc<T>` holds only the transport; `ScdcError<E>` is stack-allocated. The full
API is available in bare `no_std` environments, consistent with culvert and the rest of
the stack.

`#![allow(async_fn_in_trait)]` is not needed in culvert-async: the crate implements async
traits but does not define them. The lint applies to trait definitions, not to implementors.

---

## Design Principles

The principles are the same as culvert's:

- **Type reuse over duplication.** All register types and error types live in `culvert`
  and are re-exported here. The only thing culvert-async adds is the async dispatch layer.
- **Stateless client, stateful caller.** `Scdc<T>` holds no protocol state. The async
  nature of the transport does not change this.
- **Deterministic and testable.** The async `TestTransport` supports the same pre-loaded
  register array and `failing_after` pattern as culvert's sync version. No hardware required.
- **Interface owned by the consumer.** `plumbob_async::ScdcClient` is defined in
  `plumbob-async`; culvert-async implements it. The cargo feature gates the impl so
  culvert-async remains independently usable without the training layer as a dependency.
- **No unsafe code.** `#![forbid(unsafe_code)]`.
- **Parity with culvert.** When culvert adds a method or register group, culvert-async
  adds its async mirror in the same release. The two crates move in lockstep.
- **Attested releases.** Every release is published through a GitHub Actions workflow
  that signs the `.crate` package with [SLSA Build Level 2](https://slsa.dev) provenance.
  Verify with `gh attestation verify <file> --repo DracoWhitefire/culvert-async`.

---

## Implementation Plan

### Step 1 — `Cargo.toml`

```toml
[package]
name = "culvert-async"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
description = "Async typed access to the HDMI 2.1 SCDC register map"
license = "MPL-2.0"
repository = "https://github.com/DracoWhitefire/culvert-async"
keywords = ["hdmi", "scdc", "frl", "embedded", "no-std"]
categories = ["hardware-support", "embedded", "no-std", "asynchronous"]

[package.metadata.docs.rs]
all-features = true

[features]
plumbob-async = ["dep:plumbob-async"]

[dependencies]
culvert = { version = "0.1.0", default-features = false }
hdmi-hal-async = { version = "0.1.0", default-features = false }
plumbob-async = { version = "0.1.0", optional = true, default-features = false }

[dev-dependencies]
pollster = { version = "0.3", features = ["macro"] }
```

Note: the feature is named `plumbob-async` (with a hyphen) to match the dependency name,
parallel to how culvert names its feature `plumbob`.

### Step 2 — `src/lib.rs`

```rust
#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod client;

pub use client::Scdc;
pub use culvert::{
    CedCount, CedCounters, FfeLevels, FrlConfig, FrlRate, LtpReq, ProtocolError, ScdcError,
    ScramblerStatus, StatusFlags, TmdsConfig, UpdateFlags,
};
```

### Step 3 — `src/client/mod.rs`

Define `Scdc<T>` with `new` and `into_transport`. The bound on `T` is
`hdmi_hal_async::scdc::ScdcTransport`. No methods live here; they are in the sub-modules.

### Step 4 — `src/client/address.rs`

Copy culvert's register address constants verbatim. They are `pub(crate)` and referenced
by all method sub-modules.

### Step 5 — Method sub-modules

Implement each method module (`version.rs`, `scrambling.rs`, `frl.rs`, `update.rs`,
`ced.rs`) as a direct async mirror of its culvert counterpart:

- Replace `use hdmi_hal::scdc::ScdcTransport` with `use hdmi_hal_async::scdc::ScdcTransport`.
- Add `async` to each `fn`.
- Replace `self.transport.read(...)` with `self.transport.read(...).await`.
- Replace `self.transport.write(...)` with `self.transport.write(...).await`.
- All error mapping, bit manipulation, and decode logic is unchanged from culvert.

Each module contains its own `#[cfg(test)]` block mirroring culvert's tests, updated to
use `#[pollster::test]` and `async fn` test bodies, and to await every `Scdc` method call.

### Step 6 — `src/client/test_transport.rs`

Implement `TestTransport`:

```rust
pub struct TestTransport { regs: [u8; 256], fail_after: usize, ops: usize }

impl hdmi_hal_async::scdc::ScdcTransport for TestTransport {
    type Error = ();
    async fn read(&mut self, reg: u8) -> Result<u8, ()> { ... }
    async fn write(&mut self, reg: u8, value: u8) -> Result<(), ()> { ... }
}
```

`new`, `failing_after`, `set`, and `get` are identical to culvert's sync `TestTransport`.

### Step 7 — `src/client/plumbob_client.rs`

Implement `plumbob_async::ScdcClient` for `Scdc<T>`, gated with
`#[cfg(feature = "plumbob-async")]`. The impl block is:

```rust
impl<T: hdmi_hal_async::scdc::ScdcTransport> plumbob_async::ScdcClient for Scdc<T> {
    type Error = ScdcError<T::Error>;

    async fn write_frl_config(&mut self, config: plumbob_async::FrlConfig)
        -> Result<(), Self::Error> { ... }

    async fn read_training_status(&mut self)
        -> Result<plumbob_async::TrainingStatus, Self::Error> { ... }

    async fn read_ced(&mut self)
        -> Result<plumbob_async::CedCounters, Self::Error> { ... }
}
```

The type conversion helpers (`ffe_levels`, `ltp_req`, `ced_count`) are copied from
culvert's `plumbob_client.rs` unchanged — they are pure enum-to-enum match expressions.

Tests mirror culvert's `plumbob_client.rs` tests, converted to `#[pollster::test]`.

### Step 8 — README and doc

README follows culvert's structure: crate role, quick-start snippet showing `Scdc::new`
with an async transport and `.await` on a method call, feature flags table, and a pointer
to this file.
