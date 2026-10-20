#!/usr/bin/env bash
# First dependency resolution. No system package installation is performed here.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p verification
log="verification/local-bootstrap.log"
: > "$log"
run() { printf '\n>>> ' | tee -a "$log"; printf '%q ' "$@" | tee -a "$log"; printf '\n' | tee -a "$log"; "$@" 2>&1 | tee -a "$log"; }
command -v cargo >/dev/null || { echo 'NOT_RUN: install the requested Rust toolchain first.' >&2; exit 1; }
command -v python3 >/dev/null || { echo 'NOT_RUN: Python 3.11+ is required for acceptance scripts.' >&2; exit 1; }
run rustc --version
run cargo --version
if [[ ! -f Cargo.lock ]]; then run cargo generate-lockfile; fi
run cargo fmt --all
run cargo build --workspace --locked
printf '\nBootstrap finished. Review Cargo.lock, then run scripts/verify.sh.\n' | tee -a "$log"
