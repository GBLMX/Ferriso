#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo fmt --all -- --check
./scripts/architecture-check.sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p ferriso-platform --no-default-features --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p ferriso-platform --no-default-features --locked
cargo check --workspace --all-targets --locked
