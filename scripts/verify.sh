#!/usr/bin/env bash
# The check to run before every commit: formatting, lints and tests (which never touch Music).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check || { echo "Formatting differs. Fix: cargo fmt" >&2; exit 1; }
cargo clippy --all-targets -- -D warnings
cargo test --no-fail-fast
cargo build --release
echo "✓ all checks passed"
