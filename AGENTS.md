# Odin engineering rules

- Read README, docs/ARCHITECTURE.md, docs/KNOWN_LIMITATIONS.md and verification/delivery-status.json first.
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
