# Changelog

## 0.2.0 — Windows native preview, 2026-10-21

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
