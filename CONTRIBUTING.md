# Contributing to culvert-async

Thanks for your interest in contributing. This document covers the basics.

## Getting started

Relevant docs for contributors:

- [`doc/setup.md`](doc/setup.md) — build, test, and coverage commands
- [`doc/testing.md`](doc/testing.md) — testing strategy, transport harness, and CI expectations
- [`doc/architecture.md`](doc/architecture.md) — role, scope, module structure, design principles, and the culvert-async / plumbob-async boundary

## Issues and pull requests

**Open an issue first** if you're unsure whether something is a bug or if you want to
discuss a change before implementing it. For small, self-contained fixes a PR on its own
is fine.

- Bug reports: describe which register group is affected, what value the sink returned,
  and what culvert-async did with it.
- Feature requests: a brief description of what you need and why is enough to start a
  conversation. Note that new register coverage is only added here when `culvert` gains
  the same method — the two crates move in lockstep.
- PRs: keep them focused. One logical change per PR makes review faster and keeps history
  readable.

## Coding standards

- Run `cargo fmt` and `cargo clippy -- -D warnings` before pushing.
- Public items need rustdoc comments (`cargo rustdoc -- -D missing_docs` must pass).
- Follow the existing patterns in the codebase — see [`doc/architecture.md`](doc/architecture.md)
  for the design principles behind them.
- `#![forbid(unsafe_code)]` is enforced; no unsafe code.
- Keep `no_std` compatibility. `Scdc<T>` and all output types must compile without `alloc`
  or `std`.
- culvert-async adds no protocol logic. Bit decoding, register encoding, and error
  classification belong in `culvert`; this crate only adds the `async` dispatch layer.

## Commit and PR expectations

- Write commit messages in the imperative mood ("Add support for …", not "Added …").
- Keep commits logically atomic. A PR that touches three unrelated things should be three
  commits (or three PRs).
- Tests are expected for new register coverage. Pre-loading a `TestTransport` and
  asserting on the decoded struct is the established pattern; see the existing `client`
  modules for examples. Tests use `#[pollster::test]` and `async fn` bodies.
- CI must be green before a PR can merge: fmt, clippy, docs, all test and build targets,
  and coverage must not drop more than 0.1% below the baseline (stored in
  `.coverage-baseline`). New register coverage without tests will likely trip this.

## Review process

PRs are reviewed on a best-effort basis. Expect feedback within a few days; if you haven't
heard back in a week feel free to ping the thread. Reviews aim to be constructive — if
something needs to change, the reviewer will explain why. Approval from the maintainer is
required to merge.

## Code of Conduct

This project follows the [Contributor Covenant 3.0](CODE_OF_CONDUCT.md). Please read it
before participating.
