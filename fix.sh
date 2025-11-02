#!/usr/bin/env bash
# This scripts runs various CI-like checks in a convenient way.
set -eu

cargo fmt --all -q
cargo fix --lib -p simu --allow-dirty
