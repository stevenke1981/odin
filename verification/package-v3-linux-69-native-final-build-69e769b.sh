#!/bin/sh
set -u
export PATH="/opt/odin-rust-1.90/bin:$PATH"
export CARGO_HOME=/home/odin-qa/.cargo
export CARGO_TARGET_DIR=/root/odin-linux-target
export CARGO_BUILD_JOBS=4
export CC_x86_64_unknown_linux_musl=musl-gcc
export AR_x86_64_unknown_linux_musl=ar
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=cc
export ODIN_PYTHON=/usr/bin/python3
export RUST_TEST_THREADS=1
export CARGO_NET_OFFLINE=true
delivery=/opt/odin-delivery/v3-69e769b
mkdir -p "$delivery" || exit 98
cd /opt/odin-final || exit 98
test "$(git rev-parse HEAD)" = 69e769be9baa4e825139b72ce1a9b48bf8143114 || exit 97
test -z "$(git status --porcelain --untracked-files=no)" || exit 97
{
  id
  uname -a
  rustc --version --verbose
  cargo --version
  cc --version
  musl-gcc --version
  git rev-parse HEAD
  git status --porcelain
  sha256sum Cargo.lock
  env | sort | sed -n '/^AR_x86_64_unknown_linux_musl=/p;/^CARGO_/p;/^CC_x86_64_unknown_linux_musl=/p;/^ODIN_PYTHON=/p;/^PATH=/p;/^RUST_TEST_THREADS=/p'
} > "$delivery/build-environment.txt" 2>&1
run_stage() {
  stage=$1
  shift
  start=$(date +%s)
  printf '%s\n' "$*" > "$delivery/$stage.command"
  "$@" > "$delivery/$stage.log" 2>&1
  result=$?
  elapsed=$(($(date +%s)-start))
  printf '%s\n' "$result" > "$delivery/$stage.exit"
  printf '%s\n' "$elapsed" > "$delivery/$stage.seconds"
  return "$result"
}
run_stage fmt cargo fmt --all -- --check || exit $?
run_stage build cargo build -p odin-agent --release --target x86_64-unknown-linux-musl --locked --offline || exit $?
cp "$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/release/odin" "$delivery/odin" || exit 96
sha256sum "$delivery/odin" > "$delivery/binary.sha256"
run_stage metadata "$delivery/odin" metadata || exit $?
run_stage elf readelf -l -d "$delivery/odin" || exit $?
run_stage tests cargo test --workspace --target x86_64-unknown-linux-musl --locked --offline -- --test-threads=1
test_result=$?
run_stage clippy cargo clippy --workspace --all-targets --target x86_64-unknown-linux-musl --locked --offline -- -D warnings
clippy_result=$?
git status --porcelain > "$delivery/source-status-after.txt"
if [ "$test_result" -ne 0 ]; then exit "$test_result"; fi
exit "$clippy_result"
