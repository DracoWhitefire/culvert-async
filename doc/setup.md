# Development Setup

**Requirements:** Rust 1.85+ (stable). Install via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/DracoWhitefire/culvert-async.git
cd culvert-async
cargo build
```

## Running checks

```sh
cargo fmt --check
cargo clippy --all-features -- -D warnings
cargo rustdoc --all-features -- -D missing_docs
```

## Running tests

```sh
cargo test                              # default features
cargo test --features plumbob-async    # with plumbob_async::ScdcClient impl
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
