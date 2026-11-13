# Development Setup

**Requirements:** Rust 1.85+ (stable). Install via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/ashlar-display/culvert-async.git
cd culvert-async
cargo build
```

## Running checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features plumbob-async -- -D warnings
cargo rustdoc --all-features -- -D warnings
```

## Running tests

```sh
cargo test                              # default features
cargo test --features plumbob-async    # with plumbob_async::ScdcClient impl
```

The README's example runs as a doctest with the others.

## `no_std` builds

The crate is `no_std` without `alloc`. CI checks it on a bare target, which has no `std`
for a dependency to pull in:

```sh
rustup target add thumbv7em-none-eabi
cargo build --target thumbv7em-none-eabi
cargo build --features plumbob-async --target thumbv7em-none-eabi
```

## Checking the MSRV

The MSRV is `rust-version` in `Cargo.toml`. CI checks it with
[`cargo-msrv`](https://github.com/foresterre/cargo-msrv), which runs the checks on that
toolchain:

```sh
cargo install cargo-msrv --locked
cargo msrv verify -- cargo test --locked
cargo msrv verify -- cargo test --locked --features plumbob-async
cargo msrv verify --target thumbv7em-none-eabi \
  -- cargo build --locked --no-default-features --target thumbv7em-none-eabi
cargo msrv verify --target thumbv7em-none-eabi \
  -- cargo build --locked --no-default-features --features plumbob-async --target thumbv7em-none-eabi
```

## Measuring coverage

Coverage requires [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```sh
cargo install cargo-llvm-cov
cargo llvm-cov --all-features
```

The current baseline is stored in `.coverage-baseline`. CI fails if coverage drops more
than 0.1% below it. On pushes to `main` or `develop`, an improvement automatically opens
a `ci/coverage-ratchet` PR to commit the new baseline.

## Running the example

```sh
cargo run --example scdc
```
