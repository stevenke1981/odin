#!/usr/bin/env bash
# Run on the actual Linux/macOS target. No cross-runtime verification is implied.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ -f Cargo.lock ]] || { echo 'A reviewed Cargo.lock is required' >&2; exit 1; }
cargo build -p odin-agent --release --locked
target/release/odin metadata > /dev/null
output="${1:-dist}"
mkdir -p "$output"
binary_sha="$(sha256sum target/release/odin | cut -d ' ' -f 1)"
if [[ "$(uname -s)" == Linux ]] && target/release/odin metadata | python3 -c 'import json,sys; sys.exit(json.load(sys.stdin)["target"]!="x86_64-unknown-linux-musl")'; then
  python3 scripts/package_linux.py --binary target/release/odin --expected-binary-sha256 "$binary_sha" --output "$output"
else
  python3 scripts/package_tar.py --binary target/release/odin --expected-binary-sha256 "$binary_sha" --output "$output"
fi
