#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

cargo fmt --all -- --check
./scripts/architecture-check.sh
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p ferriso-platform --no-default-features --all-targets -- -D warnings
cargo test --workspace
cargo test -p ferriso-platform --no-default-features
cargo check --workspace --all-targets
