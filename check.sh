#!/usr/bin/env bash
# This scripts runs various CI-like checks in a convenient way.
set -eu

cargo check --quiet --workspace --all-targets
cargo check --quiet --workspace --all-features --lib --target wasm32-unknown-unknown
cargo fmt --all -- --check
cargo clippy --quiet --workspace --all-targets --all-features --  -D warnings -W clippy::all
cargo test --quiet --workspace --all-targets --all-features > /dev/null
cargo test --quiet --workspace --doc > /dev/null

env -u NO_COLOR trunk build --quiet > /dev/null
