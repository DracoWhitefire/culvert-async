# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Part of Ashlar** — the README now opens with a block naming the
  [Ashlar](https://ashlar-display.org) stack and linking the other crates in it. Keywords
  are aligned across the stack, with `ashlar` and `hdmi` on every crate; `ashlar` and
  `async` are added, `embedded` is dropped in favour of more specific keywords. Categories
  gain `no-std::no-alloc`. The crate's `homepage` is now the Ashlar website.

## [0.2.0] - 2026-10-18

### Breaking changes

- **The register map is culvert's corrected one, and the re-exported types follow it.**
  Every method now reads and writes the registers `culvert::codec` names and converts with
  its functions, instead of culvert-async's own copy of culvert's previous, incorrect map
  (see culvert 0.2.0's changelog, *Fixed*). The register and error types re-exported here
  are culvert 0.2's:
  - `write_frl_config` writes `Config_1` (0x31) instead of `Config_0` (0x30).
    `FrlConfig` is `#[non_exhaustive]`, built with `FrlConfig::new(frl_rate, ffe_levels)`;
    `FrlConfig::dsc_frl_max` is removed (`DSC_FRL_Max` is a sink-written flag, read with
    `read_source_test_config`). `FfeLevels` is the highest TxFFE level index (0–7,
    `FfeLevels::new`, `FfeLevels::value`) instead of the `Ffe0`–`Ffe7` enum, and
    `write_frl_config` returns `ProtocolError::FfeLevelsOutOfRange`, writing nothing, when
    it exceeds the maximum for the rate (3 up to 12 Gbps, 7 above).
  - `read_status_flags` decodes `Status_Flags_0` only: `cable_connected`, `ch3_locked`,
    `frl_start` and `ltp_req` are removed from `StatusFlags`, and the method no longer
    reads `Status_Flags_1` or returns `ProtocolError::UnknownLtpReq`.
  - Link training pattern requests are read per lane with `read_ltp_requests`. `LtpReq`
    follows the specification's values (LFSR 0–3 are 0x5–0x8; `AllOnes`, `AllZeros`,
    `NyquistClock`, `DdeCompliance`, `FfeChange` and `RateChange` are added), and an
    undefined value is `LtpReq::Reserved(value)` rather than an error:
    `ProtocolError::UnknownLtpReq` is removed.
  - `UpdateFlags` decodes all seven `Update_0` flags; `frl_update` and `dsc_update` are
    removed, and `Update_1` (0x11) is no longer read or written. `clear_update_flags` takes
    culvert's new `ClearableUpdateFlags` instead of `UpdateFlags`: every flag but
    `rr_test`, which the source does not clear (a provisional rule; see culvert's
    `doc/architecture.md`), with `UpdateFlags::clearable()` for a read-then-clear.
  - `read_ced` reads lane 3 from 0x57/0x58; 0x56, which it read before, is the CED
    checksum.
- **`hdmi-hal-async` updated to 0.2.1**: `Scdc<T>`'s transport implements hdmi-hal-async
  0.2's `ScdcTransport`, whose `read` takes `&self` instead of `&mut self`. Transports
  must change the receiver and use interior mutability for any state changed during
  reads. The new `read_block` method has a default and needs no changes.
- **`culvert` updated to 0.2, and `display-types` to 0.4**: the re-exported
  register and error types are the new culvert's, and `FrlRate` (culvert's re-export of
  display-types' `HdmiForumFrl`) is display-types 0.4's type. Code that passes a
  display-types 0.3 `HdmiForumFrl` must move to display-types 0.4.
- **The `plumbob-async` feature implements plumbob-async 0.2's per-lane `ScdcClient`**:
  the eight methods of plumbob's link training states (`read_flt_ready`,
  `read_update_flags`, `clear_update_flags`, `read_ltp_requests`,
  `read_source_test_config`, `write_config_0_defaults`, `write_frl_config`, `read_ced`);
  `clear_update_flags` takes plumbob's `ClearableUpdateFlags`. The conversions are
  culvert's own: the feature enables culvert's `plumbob` feature. The feature now
  requires plumbob-async 0.2 (and so plumbob 0.3.1); see plumbob-async's changelog for the
  changes code using it meets, such as `#[non_exhaustive]` outcomes and events.

### Added

- `ScdcTransport` and its `Send` twin `SendScdcTransport` re-exported at the crate root,
  from `hdmi-hal-async`, so `Send` code needs no direct dependency on it.
- `write_config_0`, `read_source_test_config`, `read_ltp_requests` and
  `read_rs_correction`, with the re-exports `Config0`, `SourceTestConfig`, `LtpRequests`
  and `RsCorrectionCount`.
- **Training through `Scdc<T>` can run on a multi-threaded executor**: over a transport
  that implements hdmi-hal-async's `SendScdcTransport`, `Scdc<T>`'s futures are `Send`,
  so with a `SendHdmiPhy` the training futures are `Send` in code generic over the
  transport and the PHY. No second client type is needed for that. Code bounded by
  plumbob-async's `SendScdcClient` instead cannot take `Scdc<T>`, which implements the
  plain `ScdcClient`; a local wrapper type that implements `SendScdcClient` by forwarding
  to it does.
- **docs.rs marks the `plumbob-async` implementation** as needing the feature (the
  `doc_cfg` badge, enabled only in docs.rs builds).
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
  culvert-async's own register address module is removed. The plumbob-async adapter's
  tests mirror culvert's: all seven `Update_0` flags read, each clearable flag written to
  its own bit, and all six `Source_Test_Configuration` bits.
- **End-to-end tests** of plumbob-async's training through `Scdc<T>`
  (`tests/plumbob_async.rs`), against a sink simulated at the register level.
- **A test checks the `Send` futures at compile time** — a function generic over
  `SendScdcTransport` and `SendHdmiPhy` must return a `Send` future that trains through
  `Scdc<T>`, runs LTS:L and trains again with events (both attempts must succeed: the
  simulated sink rearms after LTS:L), and another runs every `Scdc` operation that way.
- **culvert's public-API tests are ported** (`tests/scdc.rs`), with a test that names
  every type and trait the crate root re-exports.
- **The client tests check more of what they claim**, as culvert's now do: transport
  errors are matched as `ScdcError::Transport`; every CED lane gets a distinct,
  asymmetric count; the version tests use 0xA5 and 0x5A; `TestTransport` records the
  operations, so tests pin the registers, order and count of the multi-register reads
  and of `clear_update_flags`; and the end-to-end transports count the `Pending`s they
  return, so "the transport waited" is checked rather than assumed. The tests of
  culvert's own `FfeLevels` are left to culvert.
- **Publishing uses crates.io trusted publishing** — `publish.yml` obtains a short-lived
  token through OIDC (`rust-lang/crates-io-auth-action`, revoked at the end of the job, and
  short-lived regardless) instead of the long-lived `CARGO_REGISTRY_TOKEN` secret. The
  upload job runs in the `release` environment, which only `v*` tags can deploy to and
  which requires a maintainer's approval. Quality gates moved to a separate `verify` job,
  so no dependency build scripts run and no cache is restored where publishing credentials
  exist. The attested `.crate` is kept as a separate copy; after upload the job fails if
  crates.io's checksum differs from it, and that same copy is attached to the GitHub
  release. Re-running a partially failed publish reuses an existing GitHub release.
  `publish.yml` refuses tags that do not match the `Cargo.toml` version, and gains a
  `workflow_dispatch` trigger for `release-tag` (tags pushed with `GITHUB_TOKEN` start no
  push-triggered workflows); dispatches against a non-tag ref are skipped.
- **Release summary before approval** — `publish.yml` gains a `summary` job that shows the
  crate, version and commit, changes since the previous published release (ignoring
  pre-release or malformed tags, and flagging a baseline that is not on crates.io), changes
  to workflow, toolchain and build configuration, manifest changes, the changelog section,
  and the `.crate` sha256 with a command to reproduce it, on the run page while the publish
  job waits for approval. The publish job refuses to attest a package whose sha256 differs
  from the reviewed one.
- **Release workflows** — `release-prep` picks the version bump with `cargo-semver-checks`
  (`auto`, or checks an explicit bump), stamps the changelog, refreshes `Cargo.lock` and
  opens a `Release vX.Y.Z` PR. `release-tag` checks that CI passed on `develop` (and says so
  when the run has not finished yet), promotes `develop` to `main` through a
  `release-promote/v*` branch and PR, fails if that merge fails, tags only when `main` holds
  exactly the promoted tree, and dispatches `publish.yml` for the tag; it also accepts a
  squash-merged release commit (`Release vX.Y.Z (#N)`). Both fail when started from a branch
  other than `develop`. Releases were previously tagged by hand.
- **No CI run on promotion PRs** — CI no longer runs on pull requests into `main`. The only
  ones are `release-tag`'s `release-promote/*` PRs, whose content already passed CI on
  `develop`; opened by `github-actions[bot]`, their runs would be held for approval and
  listed as `action_required` or failed.
- **Supply-chain hardening for CI** — every third-party action is pinned to a commit SHA
  of a release tag, with `dependabot.yml` keeping the pins and `Cargo.lock` current
  (lockfile-only: version requirements stay a deliberate choice). The Rust toolchain itself
  still follows `stable`. `cargo-semver-checks` and `cargo-llvm-cov` are installed as
  checksum-verified prebuilt binaries by the pinned `install-action`. The security audit
  also runs weekly, with read-only repository permissions.
- **The documentation is built as docs.rs builds it, weekly** — `docs.yml` runs
  `cargo doc --all-features` with nightly and `--cfg docsrs` (and `-D warnings`), so a
  nightly change to the `doc_cfg` feature badges shows up before it breaks a docs.rs build.
- **CI runs the example**, not only builds it (`cargo run --example scdc`), in `ci.yml` and
  in the publish workflow's `verify` job.
- **README documents release provenance** — a badge shows whether crates.io requires trusted
  publishing for this crate, and "Verifying releases" now states which versions carry SLSA
  provenance and are uploaded through trusted publishing (from 0.2.0; it previously claimed
  every release was attested), that 0.1.0 was uploaded by hand with an API token, and how
  to look up a version's publishing details on crates.io.

## [0.1.0] - 2026-04-05

### Added

**Async SCDC client**

- `Scdc<T>` — stateless async typed client wrapping an
  `hdmi_hal_async::scdc::ScdcTransport`. Structurally identical to `culvert::Scdc<T>`
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

- Implements `plumbob_async::ScdcClient` for `Scdc<T>`, bridging `write_frl_config`,
  `read_training_status`, and `read_ced` to the `plumbob-async` trait vocabulary.
  Enabled by `features = ["plumbob-async"]`.

**Safety and portability**

- `#![no_std]` — fully `no_std` compatible with no `alloc` requirement.
- `#![forbid(unsafe_code)]` — no unsafe code anywhere in the crate.
- 100% line coverage measured with `cargo-llvm-cov`; baseline stored in
  `.coverage-baseline`.
- Unit tests using `#[pollster::test]` as the test executor; every test from `culvert`
  has an async counterpart here.
