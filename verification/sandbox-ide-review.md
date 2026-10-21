# Sandbox and IDE independent review — revision 4

Result: **PASS for the independent source/contract review at `3d4a9074e8bd693ec5c896bb838ae61ee65e3a0b`**. No blocking review finding remains open. The source-bound 17-command Windows gate and 30-check native analyzer evidence were checked for consistency and pass. Executed six-language/two-project and TypeScript native-sandbox LSP acceptance are both **PARTIAL**, with versionless diagnostics still unverified.

- Task: `sandbox_ide_review`; reviewer: `/root/independent_review`.
- Base: `9b71abc98b89f72cce08d2795dfb139281f6d613`.
- Initially reviewed head: `155f979c693241087ed4fe6778da19a30814979c`.
- Latest reviewed core fix: `2b86310e13a731fdf7b86f48c593e0aa0e71d7c4`.
- Latest integrated head reviewed, including the engine slice: `5e4cf035222cff82b1bc95d74c54ceb894195796`.
- Revision 3 head: `fe5996dc5793b64cb68a5f2d1139fb696cff32cf`.
- Tested binary source head: `c5084f4947fc4a07c9ab381fa9998ba1a1e2246d`; last runtime repair: `6a7c319ed0a9ed874a57bf79e38a8903402ed1d4`.
- Latest integrated review head: `3d4a9074e8bd693ec5c896bb838ae61ee65e3a0b`; final harness author commit: `cd3618f196189106eb30b9bc1a935abaf5664c75`. No crates, manifests, lockfile or catalog changed between this head and the tested binary source head.
- Reviewer did not author or modify product code and did not execute product tests or remote operations. Only the two assigned review artifacts were updated. Runtime execution belongs to the named author/root owners; the reviewer inspected source, artifacts and hashes.

History: the initial review at `155f979` found SIR-001 and SIR-002. Revision 2 at `5e4cf03` remained FAIL with SIR-001 and SIR-003 open, after the source fix for SIR-002. Revision 3 at `fe5996d` passed source review, with final runtime and the new language matrix pending. Actual backend acceptance subsequently discovered the URI and JDTLS compatibility defects recorded as SIR-004 and SIR-005 below; this reviewer validated their repairs independently. Historical defects, failures and snapshot hashes are retained.

## SIR-001 — P1 — hierarchy responses accumulate without a query budget

Status: **RESOLVED**, fixed by `70ae12046eaceb5059aece95cf5724ef40e914a3`, re-reviewed at `fe5996d`. Original source: `crates/odin-protocol/src/lsp.rs`, `hierarchy_response`, lines 746–804 at the initial head.

The prepare response is bounded by the RPC frame limit, but every prepare item starts another RPC. Every complete related response is retained in `output`. There is no prepare-item cap, cumulative response-byte budget, RPC-count cap, or whole-query deadline. The final response also clones `result` into `hierarchy` before the engine applies its output limit.

Concrete trigger: a protocol peer returns 1,000 valid, identical project-local prepare items, each with valid ranges. Each subsequent hierarchy response returns one valid project-local item with about 1 MiB of otherwise inert data. Each individual frame is below 8 MiB and source hashes remain unchanged. The query can retain about 1 GiB before the engine rejects its final size; delayed responses can additionally consume the per-RPC timeout once per item.

Impact: memory exhaustion or a long-running query holding the project execution gate. No destructive/OOM reproduction was run by the reviewer.

Minimal repair: check a small prepare-item/RPC-count cap before issuing related requests, enforce a cumulative serialized-byte budget before retaining/cloning related results, include the duplicated compatibility field in that budget, and bound the complete query deadline. Verify with a small-budget fixture that fails early with `LIMIT_EXCEEDED`, without allocating an unsafe result.

Repair: prepare items are capped at 128, related items at 4096 total and call ranges at 8192 total. Exact encoded JSON is measured with a bounded writer before response retention; the retained result is capped at 4 MiB, including reserved prepare items and wrapper fields. The compatibility alias therefore duplicates at most 8 MiB across the two result fields. One startup-configured deadline covers the complete query, with checks around bounded synchronous work and serialization and an outer timeout for awaited work. Blocking OS file reads cannot be preempted; expiry is checked after they return.

Evidence: `verification/protocol-ide-hierarchy-verification.json` and its raw logs record six quota/deadline regressions, 31 unit tests, 26 fixture tests, one ignored test, strict Clippy and fmt with exit code 0. All nine normalized source hashes matched the revision 3 candidate at `fe5996d`. The final integrated gate below binds the current source. Fixtures demonstrate resource and protocol behavior, not real-language hierarchy acceptance.

Next owner: `/root` for final evidence integration and the remaining language-matrix acceptance.

## SIR-002 — P2 — bounded command cancellation released sandbox resources before reaping

Status: **RESOLVED** by `2b86310`, retained unchanged in the re-reviewed candidate.

Initial source: `crates/odin-core/src/execution.rs`, spawn/attach and `ExecutionLifetime::drop`. The lifetime object owned the process tree and sandbox guard but not `Child`. Cancellation while awaiting the child dropped policy/scratch before child reaping. Attachment failure returned before constructing that lifetime owner and likewise lacked guarded reaping.

The repair creates the lifetime owner immediately after spawn, before attachment, and owns `Child` there. Drop closes the process tree, requests termination, checks exit, and otherwise transfers child plus guard to a runtime-independent reaper thread. The guard is retained until reaping completes. The source repair covers cancellation, attachment failure, and error exits.

The reviewer inspected the committed repair and regression source. Final source-bound evidence in `verification/sandbox-ide-release-logs/native_sandbox.txt` records exit code 0 and three passing native opt-in tests. It includes `abort=true` and `abort=false` markers confirming parent/descendant reaping and scratch/policy removal. This is execution evidence from root, not a test executed by the reviewer.

## SIR-003 — P2 — symbol diagnostics compare textual severity to integers

Status: **RESOLVED**, fixed by `6fc1bd54c84f074f5e324b8670522d1d74471b73`, re-reviewed at `fe5996d`. Original source: `crates/odin-engine/src/lib.rs`, `get_diagnostics_for_symbol` dispatch arm, and `crates/odin-protocol/src/lsp.rs`, `diagnostic_output`.

The protocol finding has a textual `severity` such as `error` or `warning`, with the numeric value stored in `lsp_severity`. The new symbol-level summary counts `finding["severity"] == 1..=4`, so all four counts are zero even when the selected symbol has a current error. The filtered response also retains its previous document-level status and unknown count.

Concrete trigger: a current diagnostic within the selected symbol has LSP severity 1. The tool retains one finding with `severity: "error"`, returns `summary.total: 1`, but reports `summary.errors: 0`. Diagnostics outside the symbol can likewise leave the document status attached to the wrong scope.

Minimal repair: recount from `lsp_severity` or the documented string values, including unknown severity, and derive status/summary scope from the filtered findings while preserving pending/stale/unverified states. Add a fixture with an error inside one symbol and a diagnostic outside it, plus an empty selected scope.

Repair: the filtered summary counts numeric `lsp_severity`, derives the unknown count, recomputes errors/warnings/findings/clean only for current findings, preserves non-current status and labels the summary scope as symbol. The targeted fixture checks an error in one symbol, a warning and unknown severity in another, and an empty scope.

Evidence: `verification/engine-ide-symbol-diagnostics.json` records the targeted regression, fmt and strict Clippy with exit code 0. All four source hashes and all three raw log hashes matched at revision 3. The current integrated workspace evidence was also checked. The reviewer did not rerun the product test.

Next owner: `/root` for final evidence integration and the remaining language-matrix acceptance.

## SIR-004 — P2 — equivalent Windows diagnostic URIs miss the push cache

Status: **RESOLVED**, fixed by `8f515447486e474fe1a867b9fb85c972078118a0`, reviewed at `87c11a9` and retained in `c5084f4`. Source: `crates/odin-protocol/src/rpc.rs`, diagnostic insertion at line 306, lookup at line 519, invalidation at line 528 and `diagnostic_uri_key` at line 539. This defect was discovered by actual backend acceptance; the reviewer did not author the fix.

Concrete trigger: didOpen uses `file:///C:/...`, while installed basedpyright and TypeScript publish `file:///c%3A/...`. Literal cache keys cause lookup to return none even though the backend published error diagnostics. `verification/protocol-ide-uri-real-before.txt` records `cache_lookup:false` for both real backends. These observational probes exited 0; their false lookup values are the defect, not passing cache assertions. The impact is missing/pending diagnostics, not evidence of a clean file.

Repair: insertion, lookup and invalidation share a validated lexical file-URI identity. Percent escapes are decoded without filesystem I/O; only the Windows drive-letter case is folded. Path-component case and the original report payload remain intact. Authority/UNC, query/fragment, malformed escapes, encoded separators/NUL and non-UTF-8 paths are protocol errors. Cache identity provides no path authorization. The unchanged freshness logic still labels missing/null versions `unverified`; URI equivalence does not supply a version.

Evidence: `verification/protocol-ide-uri-java-final-uris.txt` records `cache_lookup:true` for real basedpyright and TypeScript notifications, retaining basedpyright version 1 and TypeScript's missing version. Fixtures cover insertion/lookup/invalidation in both alias directions, current error/clean recovery, stale and unversioned states and malformed URI rejection. The latest package log records 35 unit and 31 fixture tests passing, with three ignored, plus strict Clippy and fmt exit 0. Direct backend probes are narrower than full integrated language acceptance.

## SIR-005 — P2 — JDTLS empty WorkspaceEdit alternative rejects cross-file rename

Status: **RESOLVED**, fixed by `6a7c319ed0a9ed874a57bf79e38a8903402ed1d4`, reviewed at `87c11a9` and retained in `c5084f4`. Source: `crates/odin-protocol/src/workspace.rs`, `Planner::plan`, lines 262–310. This defect was discovered by the actual JDTLS matrix; the reviewer did not author the fix.

Concrete trigger: JDTLS/LSP4J returns `changes:{}` alongside two populated `documentChanges` text edits with null versions. The former presence-only check rejected the proposal before CAS planning. `verification/protocol-ide-uri-java-real-before.txt` preserves the actual response and `planned:false`. Its exit 0 belongs to an observational capture, not a passing planner assertion.

Repair: only absent, null or correctly typed empty alternatives are tolerated. Two populated forms, malformed types and an entirely empty proposal remain errors. The existing ordered resource-edit handling, document-version validation, no-overwrite policy, source snapshots and expected hashes remain in force. Planning remains read-only and project writes pass through core Transactions.

Evidence: `verification/protocol-ide-uri-java-source-bound-real.txt` records actual JDTLS cross-file references and rename, `planned:true`, two expected-hash changes, Transactions apply and hash-guarded rollback with exit 0. The raw probe setup matches the recorded fixture hashes, settings and initialization in `verification/protocol-ide-uri-java-verification.json`. At `87c11a9`, all 12 source hashes, two test binary hashes and five external input hashes matched. The earlier URI-only report is historical: four of its source hashes predate the Java repair. The later test-only readiness change is bound by the final gate below.

## Fixture startup regression and final execution evidence

`verification/sandbox-ide-final-validation.json` preserves the first `87c11a9` gate as **FAIL**, with workspace test exit 101. The raw log records `request_timeout_cancels_once_and_leaves_no_pending_registration` timing out again on `fixture/state` while Python startup competed with other fixtures. Later gates in that failed run remain NOT_RUN.

The tests-only repair `c5084f4947fc4a07c9ab381fa9998ba1a1e2246d` adds the existing `--started-marker` option and a separate, at-most-three-second readiness wait. The deliberately unanswered RPC still uses a 100 ms timeout; the expected timeout error, exactly-one cancellation, zero pending requests and termination assertions are retained. No runtime source changed. The reviewed LF hash of `fixture_protocols.rs` is `da2bbda3e1f0203f68014626f78992a00c654ed513cb1529ba5fbfe364158b3b`.

`verification/sandbox-ide-release-validation.json` records **PASS, 17/17 commands, all exit 0**, for source `c5084f4`. The reviewer checked all 102 normalized source hashes and all 17 raw command-log hashes without mismatch. The raw workspace log sums to 200 passing and six ignored tests; the native opt-in log has three passing tests. The gate includes fmt, strict workspace Clippy, locked release build, CLI/MCP/HTTP/cancellation smoke, 29 real Rust/Python/TypeScript and debugpy checks, dependency audit, licenses, supply chain and source/requirements/publish-policy checks. The real-backend report preserves an initial symbol-index readiness attempt error alongside its eventual scoped PASS.

The actual release binary hash matches `f7e57a5a25229426b62be622770ac2d6b8f7cfd3fcbf47dbf25254aa739eb875`. Recorded build metadata identifies `c5084f4`, release profile, `x86_64-pc-windows-msvc`, standalone behavior and `tracked_source_dirty:false` at build time. The validation records no source changes during execution. Later review/evidence edits do not change this tested binary's identity.

`verification/native-inspections-release-final.json` and `verification/native-inspections-release-final-command.json` record **30/30 real Odin MCP/MXC analyzer checks, exit 0**, on that same binary and source revision. The reviewer checked report/log hashes, harness hash and the pinned executor hash `8a6d1db4dd1846981127d9014741be18b094d07be967765dff6754bda1d40259`. Recorded results cover sandboxed Clippy, Ruff and ESLint findings plus transaction previews. This analyzer evidence is distinct from the TypeScript LSP sandbox execution below.

## Additional language-driver source review

The tests/docs repair in author commit `5fc4c99` passes source review. Its harness SHA-256 is `d47fa6daa7c96aa8d5bc7b7c2603755b4047c4e3ed0c3d740b358f9e931a6e09`; the previous harness bound by the 17-command gate is `862bf89645e99c5110bd6d700ba8a8a16c4f796c86218dd4c20dad3d32a416bf`. The diff moves `diagnostics_valid` before rename/format mutate the fixture and adds two explanatory comments. AST comparison confirms that `diagnostics()` and `aggregate()` are unchanged. Broken-source and recovery checks still use Transactions and exact-byte rollback. Only version-verified, complete diagnostics can pass; observed unversioned diagnostics stay NOT_VERIFIED and a pending timeout remains a failure.

The installed TypeScript language server source at `cli.mjs` lines 22009–22024 suppresses consecutive empty diagnostic updates and publishes a payload without a document version. The ordering repair measures the initial valid state independently of later CAS mutations. It does not establish current clean diagnostics after every transaction or permit trusting an invalidated cached report.

The preserved `language-ide-final-baseline-02` attempt in author commit `c4b71d6` remains **FAIL, exit 1**, on the final `f7e57a5a...` binary and old harness. Its counts are 260 PASS, 14 UNAVAILABLE, two FAIL and ten NOT_VERIFIED: eight projects pass, Java's two are partial, and TypeScript's two initial-valid checks fail after rename/format. The subsequent `language-ide-typescript-order-03` probe in `5fc4c99` is **PARTIAL, exit 2**, with 40 PASS, two UNAVAILABLE, six NOT_VERIFIED and zero FAIL. Its two projects observe initial/error/recovery diagnostics, all unversioned. The reviewer checked the report/log/transcript/command hashes for both attempts without mismatch; normalization preserves their original status.

The newer harness has separate evidence provenance from the 17-command source snapshot. Final full-language and TypeScript sandbox results are recorded below. No runtime source or release binary changed for these harness repairs, and the reviewer executed neither probe.

## Final language matrix and actual TypeScript native sandbox

`verification/language-ide-final-baseline-03.json` records **PARTIAL, actual exit 2**, with 260 PASS, 14 UNAVAILABLE, 12 NOT_VERIFIED and zero FAIL on the same `f7e57a5a...` binary. All 12 real projects initialized. Both projects for Rust, Python, Go and C# pass this fixture's available assertions. Both TypeScript and Java projects are partial: initial/error/recovery diagnostic reports have no document version and remain unverified. These are executed results, not a claim of complete language or IDE parity.

`verification/language-ide-final-sandbox-typescript-04.json` records **PARTIAL, actual exit 2**, with 42 PASS, two UNAVAILABLE, six NOT_VERIFIED and zero FAIL. Both TypeScript projects pass actual LSP initialization and pinned MXC routing, symbols, cross-file references, completion/resolve, hover, signature help, workspace symbols, inlay hints, semantic tokens, call hierarchy, rename CAS apply/rollback and formatting CAS apply/rollback. Inlay-hint resolve is unavailable; all six versionless diagnostic checks remain NOT_VERIFIED.

The final sandbox harness in author commit `cd3618f` has SHA-256 `1160258157d2790782e9060be0556af874397daf741196ef32297435c6c0dbf6`. It requires two explicit TypeScript-only sandbox options: a private-temp bootstrap and fixed descendant path flags. The generated support file resides outside project roots, creates the private temp directory, checks the fixed installed TSLS module SHA and imports that module with fixed stdio arguments. The descendant option overwrites `NODE_OPTIONS` with exactly `--preserve-symlinks --preserve-symlinks-main`; it never appends inherited option text. It adds one fixed readonly bootstrap file, with no drive-root grant, arbitrary module/eval, fallback, writable project/runtime grant or Odin environment-allowlist change. Existing private scratch, deny-all networking, clipboard/injection settings and explicitly selected UI profile are retained.

Both recorded bootstrap sources hash to `95f82f490f6eb2487b25335edcc8321517ce5a0be118cf04579ed709a0131ce6`; the installed module hashes to `192638f1fa62c907fd54c0b55b12dd3dff01325a1a0ed5973907efca1c57b5eb`. The reviewer checked those bytes and all ten final baseline/sandbox report, command, transcript, stdout and normalization-receipt hashes. `verification/language-ide-final-summary.json` hashes to `aba9616b7e905cc9209556021e51c89bf2103f40e2bed3217ae3cc68b3a5da0b` and retains both harness identities and actual exit 2 results. AST comparison against `5fc4c99` confirms unchanged `project`, `diagnostics`, `rename`, `format`, `envelope`, `optional`, `prepare_edit`, `call` and `record` acceptance methods. The separate fail-fast repair `4b2936c` only stops retries after a permanently exited backend and preserves failure classification.

Sandbox attempts 01, 02 and 03 remain **FAIL, actual exit 1**, with all four artifact hashes per attempt checked. Attempt 01 records Node's ENOENT while resolving the uncreated private Package/AC/Temp. Attempts 02/03 initialize the LSP/native route but then record server/child exit; their captured output does not establish a specific child filesystem error. The installed TSLS source rebuilds child `execArgv` and copies its process environment, providing the compatibility hypothesis tested by the final fixed descendant options. Historical failures were not reclassified.

Actual sandbox transport and OS attack probes retain separate evidence scopes. The TypeScript route still reports `os_boundary_verified:false`. The final native workload log separately records true filesystem/network-denial, readonly-write denial, junction/outside-execution and filtered-environment checks for parent and descendant workloads on the same pinned executor. These workloads do not become per-LSP attack probes or full UI-isolation certification. Actual sandboxed LSP workloads for Rust, Python, Go, Java and C# remain NOT_RUN; native analyzer results do not substitute for those runs.

## Other inspected boundaries and limitations

The reviewed native sandbox path checks the host executor SHA pin again at preparation, rejects unsupported probe states, uses explicit filesystem grants and fixed deny networking, denies policy storage, and provides no unsandboxed retry. Host permission reductions preserve separate write/shell/debug/inspection decisions. Formatting remains inert WorkspaceEdit data; document queries verify source hash and synchronized version. Language registry output is labelled configuration guidance rather than backend acceptance.

The integrated catalogs contain 85 unique tools and the two parsed JSON catalogs are identical. The engine routes IDE reads directly to LSP, formatting through WorkspaceEdit/CAS Transactions, and native fixes through re-analysis, stable finding identity, source hash, safe single-file edit validation and Transactions. Inspections require explicit permission and an actual configured sandbox; source/state overlap is rejected for writable and temporary grants across all configured projects. Ruff requires `--no-fix` and `--no-cache`; ESLint disables repository config lookup and restricts explicit executable config to host paths. Single and batch DAP launch calls use the immutable host payload helper.

`verification/engine-ide-verification.json` preserves earlier passing package evidence and a later AppControl-blocked run; those outcomes are not interchangeable. The targeted SIR-003 report is independently recorded. `verification/native-inspections-attempt-3.json` records 30 real MCP/MXC analyzer checks passing on the earlier release revision `30a957a95bd39d1dbbe631d2649593dbf54e6250`, binary SHA-256 `cb0097266da19e2be25136a56ce1b60e8f02dc8f7eb8f33b282aae045251e014`; it remains historical evidence. The final-binary analyzer evidence is separately identified above.

At revision 3, `verification/sandbox-ide-local-validation.json` named source head `fe5996d`, had status NOT_RUN and no completion time. That historical state has not been relabeled as a test pass; the current executed gate has its own report above. Final language and TypeScript native-sandbox acceptance remain PARTIAL for the stated versionless diagnostics, with optional capabilities UNAVAILABLE and other five languages' actual sandbox LSP workloads NOT_RUN. Current clean diagnostics after every transaction, whole-project diagnostics, full IDE-equivalence, Linux/macOS and complete OS certification remain unproven. The reviewer executed no product tests, and historical AppControl, fixture and sandbox failures remain preserved.

Read-only Git/source/catalog and source/log hash inspection commands completed with exit code 0. Current paired catalogs contain 85 identical unique contracts. The review JSON retains all five resolved defects, fix commits, current and historical source hashes, execution-evidence paths, historical FAIL decisions and the current source-review PASS. Next owner: root to freeze these two artifact hashes, integrate delivery/manifest evidence and complete final publication checks while preserving the PARTIAL/NOT_VERIFIED/UNAVAILABLE/NOT_RUN boundaries.
