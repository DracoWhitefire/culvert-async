# Testing Strategy

culvert-async's test suite is built around deterministic register-access tests. All tests
run against in-memory transport implementations; no real hardware is required at any point.

## Test structure

Tests are inline unit tests in each `src/client/` module. There are no separate integration
tests: the public API surface is thin (one re-exported struct, typed methods) and the unit
tests already exercise it through the crate boundary via `pub use` re-exports.

### Unit tests (`src/client/`)

Each client module contains tests immediately below the code it covers. They use
`TestTransport`, a 256-byte register array backed transport defined in
`src/client/test_transport.rs`.

`TestTransport` has two constructors:

- `TestTransport::new()` — succeeds on all operations; used for happy-path tests.
- `TestTransport::failing_after(n)` — succeeds for the first `n` operations then returns
  `Err(())`; used to exercise every `?` error branch.

The single generic instantiation (`Scdc<TestTransport>`) is intentional: using one
concrete type avoids LLVM counting per-monomorphisation `?` branches as uncovered, which
would inflate the coverage denominator without corresponding tests.

Tests run under `#[pollster::test]` with `async fn` bodies, keeping the executor
dependency out of the library and avoiding any runtime choice.

Each register group has tests covering:

- **Encoding correctness** — every field of a write config maps to the correct bit
  position in the output register(s). One assertion per field, not per struct.
- **Decoding correctness** — every field of a read result is extracted from the correct
  bit position. Register values are crafted to isolate individual bits where necessary.
- **Protocol errors** — invalid enum values returned by the sink produce the correct
  `ProtocolError` variant with the raw register value preserved (e.g. all eleven
  undefined `LtpReq` nibbles 5–15 are each tested individually).
- **Transport error propagation** — every read and write call site has a
  `TestTransport::failing_after(n)` test that triggers failure at that exact operation
  and asserts the error bubbles through as `ScdcError::Transport`.

### `plumbob-async` feature tests (`src/client/plumbob_client.rs`)

When compiled with `--features plumbob-async`, additional tests exercise the
`ScdcClient` implementation. These call methods through the `plumbob_async::ScdcClient`
trait and assert that the type conversions between culvert-async and plumbob-async's owned
types are correct. Error propagation through the trait boundary is also covered.

## Coverage

CI measures line coverage with `cargo-llvm-cov`. The baseline is stored in
`.coverage-baseline` (currently 100%); CI fails if coverage drops more than 0.1% below
it. New register coverage without tests will trip this.

## Philosophy

`Scdc<T>` runs identically against simulated and real `ScdcTransport` implementations.
A test that cannot run with an in-memory transport does not belong in this repository.
Hardware is never a test dependency. The async nature of the transport does not change
this: `pollster` drives the executor in tests without requiring a tokio or embassy runtime.
