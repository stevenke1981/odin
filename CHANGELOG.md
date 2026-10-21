# Changelog

## Unreleased — optimization and multi-client auto-load, 2026-10-21

Index schema 2 links FTS5 rows to files by rowid, versions the store with
`user_version`, refuses newer schemas untouched and migrates under an immediate
lock while keeping audit rows. Engine dispatch is split into domain modules with
no behaviour change; caller-supplied numbers saturate instead of truncating.
The lexical structure checker no longer misreads `macro_rules!` repetitions.

`odin setup` emits entries for Codex, Claude Code, OpenCode, Antigravity (agy),
Gemini CLI, Cursor and VS Code, plus suggested user-scope locations and the
native registration command (never executed). `serve --project-from-cwd` and
`--default-project` advertise one immutable session default project in MCP
instructions; tool schemas still require `project`. `odin project add` appends
a project through the CAS dry-run/apply path and reports the permission change.

Independent review fixes: honest `project add` output, host config side files
denied to agents, project roots inside the state directory rejected, no
project-scope client config suggestions, no `\\?\` prefix in client entries.
Real clients: Claude Code and Codex called `status` (PASS); OpenCode and agy
loaded the server but the tool call is NOT_RUN; Gemini, Cursor and VS Code are
NOT_RUN. No cloud CI was dispatched.

## 0.2.0 — Sandbox and IDE extension, 2026-10-21

Expanded to 85 native tool contracts with completion/resolve, hover, signature
help, workspace/call hierarchy queries, formatting transactions, semantic tokens
and inlay hints. Added native Clippy/Ruff/ESLint findings and safe-fix previews.
Inspections require an explicit sandbox and read-only project grants.

Added optional hash-pinned Windows MXC ProcessContainer execution with file
grants, network denial, private scratch and fail-closed launch. LSP, DAP and
fixed commands share that boundary when configured. Added immutable host-owned
debug launch payloads and 28 declarative language setup profiles. The registry
does not assert installation or language acceptance.

Independent review addressed hierarchy query budgets, cancellation cleanup and
scoped diagnostic summaries. Real backends exposed Windows percent-encoded file
URI cache mismatches and JDTLS empty WorkspaceEdit alternatives; both now retain
freshness, conflict checks and transaction rollback. Source-bound Windows gates and real backend
reports distinguish execution, unavailable capabilities and remaining gaps.
Linux/macOS sandboxing, every-language IDE parity and all 40 original acceptance
criteria remain separate from this Windows preview. No cloud CI was dispatched.

## 0.2.0 — Windows native preview baseline, 2026-10-21 (historical)

Expanded to 72 native tool contracts. Added bounded symbol filters/paging,
occurrence selection, backend edit-only refactors, ordered resource WorkspaceEdits,
scoped memory import/export/move, incremental FTS updates, bounded per-session
dataflow, DAP sessions/tickets/batches, context/mode restrictions, client setup,
metadata/doctor, trace digests and expiring result chunks.

Hardened canonical Windows paths, readonly scopes, transaction aliases and
budgets, subprocess environment/lifetime, protocol cancellation and source
snapshot conflicts. Independent review findings were fixed and re-reviewed.
Real lockfile, local Windows Rust gates, actual MCP CLI/stdio/HTTP checks and
Rust/Python/TypeScript LSP plus debugpy evidence now replace the initial unbuilt
status. Final executed commands and artifact hashes live in verification reports.

Portable/Inno packaging is provided. OS sandboxing, full IDE/language parity,
Linux/macOS native execution, clean-machine upgrades and performance comparison
remain incomplete or unverified. No automatic cloud CI was added.

## 0.2.0 — Initial standalone source delivery (historical)

Breaking configuration/tool change: removes the external Serena bridge rather than just disabling it. Deletes BridgeConfig, SerenaBridge, upstream process management, serena_tools / serena_call, bridge examples and upstream mocks. Removes the two bridge tools from both generated catalogs; 58 Odin tools remain.

Status reports standalone=true and external_tool_bridge=false. Unknown legacy project configuration is rejected. CLI doctor reports only direct language-server / debugger profiles. Regression tests cover absence of bridge contracts, unknown old tool calls and rejected old configuration. Catalog/docs generation no longer reintroduces the dependency.

Documentation now separates independence from feature completeness: semantic Move/Inline, JetBrains inspections, arbitrary Python REPL and complete per-language compatibility remain missing. No hidden fallback was added. This version does not change native state formats intentionally; migration/runtime behavior has not been exercised.

Rust compilation, unit/integration tests and real backend/platform execution remain NOT_RUN. Static artifact checks and Python mock-peer selftests are recorded separately.

## 0.1.0 — Historical baseline

Initial source preview included the Rust core and an optional external Serena bridge. That bridge is absent from 0.2.0. Original delivery is retained separately; this package does not overwrite it.
