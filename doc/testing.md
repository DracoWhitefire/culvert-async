# Testing Strategy

culvert-async's test suite is built around deterministic register-access tests. All tests
run against in-memory transport implementations; no real hardware is required at any point.

## Test structure

### Unit tests (`src/client/`)

Each client module contains tests immediately below the code it covers. They are culvert's
tests for the same module, run against the async client: `#[pollster::test]` with
`async fn` bodies, `.await` on each `Scdc` call. They therefore check the same register
bits as culvert's, through culvert-async's own reads and writes — which is what this crate
adds; the encoding and decoding behind them is `culvert::codec`'s.

They use `TestTransport`, an async 256-byte register array transport defined in
`src/client/test_transport.rs`, with two constructors:

- `TestTransport::new()` — succeeds on all operations; used for happy-path tests.
- `TestTransport::failing_after(n)` — succeeds for the first `n` operations then returns
  `Err(())`; used to exercise every `?` error branch.
- `TestTransport::log()` — the first 16 operations, in order, so a test can pin which
  registers a method reads and writes, in what order and how often. A `read_block` is one
  `Op::ReadBlock(reg, len)`, counted once by `failing_after` (an empty one not at all), so
  a test pins each multi-register method to one transaction.

The unit tests use a single instantiation (`Scdc<TestTransport>`) on purpose: one
concrete type avoids LLVM counting per-monomorphisation `?` branches as uncovered. The
integration tests in `tests/` add their own transports, which the coverage run includes;
their `?` error branches are covered by the unit tests' instantiation.

Tests run under `#[pollster::test]`, keeping the executor dependency out of the library and
avoiding any runtime choice.

### Public API tests (`tests/scdc.rs`)

culvert's `tests/scdc.rs` made async: the client's operations through the crate root,
against `SimulatedScdc`, a 256-byte register array implementing culvert-async's
re-exported `ScdcTransport`. Each test pre-loads register state, runs operations and checks
both the returned values and the registers. One more test names every type and trait the
crate root re-exports, so a re-export that goes missing fails to compile, and one runs
every operation generic over a `SendScdcTransport`, passing each future through a function
that requires `Send`, so it compiles only if every operation could run on a multi-threaded
executor.

### Torn reads (`tests/torn_reads.rs`)

culvert's regression tests for the torn CED read (review B13 of the FRL rework), made
async: a sink whose lane 0 counter ticks while it is read, with a checksum kept correct for
its current value. Read one register at a time, 0x00FF ticking to 0x0100 reads back as
0x01FF with a `ChecksumMismatch`; read as one block, during which the sink holds its
registers still, it is 0x00FF with no warning; a sweep over starting counts shows a
counter ticking between bytes never passes the checksum.

The CED unit tests (`src/client/ced.rs`) match `CedWarning`s by their fields rather than
building them: the variants are `#[non_exhaustive]`, so outside culvert they can be
matched but not constructed, as a caller meets them.

### `plumbob-async` feature tests

When compiled with `--features plumbob-async`, two more sets of tests run, both culvert's
`plumbob` feature tests made async:

- **Unit tests** (`src/client/plumbob_client.rs`) call each method through the
  `plumbob_async::ScdcClient` trait and assert on register bits: all seven `Update_0`
  flags, every request value (and 0x9 passed on as `LtpReq::Reserved`), all six
  `Source_Test_Configuration` bits, each flag clear writing only its own bit, `Config_1`
  encoding, culvert's FFE-levels check, and transport errors.
- **End-to-end tests** (`tests/plumbob_async.rs`) run plumbob-async's `FrlTrainer`
  through `Scdc<T>` against a sink simulated at the register level: a full training run,
  a rate drop, a timeout and exhausted rates ending in TMDS, `FLT_no_timeout`, an
  undefined request ignored and returned as a `TrainingWarning`, a 3-lane link whose
  unused lane 3 reports an undefined value, `FRL_start` and `FLT_update` set together
  (recording `FrlStartWithRetrain`), a DDC NACK mid-LTS:3 still ending in LTS:L,
  a retrain resuming from no pattern, the `Update_0` flags plumbob does not handle left
  set, and the FFE maximum limited per rate. The simulated transport waits (returns
  `Pending` once) on every other operation, so training runs over I/O that really
  suspends.
- **`Send` futures** (also `tests/plumbob_async.rs`): a function generic over
  `SendScdcTransport` and `SendHdmiPhy` must return a `Send` future that trains through
  the plain `Scdc<T>`, runs LTS:L and trains again with events, so it compiles only if
  training through `Scdc<T>` could be spawned on a multi-threaded executor. It runs over
  the same simulated sink, behind a `Mutex` (the transport twin is `Sync`).

### Example (`examples/scdc/main.rs`)

`examples/scdc/main.rs` is culvert's example with an async transport, driven by
`pollster::block_on`. It exercises the public API against a simulated sink; its output is
identical to culvert's example, which is a quick check that both clients read the same
register map.

```sh
cargo run --example scdc
```

## Coverage

CI measures line coverage with `cargo-llvm-cov` over the default features and the
`plumbob-async` feature, merged into one report, so the adapter is covered too. The baseline
is stored in `.coverage-baseline` (100%); CI fails if coverage drops more than 0.1% below
it.

## Philosophy

`Scdc<T>` runs identically against simulated and real `ScdcTransport` implementations.
A test that cannot run with an in-memory transport does not belong in this repository.
Hardware is never a test dependency: `pollster` drives the tests without a Tokio or Embassy
runtime.
