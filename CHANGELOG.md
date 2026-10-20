# Changelog

## 0.2.0 — Standalone source preview

Breaking configuration/tool change: removes the external Serena bridge rather than just disabling it. Deletes BridgeConfig, SerenaBridge, upstream process management, serena_tools / serena_call, bridge examples and upstream mocks. Removes the two bridge tools from both generated catalogs; 58 Odin tools remain.

Status reports standalone=true and external_tool_bridge=false. Unknown legacy project configuration is rejected. CLI doctor reports only direct language-server / debugger profiles. Regression tests cover absence of bridge contracts, unknown old tool calls and rejected old configuration. Catalog/docs generation no longer reintroduces the dependency.

Documentation now separates independence from feature completeness: semantic Move/Inline, JetBrains inspections, arbitrary Python REPL and complete per-language compatibility remain missing. No hidden fallback was added. This version does not change native state formats intentionally; migration/runtime behavior has not been exercised.

Rust compilation, unit/integration tests and real backend/platform execution remain NOT_RUN. Static artifact checks and Python mock-peer selftests are recorded separately.

## 0.1.0 — Historical baseline

Initial source preview included the Rust core and an optional external Serena bridge. That bridge is absent from 0.2.0. Original delivery is retained separately; this package does not overwrite it.
