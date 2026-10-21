# Sandbox and IDE independent review — revision 3

Result: **PASS for the independent source/contract review at `fe5996dc5793b64cb68a5f2d1139fb696cff32cf`**. No blocking review finding remains open. Final integrated runtime acceptance and the new language matrix are separate gates.

- Task: `sandbox_ide_review`; reviewer: `/root/independent_review`.
- Base: `9b71abc98b89f72cce08d2795dfb139281f6d613`.
- Initially reviewed head: `155f979c693241087ed4fe6778da19a30814979c`.
- Latest reviewed core fix: `2b86310e13a731fdf7b86f48c593e0aa0e71d7c4`.
- Latest integrated head reviewed, including the engine slice: `5e4cf035222cff82b1bc95d74c54ceb894195796`.
- Re-reviewed integrated head: `fe5996dc5793b64cb68a5f2d1139fb696cff32cf`; runtime files have no working-tree differences against this head.
- Reviewer did not author or modify product code and did not execute product tests or remote operations. Only these two new review artifacts were written.

History: the initial review at `155f979` found SIR-001 and SIR-002. Revision 2 at `5e4cf03` remained FAIL with SIR-001 and SIR-003 open, after the source fix for SIR-002. The descriptions below preserve those historical defects; the resolved status and repair evidence describe the latest candidate.

## SIR-001 — P1 — hierarchy responses accumulate without a query budget

Status: **RESOLVED**, fixed by `70ae12046eaceb5059aece95cf5724ef40e914a3`, re-reviewed at `fe5996d`. Original source: `crates/odin-protocol/src/lsp.rs`, `hierarchy_response`, lines 746–804 at the initial head.

The prepare response is bounded by the RPC frame limit, but every prepare item starts another RPC. Every complete related response is retained in `output`. There is no prepare-item cap, cumulative response-byte budget, RPC-count cap, or whole-query deadline. The final response also clones `result` into `hierarchy` before the engine applies its output limit.

Concrete trigger: a protocol peer returns 1,000 valid, identical project-local prepare items, each with valid ranges. Each subsequent hierarchy response returns one valid project-local item with about 1 MiB of otherwise inert data. Each individual frame is below 8 MiB and source hashes remain unchanged. The query can retain about 1 GiB before the engine rejects its final size; delayed responses can additionally consume the per-RPC timeout once per item.

Impact: memory exhaustion or a long-running query holding the project execution gate. No destructive/OOM reproduction was run by the reviewer.

Minimal repair: check a small prepare-item/RPC-count cap before issuing related requests, enforce a cumulative serialized-byte budget before retaining/cloning related results, include the duplicated compatibility field in that budget, and bound the complete query deadline. Verify with a small-budget fixture that fails early with `LIMIT_EXCEEDED`, without allocating an unsafe result.

Repair: prepare items are capped at 128, related items at 4096 total and call ranges at 8192 total. Exact encoded JSON is measured with a bounded writer before response retention; the retained result is capped at 4 MiB, including reserved prepare items and wrapper fields. The compatibility alias therefore duplicates at most 8 MiB across the two result fields. One startup-configured deadline covers the complete query, with checks around bounded synchronous work and serialization and an outer timeout for awaited work. Blocking OS file reads cannot be preempted; expiry is checked after they return.

Evidence: `verification/protocol-ide-hierarchy-verification.json` and its raw logs record six quota/deadline regressions, 31 unit tests, 26 fixture tests, one ignored test, strict Clippy and fmt with exit code 0. All nine normalized source hashes in that report match this candidate. Fixtures demonstrate resource and protocol behavior, not real-language hierarchy acceptance.

Next owner: `/root` for the final integrated runtime gate.

## SIR-002 — P2 — bounded command cancellation released sandbox resources before reaping

Status: **RESOLVED IN SOURCE** by `2b86310`, retained unchanged in the re-reviewed candidate.

Initial source: `crates/odin-core/src/execution.rs`, spawn/attach and `ExecutionLifetime::drop`. The lifetime object owned the process tree and sandbox guard but not `Child`. Cancellation while awaiting the child dropped policy/scratch before child reaping. Attachment failure returned before constructing that lifetime owner and likewise lacked guarded reaping.

The repair creates the lifetime owner immediately after spawn, before attachment, and owns `Child` there. Drop closes the process tree, requests termination, checks exit, and otherwise transfers child plus guard to a runtime-independent reaper thread. The guard is retained until reaping completes. The source repair covers cancellation, attachment failure, and error exits.

The root reports an actual native abort/timeout regression passing for both parent/descendant disappearance and scratch/policy removal. The reviewer inspected the committed repair and regression source, but did not independently execute that test. Full integrated validation is still a separate gate.

## SIR-003 — P2 — symbol diagnostics compare textual severity to integers

Status: **RESOLVED**, fixed by `6fc1bd54c84f074f5e324b8670522d1d74471b73`, re-reviewed at `fe5996d`. Original source: `crates/odin-engine/src/lib.rs`, `get_diagnostics_for_symbol` dispatch arm, and `crates/odin-protocol/src/lsp.rs`, `diagnostic_output`.

The protocol finding has a textual `severity` such as `error` or `warning`, with the numeric value stored in `lsp_severity`. The new symbol-level summary counts `finding["severity"] == 1..=4`, so all four counts are zero even when the selected symbol has a current error. The filtered response also retains its previous document-level status and unknown count.

Concrete trigger: a current diagnostic within the selected symbol has LSP severity 1. The tool retains one finding with `severity: "error"`, returns `summary.total: 1`, but reports `summary.errors: 0`. Diagnostics outside the symbol can likewise leave the document status attached to the wrong scope.

Minimal repair: recount from `lsp_severity` or the documented string values, including unknown severity, and derive status/summary scope from the filtered findings while preserving pending/stale/unverified states. Add a fixture with an error inside one symbol and a diagnostic outside it, plus an empty selected scope.

Repair: the filtered summary counts numeric `lsp_severity`, derives the unknown count, recomputes errors/warnings/findings/clean only for current findings, preserves non-current status and labels the summary scope as symbol. The targeted fixture checks an error in one symbol, a warning and unknown severity in another, and an empty scope.

Evidence: `verification/engine-ide-symbol-diagnostics.json` records the targeted regression, fmt and strict Clippy with exit code 0. All four source hashes and all three raw log hashes in that report match. The reviewer did not rerun the product test.

Next owner: `/root` for the final integrated runtime gate.

## Other inspected boundaries and limitations

The reviewed native sandbox path checks the host executor SHA pin again at preparation, rejects unsupported probe states, uses explicit filesystem grants and fixed deny networking, denies policy storage, and provides no unsandboxed retry. Host permission reductions preserve separate write/shell/debug/inspection decisions. Formatting remains inert WorkspaceEdit data; document queries verify source hash and synchronized version. Language registry output is labelled configuration guidance rather than backend acceptance.

The integrated catalogs contain 85 unique tools and the two parsed JSON catalogs are identical. The engine routes IDE reads directly to LSP, formatting through WorkspaceEdit/CAS Transactions, and native fixes through re-analysis, stable finding identity, source hash, safe single-file edit validation and Transactions. Inspections require explicit permission and an actual configured sandbox; source/state overlap is rejected for writable and temporary grants across all configured projects. Ruff requires `--no-fix` and `--no-cache`; ESLint disables repository config lookup and restricts explicit executable config to host paths. Single and batch DAP launch calls use the immutable host payload helper.

`verification/engine-ide-verification.json` preserves earlier passing package evidence and a later AppControl-blocked run; those outcomes are not interchangeable. The targeted SIR-003 report is independently recorded. `verification/native-inspections-attempt-3.json` records 30 real MCP/MXC analyzer checks passing on the earlier release revision `30a957a95bd39d1dbbe631d2649593dbf54e6250`, binary SHA-256 `cb0097266da19e2be25136a56ce1b60e8f02dc8f7eb8f33b282aae045251e014`; it is not evidence for the final binary at `fe5996d`.

At this review, `verification/sandbox-ide-local-validation.json` named source head `fe5996d`, had status NOT_RUN and no completion time. Root owns completion and attachment of that final integrated runtime evidence. The new six-language matrix remains pending; no whole-language, full IDE-equivalence, Linux/macOS or complete OS certification is inferred. The reviewer executed no product tests, and historical AppControl failures remain preserved.

Read-only Git/source/catalog and source/log hash inspection commands completed with exit code 0. Current paired catalogs contain 85 identical unique contracts. The review JSON retains the original findings, fix commits, source hashes, evidence paths, historical FAIL decision and the current source-review PASS. Root may attach the final runtime results without relabeling a package fixture as full language support.
