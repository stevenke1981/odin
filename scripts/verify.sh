#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p verification
log="verification/local-verify.log"
: > "$log"
run() { printf '\n>>> ' | tee -a "$log"; printf '%q ' "$@" | tee -a "$log"; printf '\n' | tee -a "$log"; "$@" 2>&1 | tee -a "$log"; }
[[ -f Cargo.lock ]] || { echo 'NOT_RUN: generate and review a real Cargo.lock with bootstrap.sh first.' >&2; exit 1; }
export ODIN_PYTHON="$(command -v python3)"
run rustc --version
run cargo --version
run python3 scripts/check_source.py --output verification/local-source-check.json
run python3 scripts/fixture_selftest.py --output verification/local-fixture-selftest.json
run cargo fmt --all -- --check
run cargo test --workspace --locked
run cargo clippy --workspace --all-targets --locked -- -D warnings
run cargo build --workspace --release --locked
run python3 scripts/mcp_smoke.py --binary target/release/odin
run python3 scripts/http_smoke.py --binary target/release/odin
printf '\nLocal gates passed. This does NOT prove real LSP / DAP / all-language parity.\n' | tee -a "$log"
