# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking changes

- **The register map is culvert's corrected one.** Every method now reads and writes the
  registers `culvert::codec` names and converts with its functions, instead of
  culvert-async's own copy of culvert's previous, incorrect map (see culvert's changelog,
  *Fixed*). `write_frl_config` writes `Config_1` (0x31); `StatusFlags` decodes
  `Status_Flags_0` only; link training requests are read per lane with
  `read_ltp_requests`, where an undefined value is `LtpReq::Reserved` rather than an
  error (`ProtocolError::UnknownLtpReq` is removed); `UpdateFlags` decodes all `Update_0`
  flags, and `clear_update_flags` takes culvert's new `ClearableUpdateFlags` — every
  flag but `rr_test`, which the source does not clear (a provisional rule; see culvert's
  `doc/architecture.md`) — with `UpdateFlags::clearable()` for a read-then-clear. `FrlConfig`, `FfeLevels`, `StatusFlags`, `UpdateFlags` and `LtpReq` follow
  culvert's new definitions.
- **`hdmi-hal-async` updated to 0.2**: `Scdc<T>`'s transport implements hdmi-hal-async
  0.2's `ScdcTransport`, whose `read` takes `&self` instead of `&mut self`. Transports
  must change the receiver and use interior mutability for any state changed during
  reads. The new `read_block` method has a default and needs no changes.
- **`culvert` updated to 0.2, and `display-types` to 0.4**: the re-exported
  register and error types are the new culvert's, and `FrlRate` (culvert's re-export of
  display-types' `HdmiForumFrl`) is display-types 0.4's type. Code that passes a
  display-types 0.3 `HdmiForumFrl` must move to display-types 0.4.
- **The `plumbob-async` feature implements plumbob-async's per-lane `ScdcClient`**: the
  eight methods of plumbob's link training states (`read_flt_ready`,
  `read_update_flags`, `clear_update_flags`, `read_ltp_requests`,
  `read_source_test_config`, `write_config_0_defaults`, `write_frl_config`, `read_ced`).
  The conversions are culvert's own: the feature enables culvert's `plumbob` feature.

### Added

- `ScdcTransport` re-exported at the crate root, from `hdmi-hal-async`.
- `write_config_0`, `read_source_test_config`, `read_ltp_requests` and
  `read_rs_correction`, with the re-exports `Config0`, `SourceTestConfig`, `LtpRequests`
  and `RsCorrectionCount`.
- End-to-end tests of plumbob-async's training through `Scdc<T>` (`tests/plumbob_async.rs`).
- **SLSA Build Level 2 provenance** — release artifacts are attested via
  `actions/attest-build-provenance` and verified with
  `gh attestation verify <file> --repo DracoWhitefire/culvert-async`.

### Internal

- The crate documentation no longer links to `plumbob_async::ScdcClient`, which only
  resolves with the `plumbob-async` feature, so the docs build without it.
- **CI builds for a `no_std` target** — new `Build (no_std)` steps build the crate for
  `thumbv7em-none-eabi`, with and without the `plumbob-async` feature. Nothing checked a
  target without `std` before, so a dependency that enables `std` (as hdmi-hal did
  through `display-types`) went unnoticed.
  The publish workflow runs the same build steps.
- The client modules, their tests and the example are culvert's with `async`/`.await`;
  culvert-async's own register address module is removed.

## [0.1.0] - 2026-04-05

### Added

**Async SCDC client**

- `Scdc<T>` — stateless async typed client wrapping an
  [`hdmi_hal_async::scdc::ScdcTransport`]. Structurally identical to `culvert::Scdc<T>`
  but every method is `async fn` and the transport bound is the async variant.
- `Scdc::new(transport)` and `Scdc::into_transport()` for construction and unwrapping.

**Version registers**

- `read_sink_version()` — reads `Sink_Version` (0x01).
- `write_source_version(u8)` — writes `Source_Version` (0x02).

**TMDS scrambling**

- `write_tmds_config(TmdsConfig)` — writes `TMDS_Config` (0x20): `Scrambling_Enable`
  (bit 0) and `TMDS_Bit_Clock_Ratio` (bit 1).
- `read_scrambler_status()` — reads `Scrambler_Status` (0x21); returns `ScramblerStatus`
  with the sink's `scrambling_active` confirmation flag.

**FRL link training**

- `write_frl_config(FrlConfig)` — writes `Config_0` (0x30): `FRL_Rate` (bits 3:0),
  `DSC_FRL_Max` (bit 4), and `FFE_Levels` (bits 7:5).
- `read_status_flags()` — reads `Status_Flags_0` (0x40) and `Status_Flags_1` (0x41);
  returns `StatusFlags` covering clock detection, cable presence, per-lane symbol lock
  (lanes 0–3), `FLT_Ready`, `FRL_Start`, and the current `LtpReq`.

**Update flags**

- `read_update_flags()` — reads `Update_0` (0x10) and `Update_1` (0x11); returns
  `UpdateFlags` with `status_update`, `ced_update`, `frl_update`, and `dsc_update`.
- `clear_update_flags(UpdateFlags)` — write-1-to-clear: each flag set to `true` is
  cleared in the corresponding register; flags set to `false` are left unchanged.

**Character Error Detection**

- `read_ced()` — reads `ERR_DET` registers (0x50–0x57); returns `CedCounters` with
  per-lane `Option<CedCount>` values. A lane's counter is `None` when the high-byte
  validity bit is not set. Lane 3 is always `None` in TMDS or 3-lane FRL mode.

**Re-exports from `culvert`**

All register types and error types are re-exported from `culvert` so that callers do
not need to depend on both crates: `TmdsConfig`, `ScramblerStatus`, `FrlConfig`,
`FrlRate`, `FfeLevels`, `LtpReq`, `StatusFlags`, `UpdateFlags`, `CedCount`,
`CedCounters`, `ScdcError`, `ProtocolError`.

**`plumbob-async` feature**

- Implements [`plumbob_async::ScdcClient`] for `Scdc<T>`, bridging `write_frl_config`,
  `read_training_status`, and `read_ced` to the `plumbob-async` trait vocabulary.
  Enabled by `features = ["plumbob-async"]`.

**Safety and portability**

- `#![no_std]` — fully `no_std` compatible with no `alloc` requirement.
- `#![forbid(unsafe_code)]` — no unsafe code anywhere in the crate.
- 100% line coverage measured with `cargo-llvm-cov`; baseline stored in
  `.coverage-baseline`.
- Unit tests using `#[pollster::test]` as the test executor; every test from `culvert`
  has an async counterpart here.
