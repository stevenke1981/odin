#!/usr/bin/env bash
# Run on the actual Linux/macOS target. No cross-runtime verification is implied.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ -f Cargo.lock ]] || { echo 'A reviewed Cargo.lock is required' >&2; exit 1; }
cargo build -p odin-agent --release --locked
target/release/odin metadata > /dev/null
output="${1:-dist}"
mkdir -p "$output"
python3 scripts/package_tar.py --binary target/release/odin --output "$output"
