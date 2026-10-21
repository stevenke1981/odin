#!/bin/sh
set -u
export PATH="/opt/odin-rust-1.90/bin:$PATH"
export CARGO_HOME=/root/odin-cargo-home
export CARGO_TARGET_DIR=/root/odin-linux-target
export CARGO_BUILD_JOBS=8
export CC_x86_64_unknown_linux_musl=musl-gcc
export AR_x86_64_unknown_linux_musl=ar
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc
cd /root/odin-diagnostic || exit 98
git rev-parse HEAD
git diff --exit-code HEAD -- || exit 97
rustc --version
cargo build -p odin-agent --release --target x86_64-unknown-linux-musl --locked --offline
result=$?
printf '%s\n' "$result" > /root/odin-diagnostic-build.exit
exit "$result"
