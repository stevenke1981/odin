#!/usr/bin/env bash
# Run on the actual Linux/macOS target. No cross-runtime verification is implied.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ -f Cargo.lock ]] || { echo 'A reviewed Cargo.lock is required' >&2; exit 1; }
output="${1:-dist}"
mkdir -p "$output"
target_directory="${CARGO_TARGET_DIR:-target}"
if [[ "$(uname -s)" == Linux ]]; then
  [[ "$(uname -m)" == x86_64 ]] || { echo 'The Debian recipe currently requires Linux x86-64; use package_tar.py for other native targets.' >&2; exit 1; }
  export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-musl-gcc}"
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-musl-gcc}"
  command -v "$CC_x86_64_unknown_linux_musl" > /dev/null || { echo 'Install musl-tools and the pinned Rust x86_64-unknown-linux-musl standard library before packaging.' >&2; exit 1; }
  cargo build -p odin-agent --release --target x86_64-unknown-linux-musl --locked
  binary="$target_directory/x86_64-unknown-linux-musl/release/odin"
  binary_sha="$(python3 -c 'import hashlib,pathlib,sys; print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())' "$binary")"
  python3 scripts/package_linux.py --binary "$binary" --expected-binary-sha256 "$binary_sha" --output "$output"
else
  cargo build -p odin-agent --release --locked
  binary="$target_directory/release/odin"
  python3 scripts/package_tar.py --binary "$binary" --output "$output"
fi
