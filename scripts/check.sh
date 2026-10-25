#!/usr/bin/env bash
# Local preflight; ordinary compilation does not execute tests.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo test --doc --locked
cargo build --locked --bins
case "$(uname -s)" in
  Linux|Darwin) python3 tests/terminal_smoke.py ;;
esac
