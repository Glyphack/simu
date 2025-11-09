#!/usr/bin/env bash
# This script automatically fixes issues found by check.sh
set -eu

# Format all code
cargo fmt --all -q

# Fix all auto-fixable issues (works even with staged/dirty files)
cargo fix --quiet --workspace --all-targets --allow-dirty --allow-staged

# Fix clippy warnings (works even with staged/dirty files)
cargo clippy --quiet --workspace --all-targets --all-features --fix --allow-dirty --allow-staged -- -D warnings -W clippy::all
