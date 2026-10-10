# Architecture

## Role

`culvert-async` is the async companion to `culvert`: typed access to the HDMI 2.1 SCDC
register map over an `hdmi_hal_async::scdc::ScdcTransport`. Its `Scdc<T>` has culvert's
methods as `async fn`s.

The register map itself is not duplicated. Every address, bit position and protocol check
lives in [`culvert::codec`], the I/O-free part of culvert; culvert-async's methods only
perform the reads and writes, `.await`ing the transport, and hand the bytes to `codec`.
The sync and async clients therefore share one register map: a fix in culvert reaches
both. The stack design document's "Sync and Async Companions" section records why.

The crate also implements `plumbob_async::ScdcClient` for `Scdc<T>`, gated behind a
`plumbob-async` cargo feature: the async counterpart to culvert's `plumbob` feature, and
the hook that connects culvert-async to `plumbob-async`'s training.

All register types and error types are re-exported from `culvert`, so callers do not need
both crates.

---

## Scope

culvert-async covers:

- `Scdc<T>`: the client, wrapping an async `ScdcTransport`, with culvert's methods as
  `async fn`s:
  - version negotiation: `read_sink_version`, `write_source_version`,
  - scrambling control: `write_tmds_config`, `read_scrambler_status`,
  - update flags: `read_update_flags`, `clear_update_flags`,
  - FRL configuration and training: `write_config_0`, `write_frl_config`,
    `read_source_test_config`, `read_status_flags`, `read_ltp_requests`,
  - error counters: `read_ced`, `read_rs_correction`,
- re-exports of culvert's public types (`ScdcError`, `ProtocolError`, `TmdsConfig`,
  `ScramblerStatus`, `Config0`, `FrlConfig`, `FfeLevels`, `FrlRate`, `SourceTestConfig`,
  `StatusFlags`, `LtpReq`, `LtpRequests`, `UpdateFlags`, `ClearableUpdateFlags`,
  `CedCount`, `CedCounters`, `RsCorrectionCount`), and of hdmi-hal-async's
  `ScdcTransport`, the bound on `Scdc<T>`,
- the `plumbob_async::ScdcClient` implementation for `Scdc<T>`, behind the `plumbob-async`
  cargo feature.

The following are out of scope:

- **Protocol logic** — register addresses, bit encoding and decoding, and protocol checks
  are `culvert::codec`'s. culvert-async adds no protocol logic.
- **Sync API** — callers on sync transports use `culvert` directly.
- **Link training state machine** — `plumbob`'s, which `plumbob-async` drives.
- **I²C / DDC transport** — platform backends implement `hdmi_hal_async::scdc::ScdcTransport`.
  culvert-async never touches I²C directly.
- **New register coverage** — registers on culvert's roadmap are added to culvert first;
  culvert-async gains the async method in the same release.

---

## Dependencies

```
display-types  ─┐
hdmi-hal       ─┼─►  culvert          ─┐
                │                      │
hdmi-hal       ─┴─►  hdmi-hal-async  ─┴─►  culvert-async  ──►  plumbob-async
```

- `culvert` — the register map (`culvert::codec`) and all shared types, re-exported here.
- `hdmi-hal-async` — the async `ScdcTransport` trait.
- `plumbob-async` (optional) — the `ScdcClient` trait that `Scdc<T>` implements. The
  feature also enables culvert's `plumbob` feature, for its type conversions.

---

## The `Scdc<T>` Client

`Scdc<T>` is culvert's `Scdc<T>` with an async transport: a thin, stateless client that
owns the transport.

```rust
impl<T: hdmi_hal_async::scdc::ScdcTransport> Scdc<T> {
    pub fn new(transport: T) -> Self;
    pub fn into_transport(self) -> T;

    pub async fn read_sink_version(&mut self) -> Result<u8, ScdcError<T::Error>>;
    pub async fn write_frl_config(&mut self, config: FrlConfig) -> Result<(), ScdcError<T::Error>>;
    pub async fn read_ltp_requests(&mut self) -> Result<LtpRequests, ScdcError<T::Error>>;
    pub async fn read_ced(&mut self) -> Result<CedCounters, ScdcError<T::Error>>;
    // … one async method per culvert method
}
```

Each method reads or writes the registers `culvert::codec` names and converts with the
matching `codec` function:

```rust
pub async fn read_ced(&mut self) -> Result<CedCounters, ScdcError<T::Error>> {
    let mut bytes = [0; 8];
    for (byte, reg) in bytes.iter_mut().zip(codec::CED_REGISTERS) {
        *byte = self.transport.read(reg).await.map_err(ScdcError::Transport)?;
    }
    Ok(codec::decode_ced(bytes))
}
```

The semantics of every method are culvert's; the only difference is that register
accesses yield to the executor rather than blocking.

---

## The `plumbob-async` Feature

culvert-async implements `plumbob_async::ScdcClient` for `Scdc<T>`, gated behind a
`plumbob-async` cargo feature.

```toml
# Cargo.toml of a crate using both
culvert-async  = { version = "0.1", features = ["plumbob-async"] }
plumbob-async  = "0.1"
```

Each of the eight `ScdcClient` methods awaits one culvert-async method, exactly as
culvert's `plumbob` feature calls one culvert method. The conversions between culvert's
register types and plumbob's are culvert's own `From` impls — the feature enables
`culvert/plumbob` to get them — so the sync and async adapters convert identically.
`Scdc` does not wait between calls: the poll interval plumbob's limits assume (2 ms by
default) belongs in the transport, where an async implementation can await a timer.

---

## Module Structure

```
src/
  lib.rs               — #![no_std], re-exports, module declarations, crate-level docs
  client/
    mod.rs             — Scdc<T>, new, into_transport
    version.rs         — read_sink_version, write_source_version
    scrambling.rs      — write_tmds_config, read_scrambler_status
    update.rs          — read_update_flags, clear_update_flags
    frl.rs             — write_config_0, write_frl_config, read_source_test_config,
                         read_status_flags, read_ltp_requests
    ced.rs             — read_ced, read_rs_correction
    plumbob_client.rs  — plumbob_async::ScdcClient impl (cfg feature = "plumbob-async")
    test_transport.rs  — async TestTransport (cfg test)
tests/
  plumbob_async.rs     — end-to-end training through Scdc<T> (cfg feature = "plumbob-async")
```

The layout matches culvert's module for module, and the client modules, their tests and
the example are culvert's with `async`/`.await`, so the two crates stay easy to compare.

---

## `no_std` Compatibility

culvert-async is `#![no_std]`. The async transport trait and all return types require no
allocator. `Scdc<T>` holds only the transport; `ScdcError<E>` is stack-allocated. The full
API is available in bare `no_std` environments, consistent with culvert and the rest of
the stack. CI builds the crate, with and without the `plumbob-async` feature, for
`thumbv7em-none-eabi`, a target without `std`.

`#![allow(async_fn_in_trait)]` is not needed in culvert-async: the crate implements async
traits but does not define them. The lint applies to trait definitions, not to implementors.

---

## `Send` futures

`Scdc<T>`'s futures are `Send` when the transport's are and the transport is `Sync`:
`ScdcTransport::read` takes `&self`, so a future that reads holds `&T`. A transport with a
`Cell` or `RefCell` inside is therefore fine on a single-threaded executor and cannot be
spawned on a multi-threaded one. As with `hdmi-hal-async` and `plumbob-async`, code
generic over the transport cannot prove its futures `Send` at all. The stack design
document ("Sync and Async Companions", "`Send` futures for multi-threaded executors")
records how this would be lifted — including why it would take a second client type next
to `Scdc<T>` — and why it is deferred.

---

## Design Principles

The principles are culvert's:

- **One register map.** Addresses, bit positions and protocol checks are `culvert::codec`'s;
  culvert-async adds only the async I/O, so it cannot drift from culvert.
- **Type reuse over duplication.** All register types and error types live in `culvert`
  and are re-exported here.
- **Stateless client, stateful caller.** `Scdc<T>` holds no protocol state. The async
  nature of the transport does not change this.
- **Deterministic and testable.** The async `TestTransport` supports the same pre-loaded
  register array and `failing_after` pattern as culvert's sync version. No hardware required.
- **Interface owned by the consumer.** `plumbob_async::ScdcClient` is defined in
  `plumbob-async`; culvert-async implements it. The cargo feature gates the impl so
  culvert-async remains independently usable without the training layer as a dependency.
- **No unsafe code.** `#![forbid(unsafe_code)]`.
- **Parity with culvert.** When culvert adds a method or register group, culvert-async
  adds its async counterpart in the same release.
- **Attested releases.** Every release is published through a GitHub Actions workflow
  that signs the `.crate` package with [SLSA Build Level 2](https://slsa.dev) provenance.
  Verify with `gh attestation verify <file> --repo DracoWhitefire/culvert-async`.

[`culvert::codec`]: https://docs.rs/culvert/latest/culvert/codec/index.html
