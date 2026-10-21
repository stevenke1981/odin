# Native IDE tools and inspection providers

Odin handles these tools through its own Rust engine, protocol client and core Transactions. No Serena process, JetBrains private protocol or automatic backend fallback is involved. Backend capability declarations and protocol fixture tests do not prove real language support. Runtime evidence is recorded separately in `verification/engine-ide-verification.json`; real analyzer and language-server acceptance belongs to the integration owner.

## LSP tools

| Tools | Contract |
|---|---|
| `completion`, `completion_resolve` | Completion list/array, bounded returned items and optional metadata resolution. Resolve requires the exact document hash. Commands and suggested text edits remain inert data. |
| `hover`, `signature_help` | Server-provided hover/signature data, validated document coordinates and source freshness. |
| `workspace_symbols` | Explicit configured language, optional file/directory scope, project-local matching-language source snapshot, limit 1–1000 and truncation metadata. Outside-snapshot results are excluded. |
| `call_hierarchy` | Prepare actual call-hierarchy items and query incoming/outgoing relations, with local source/range checks. |
| `format_document` | Backend document/range formatting becomes a durable CAS transaction preview. Requires `allow_write` and `expected_sha256`. The caller must separately apply the transaction. Empty edits return `changed:false`. |
| `inlay_hints`, `inlay_hint_resolve` | Bounded hints with optional range. Resolve requires the source hash. Label commands and text edits never execute. |
| `semantic_tokens` | Server semantic-token data with its actual legend, UTF-16 encoding and source metadata. Full/range capability is negotiated; unsupported modes fail explicitly. |

Read responses contain `provider`, `source:lsp`, `freshness:current`, `position_encoding:utf-16` and document `{path,uri,sha256,version}`. Workspace responses identify their checked project scope. The protocol client reads current sources again after requests, so an external writer during the RPC produces `CONFLICT`. `current` does not prove that a server's project index is complete; `index_complete_proven` remains false. Read tools still initialize explicitly configured language servers and write local audit/cache state.

Odin does not call `workspace/executeCommand`. Returned Markdown, command objects, data fields, completion edits and hint label parts are untrusted display/proposal data. No resolve tool mutates source. Formatting, native inspection fixes and existing edit-only CodeActions all enter the core transaction journal and revalidate hashes at apply time.

## Native inspections

The host must explicitly set `allow_inspections=true` and configure an `inspections` profile and native `sandbox`. `allow_shell` does not grant inspection permission. Contexts/modes may only reduce these grants. `list_inspection_providers`, `get_capabilities` and redacted configuration expose names/providers and permission metadata, never executable paths, arguments or executable configuration contents.

Only `profile` and report limits are accepted from a caller. The host owns executable, complete argv, timeout and version argv. Example analysis argv (the host supplies installed executable paths and external cache/config locations):

| Provider | Analysis argv | Version argv |
|---|---|---|
| Clippy, using `cargo` | `clippy --locked --offline --message-format=json --target-dir <external-cache>` | `clippy --version` |
| Ruff, using `ruff` | `check --no-fix --no-cache --output-format=json .` | `--version` |
| ESLint, using a trusted Node/ESLint entry point | `--no-config-lookup --format=json --config <absolute-host-config> .` | `--version` (include a trusted script entry point when using Node) |

Known direct-write flags/subcommands are rejected, including `--fix`, output-file and cache write switches. Ruff must specify `--no-fix --no-cache` so repository Ruff settings cannot enable fixing. Clippy must specify `--locked` to prevent lockfile updates; build output must use an explicitly writable external cache. ESLint must disable executable config discovery (`--no-config-lookup`, or legacy `--no-eslintrc`); explicit executable config must be an absolute canonical host path outside **every** configured project. Version argv are checked for the same known direct-write flags. Host configuration remains a trust boundary: an arbitrary host-chosen executable is executable code, and Clippy builds can execute build scripts/procedural macros.

Inspections require a configured native sandbox and verified sandboxed execution for both version and analysis invocations. There is no unsandboxed fallback. Canonical, component-based Windows case-aware checks reject writable grants and temp directories that overlap any configured source root or Odin state, in either direction. Toolchains, dependencies and external configuration require host read grants; external caches require separate write grants. Analysis source edits are permitted only through Transactions. The process runner owns timeout, output bounding, descendant cancellation and sandbox enforcement.

### Findings and freshness

Clippy JSON lines, Ruff JSON arrays and ESLint JSON reports are normalized to `finding_id`, `provider`, `source`, `rule_id`, `relative_path`, `range`, LSP numeric `severity`, `message`, analyzer `version`, source `sha256`, `document_version:null`, `freshness`, `position_encoding:utf-16` and `fix_available`. Standalone analyzers have no LSP document version; the hash is the freshness identity.

Clippy uses UTF-8 byte offsets. Ruff uses one-based Unicode scalar columns. ESLint locations and fix offsets use JavaScript UTF-16 units. Conversion respects BOM and CRLF, and rejects ranges splitting surrogate pairs, UTF-8 characters or CRLF boundaries. Relevant upstream format contracts: [rustc JSON](https://doc.rust-lang.org/rustc/json.html), [Ruff JSON renderer](https://github.com/astral-sh/ruff/blob/main/crates/ruff_db/src/diagnostic/render/json.rs), [Ruff line index](https://github.com/astral-sh/ruff/blob/main/crates/ruff_source_file/src/line_index.rs), [ESLint JSON formatter](https://eslint.org/docs/latest/use/formatters/) and [ESLint SourceCode](https://github.com/eslint/eslint/blob/main/lib/languages/js/source-code/source-code.js).

The engine snapshots relevant source files before the version/analysis processes and compares them after execution. Changed sources produce a stale/incomplete report with no usable fixes. Outside-project, ignored or unavailable locations are unlocated findings and make the report incomplete. Unknown/mismatched provider version, malformed output, timeout, truncation, abnormal exits and missing Cargo completion cannot become an empty clean result. `summary.complete` describes only the configured profile's report. It does not certify complete project coverage: `scope:host_fixed_profile` and `whole_project_verified:false` remain explicit.

Snapshots are bounded at 32 MiB plus the host per-file/file-count limits. Raw output, normalized metadata and cumulative copied fix edits are bounded by the host output budget (hard ceiling 16 MiB); count is bounded by both caller and host limits. Replacement lengths and final file size are checked before editing. Oversize/truncated results return `LIMIT_EXCEEDED`, rather than claiming clean analysis.

### Inspection fixes

`prepare_inspection_fix` reruns the same host profile, requires a complete/current report and looks up the caller's exact `finding_id`; the identity includes provider, rule, source hash, coordinates, message and normalized fix edits. It accepts only Ruff safe-applicability edits, ESLint's direct fix (not optional suggestions), and a Clippy `MachineApplicable` suggestion group. Only single-file, non-overlapping, bounded edits are previewed. Cross-file suggestions are excluded. The caller supplies the exact `expected_sha256`, and must separately call `transaction_apply`. Native analyzers are never invoked with a fixing flag.

## Verification boundary

Configured LSP and DAP sessions pass their optional native sandbox policy into the protocol transport, which retains the launch guard through cleanup. A configured boundary failure never silently retries without isolation. Native inspection execution always requires this boundary; language/debug execution uses the explicit host setting. Transport isolation metadata reports the requested route and pinned executor identity; only actual OS denial tests establish an enforced OS boundary.

Debug sessions retain the host-selected debugger profile. Optional `debug_launches[profile]` supplies immutable launch/attach/restart JSON payloads: empty caller arguments select the configured payload, nonempty arguments must exactly match it, and an absent attach/restart payload is denied. Both individual requests and every batch step enforce this policy before contacting the adapter. Without that optional profile restriction, `allow_debug` continues to grant privileged host debugging; results explicitly report `unrestricted_host_debug:true`. This payload restriction and process isolation are separate boundaries. The engine never reads executable debug configuration from the repository.

Engine integration fixtures exercise real stdio subprocesses and protocol messages for all ten IDE tools, unsupported capability/hash/permission failures, unchanged source before apply, apply-time conflicts and a coordinated external writer during RPC. Pure output-adapter tests cover provider-specific Unicode coordinates, real report shapes, safe-fix classification, incomplete reports, count/size limits and repeated-suggestion expansion. Sandbox refusal/policy tests do not substitute for real native sandbox/analyzer acceptance.

See the generated [tool API](API.md), [protocol contracts](PROTOCOL_IDE.md) and the scoped evidence JSON for actual commands and exit codes. Tests authored but not executed remain `NOT_RUN`; full-language/IDE parity is not claimed.
