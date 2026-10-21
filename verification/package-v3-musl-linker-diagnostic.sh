set -u
export PATH="/opt/odin-rust-1.90/bin:$PATH"
mkdir -p /opt/odin-linker-diagnostic
printf '%s\n' 'fn main() { println!("native-musl-runtime-ok"); }' > /opt/odin-linker-diagnostic/hello.rs
for linker in musl-gcc cc; do
  rustc --edition 2024 --target x86_64-unknown-linux-musl -C "linker=$linker" /opt/odin-linker-diagnostic/hello.rs -o "/opt/odin-linker-diagnostic/hello-$linker"
  printf 'COMPILE_%s_EXIT=%s\n' "$linker" "$?"
  readelf -l "/opt/odin-linker-diagnostic/hello-$linker" | grep -E 'INTERP|interpreter' || true
  "/opt/odin-linker-diagnostic/hello-$linker"
  printf 'EXEC_%s_EXIT=%s\n' "$linker" "$?"
done
