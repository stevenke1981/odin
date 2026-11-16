# Odin engineering rules

## Quick reference for coding agents

- Toolchain: Rust 1.90.0 (`rust-toolchain.toml`), edition 2024; Windows needs the MSVC linker.
  Python 3 is only for scripts/acceptance (`python` must resolve to Python 3).
- Build: `cargo build --workspace --release --locked` (binary `target/release/odin(.exe)`).
- Fast gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`. Full local gate: `scripts/verify.ps1` / `scripts/verify.sh`
  (adds source checks and real-binary MCP/HTTP/CLI smokes; it rewrites `verification/local-*.json|log`).
- Tool catalog: edit `scripts/generate_catalog.py` (contracts, `DESCRIPTIONS`, `CORE`/`IDE` tiers), then
  `python scripts/generate_catalog.py && python scripts/generate_docs.py`; `python scripts/check_generated.py` must pass.
  Never hand-edit `schemas/tools.json`, `crates/odin-engine/tools.json` or `docs/API.md`.
- `MANIFEST.sha256` lists shipped file hashes (`python scripts/verify_manifest.py`); refresh entries for files you change.
- Layout: `crates/odin-core` (config, paths, transactions, memories, sandbox), `crates/odin-protocol`
  (LSP/DAP JSON-RPC), `crates/odin-engine` (tool dispatch `src/lib.rs`, schema validation), `crates/odin`
  (CLI `main.rs`, MCP `mcp.rs` incl. tool profiles, `setup.rs` client configs, `http.rs`).
  `scripts/` generators and smokes, `tests/` real-backend acceptance harnesses, `examples/` host configs.
- `verification/` holds 1000+ historical evidence files. Do not read them for routine work; consult
  `verification/delivery-status.json` only when a task concerns release status or evidence.
- Agent-facing usage: [docs/CLAUDE_CODE.md](docs/CLAUDE_CODE.md). Docs are mostly zh-TW; tool descriptions are English.

## Rules

- For architecture or release work, read README, docs/ARCHITECTURE.md, docs/KNOWN_LIMITATIONS.md and verification/delivery-status.json first.
- No fake test results. A test written is not a test executed. Preserve NOT_RUN until there is a real command log.
- Route all native project mutations through odin-core Transactions. Require expected hashes; never overwrite a conflict to obtain a green result.
- Every tool needs catalog/schema, policy, dispatch, tests and docs. Regenerate both tools.json copies together.
- Keep Odin independent: do not add a Serena bridge or automatic upstream fallback. Never label regex as semantic symbol analysis.
- Do not enable shell, debug or write permissions without an explicit host configuration decision.
- Do not introduce arbitrary eval into native flow. Do not parse repository-supplied files as executable host configuration.
- Worktree / branch per change; plan, implement, tests, review and evidence before release. No invented agent-team participation.
- No cloud CI execution by default; manual self-hosted only. Do not create or push a remote repository merely to run checks.
- Initial source delivery has no Cargo.lock because Cargo was unavailable. Generate and review a real lockfile on first build; thereafter use --locked.
- Run cargo fmt, cargo test --workspace --locked, cargo clippy --workspace --all-targets --locked -- -D warnings, and binary smoke tests on the target platform.
- Backend mocks are protocol fixtures, not real language support / debugging / full-feature parity evidence.
- Document security boundaries and incomplete features in the same change as code.
