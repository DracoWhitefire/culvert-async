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
  culvert-async gains the async method in its next release.

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

```sh
# a crate using both
cargo add culvert-async --features plumbob-async
cargo add plumbob-async
```

Each of the eight `ScdcClient` methods awaits one culvert-async method, exactly as
culvert's `plumbob` feature calls one culvert method. The conversions between culvert's
register types and plumbob's are culvert's own `From` impls — the feature enables
`culvert/plumbob` to get them — so the sync and async adapters convert identically.

**Polling and stopping early.** `Scdc` does not wait between calls. plumbob's limits
assume one poll every 2 ms by default, and plumbob's contract puts the wait inside the two
polled methods, `read_flt_ready` and `read_update_flags`. With `Scdc<T>` that means an
`ScdcClient` of your own that wraps it, waits in those two methods and forwards the rest:

```rust
use core::time::Duration;
use culvert_async::{Scdc, ScdcError, ScdcTransport};
use plumbob_async::{ScdcClient, UpdateFlags};

/// `Scdc<T>` that waits the poll interval in the two methods plumbob polls.
struct Paced<T>(Scdc<T>);

impl<T: ScdcTransport> ScdcClient for Paced<T> {
    type Error = ScdcError<T::Error>;

    async fn read_flt_ready(&mut self) -> Result<bool, Self::Error> {
        tokio::time::sleep(Duration::from_millis(2)).await; // or the executor's timer
        ScdcClient::read_flt_ready(&mut self.0).await
    }

    async fn read_update_flags(&mut self) -> Result<UpdateFlags, Self::Error> {
        tokio::time::sleep(Duration::from_millis(2)).await;
        ScdcClient::read_update_flags(&mut self.0).await
    }

    // … the other six methods forward without waiting
}
```

A wall-clock deadline or a cancel signal goes in the same two methods, following
plumbob's "Stopping early" rule (`plumbob::ScdcClient`): return one error of your own
(the wrapper's `Error` then wraps `ScdcError`), let every later call through, and never
fail a read made after a `write_frl_config` with `NotSupported` until a `write_frl_config`
with an FRL rate starts training again. Neither belongs in the transport: a wait there
applies to every register access, not only the polls, and a transport cannot tell the
polled reads from LTS:L's, so a deadline there can fail LTS:L itself and leave the sink in
FRL. `Paced<T>` stays `Send` over a `SendScdcTransport`, as `Scdc<T>` does.

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

`Scdc<T>`'s futures are `Send` when the transport's are and the transport is `Send` and
`Sync`: a future holds `&mut Scdc<T>`, and `ScdcTransport::read` takes `&self`, so a future
that reads holds `&T` as well. A transport with a
`Cell` or `RefCell` inside is therefore fine on a single-threaded executor and cannot be
spawned on a multi-threaded one.

Code generic over the plain `ScdcTransport` cannot prove the futures `Send` at all: an
`async fn` in a trait promises nothing about its future. Generic code bounds the
transport by hdmi-hal-async's `Send` twin, `SendScdcTransport` (`Send + Sync`, with `Send`
futures and errors), and the PHY by `SendHdmiPhy`. Then training through the plain
`Scdc<T>` is `Send`: `Scdc<T>`'s `ScdcClient` implementation is concrete, so the compiler
sees through it to the transport's futures. No second client type is needed, and a test
(`tests/plumbob_async.rs`) checks this at compile time. Bound by the twins alone: an added
`+ ScdcTransport` or `+ HdmiPhy` makes the compiler use that bound instead of the blanket
implementation, and the futures are no longer known to be `Send`.

```rust
use culvert_async::{Scdc, SendScdcTransport};
use plumbob_async::{FrlTrainer, HdmiForumFrl, SendHdmiPhy, TrainingConfig};

const RATES: &[HdmiForumFrl] = &[
    HdmiForumFrl::Rate12Gbps4Lanes,
    HdmiForumFrl::Rate6Gbps4Lanes,
];

fn spawn_training<T, P>(mut trainer: FrlTrainer<Scdc<T>, P>)
where
    T: SendScdcTransport + 'static,
    P: SendHdmiPhy + 'static,
{
    tokio::spawn(async move {
        let _ = trainer.train(RATES, &TrainingConfig::default()).await;
    });
}
```

Both twins are re-exported where the code needs them: `SendScdcTransport` from
culvert-async, `SendHdmiPhy` from plumbob-async. `train_with_events` is `Send` too when its
callback is.

**Cancellation.** Every method performs at most one write and keeps no state between
calls, so a dropped future leaves `Scdc<T>` usable (plumbob-async's `exit_to_tmds` then
returns a dropped training attempt to TMDS). The multi-register reads, `read_ced` and
`read_rs_correction`, read one register at a time and can tear; their documentation says
so.

**Code bounded by `SendScdcClient`.** plumbob-async's `SendScdcClient` is the client's
twin. `Scdc<T>` implements the plain `ScdcClient` for every transport and cannot also
implement `SendScdcClient` for `Send` ones: plumbob-async's blanket implementation would
give it `ScdcClient` twice (E0119). Code bounded by `C: SendScdcClient` (an integration
layer generic over the client, for example) therefore cannot take `Scdc<T>` directly. A
local wrapper that forwards each method can:

```rust
struct SendScdc<T>(Scdc<T>);

impl<T: SendScdcTransport> SendScdcClient for SendScdc<T> {
    type Error = ScdcError<T::Error>;

    async fn read_flt_ready(&mut self) -> Result<bool, Self::Error> {
        ScdcClient::read_flt_ready(&mut self.0).await
    }
    // … the other seven methods alike
}
```

culvert-async does not ship that wrapper yet; it will when a consumer generic over
`SendScdcClient` needs it, or when return-type notation is decided against (see
`doc/roadmap.md`; recorded in the stack design document, "Sync and Async Companions").

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
  adds its async counterpart in its next release.
- **Attested releases.** Every release is published through a GitHub Actions workflow
  that signs the `.crate` package with [SLSA Build Level 2](https://slsa.dev) provenance.
  Verify with `gh attestation verify <file> --repo DracoWhitefire/culvert-async`.

[`culvert::codec`]: https://docs.rs/culvert/latest/culvert/codec/index.html
