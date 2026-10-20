# Native completion dispatch

Date: 2026-10-21 (Asia/Taipei). Baseline: `2ed95d6`.

The user authorized implementation, local verification, and publishing the result
to a public repository under `stevenke1981`. No cloud CI run is authorized or
required. Serena is a development navigation tool, never an Odin backend.

## Owners and dependencies

| Owner | Scope | Acceptance |
|---|---|---|
| root | core transactions, paths, memories, index, process lifecycle, integration, release evidence | CAS conflicts never overwrite; export/import integrity; Windows native gates; honest requirement statuses |
| requirements_audit | read-only requirements discovery, then owned requirement/documentation evidence updates | distinguish actual implementation from platform/backend evidence |
| independent_review | independent read-only review, no authored product changes | review the integrated runtime, report concrete findings and verify fixes |
| protocol_implementation | odin-protocol, protocol fixtures and implementation notes | resource WorkspaceEdits normalize into CAS changes; backend lifecycle and bounded RPC |
| engine_implementation | odin-engine, catalog generators and both tool catalogs, API docs | tool schema/policy/dispatch/tests agree; semantic refactors use backend edits; memory previews cannot cross scope |
| platform_implementation | CLI/HTTP/dashboard, setup/packaging and acceptance scripts | native binary operation, client setup without implicit writes, portable release and installer recipe |

Each author has a separate worktree and branch. Shared Cargo manifests and the
lockfile have one owner (root). Integration is serialized by cherry-pick.

## Verification order

1. Preserve baseline, generate/review the real lockfile and run baseline tests.
2. Implement the owned slices with regression tests and boundary documentation.
3. Integrate, format, run workspace tests and Clippy with `--locked`.
4. Run real binary MCP stdio/HTTP smoke tests and available real LSP/DAP probes.
5. Obtain an independent review of the integrated candidate and fix findings.
6. Refresh requirement evidence, source hashes, public-file/secret checks and
   release artifacts; publish only the reviewed candidate.

The 40 requirements include clean-machine installer tests, Linux/macOS native
verification and multi-language/performance matrices. They remain NOT_RUN until
their actual commands run; source implementation is not acceptance of those
requirements. Arbitrary evaluation, upstream fallback and implicit permission
grants remain prohibited.
