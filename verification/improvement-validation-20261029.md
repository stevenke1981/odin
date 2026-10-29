# Improvement validation, 2026-10-29

Windows local run against the working tree based on `c78ca8d002a6f42ee95cad74712b0c943f0918a8`. The debug executable SHA-256 was `81939de30efff90a42c28dc02a0b1427469146fc120d8777da0650c7785a27ce`.

- `cargo fmt --all -- --check`: exit 0.
- `cargo test --workspace --locked`: exit 0; six tests ignored (three native MXC and three real backend probes).
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0.
- `cargo build --workspace --locked`: exit 0.
- `python scripts/cli_smoke.py --binary target/debug/odin.exe`, `mcp_smoke.py`, `http_smoke.py` and `mcp_cancel_smoke.py` with the same binary: each exit 0. Cancellation uses a protocol fixture; no real LSP is inferred from that smoke.
- `python tests/real_ide_backends.py --binary target/debug/odin.exe --output verification/improvement-real-ide-20261029.json --languages rust python typescript --timeout 60`: exit 1, PARTIAL; 128 PASS, 6 UNAVAILABLE, 6 NOT_VERIFIED. Rust and Python two-project matrices passed; TypeScript diagnostics without verified versions remain NOT_VERIFIED. See the [raw report](improvement-real-ide-20261029.json) and its request transcript.

Real desktop client reconnect, clean-machine install/upgrade/crash tests, multi-scale benchmark, retention/restore implementation, supply-chain rebuild and non-Windows validation remain NOT_RUN or incomplete. This record does not supersede historical candidate evidence or certify a release.
