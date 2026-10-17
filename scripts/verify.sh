#!/usr/bin/env bash
# The check to run before every commit: formatting, lints and tests (which never touch Music).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check || { echo "Formatting differs. Fix: cargo fmt" >&2; exit 1; }
cargo clippy --all-targets -- -D warnings
cargo test --no-fail-fast
cargo build --release
# The /cctune plugin: checked where Claude Code is installed (its tests never run cctune).
if command -v claude > /dev/null; then
  claude plugin validate claude-plugin > /dev/null
  claude plugin validate . > /dev/null
  claude plugin test claude-plugin
else
  echo "skipped the /cctune plugin checks: Claude Code is not installed"
fi
echo "✓ all checks passed"
