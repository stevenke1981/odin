# Direct LSP IDE queries

Odin requests these capabilities from the configured language server directly.
There is no Serena bridge, lexical substitute for semantic symbols, automatic
backend retry, or backend executeCommand path.

## Engine-facing API

Every method below is asynchronous and returns `anyhow::Result<Value>`.
Existing `symbols`, `at`, `hierarchy`, `actions`, `changes`, and lifecycle APIs
remain available.

| Method | Parameters | Capability / result |
|---|---|---|
| `completion` | `path: &str, position: Position` | completionProvider; CompletionList, items, or null |
| `resolve_completion` | `path: &str, item: &Value, expected_sha256: &str` | completionProvider.resolveProvider |
| `hover` | `path: &str, position: Position` | hoverProvider |
| `signature_help` | `path: &str, position: Position` | signatureHelpProvider |
| `workspace_symbols` | `query: &str` | workspaceSymbolProvider; default limit 100 |
| `workspace_symbols_scoped` | `query: &str, scope: Option<&str>, limit: usize` | project-local results, limit 1..1000 |
| `call_hierarchy` | `path: &str, position: Position, direction: &str` | callHierarchyProvider; incoming / outgoing |
| `hierarchy` | existing path / position / direction | typeHierarchyProvider; super / sub |
| `formatting` | `path: &str, options: &Value, range: Option<Range>, expected_sha256: &str` | documentFormattingProvider / documentRangeFormattingProvider; raw WorkspaceEdit |
| `semantic_tokens` | `path: &str, range: Option<Range>` | semanticTokensProvider.full / range; includes negotiated legend |
| `inlay_hints` | `path: &str, range: Range` | inlayHintProvider |
| `resolve_inlay_hint` | `path: &str, hint: &Value, expected_sha256: &str` | inlayHintProvider.resolveProvider |

New document reads use an envelope with `result`, `provider`, `source: "lsp"`,
`document: {path, uri, sha256, version}`, `position_encoding: "utf-16"`, and
`freshness: "current"`. Commands and edits remain inert data; `commands_executed`
and `edits_applied` are false. Hierarchy responses additionally retain the old
`hierarchy` field. Workspace symbol responses contain scope, limit, excluded
count, and truncation information instead of a single document.

Completion/inlay resolve requires the source hash from the originating query.
Source mismatch is `CONFLICT`. The input's opaque server `data` survives resolve;
returned commands are never executed. Completion snippets are not negotiated.
Formatting requires tabSize 1..32 and insertSpaces boolean. It validates edits,
returns a versioned WorkspaceEdit, and never writes the source. Engine must pass
the result through `changes` with its pre-request source snapshot and then core
Transactions for preview/apply. Empty edits do not prove a transaction occurred.
WorkspaceEdit accepts an absent, null, or correctly typed empty unused
`changes`/`documentChanges` alternative, as emitted by JDTLS/LSP4J. Both populated
alternatives remain ambiguous and are rejected; malformed field types are never
ignored. An entirely empty proposal remains an error. Source hashes, document
versions, resource-operation ordering, and forbidden overwrite rules still apply.

## Coordinate and freshness rules

The client offers UTF-16 and rejects a different or malformed selected encoding,
including the legacy offsetEncoding selection. Omitted positionEncoding follows
the LSP UTF-16 default. Positions are zero-based; surrogate-pair splits, reversed
ranges, and out-of-document positions are errors. CRLF, bare CR, and LF are
supported. `text::position_from_offset(text, byte_offset)` converts UTF-8 linter
offsets to UTF-16 and rejects code-point or CRLF splits.

Every document response is checked against a fresh source read and its opened
version after RPC completion. This applies to existing symbols, locations,
rename, and CodeAction requests as well as new IDE reads. It detects source
changes during computation; it does not provide an OS lock after the check or
prove that a backend finished indexing.

Workspace symbols and hierarchy queries also compare bounded matching-language
file hashes before and after requests: at most 10000 listed files and 32 MiB of
source reads per snapshot. Workspace symbol URIs must resolve to allowed files
inside that snapshot; excluded entries are counted. Scope can name an allowed
file/directory. Hierarchy items must be project-local and have valid ranges;
external/generated items outside the verified snapshot fail explicitly.
Readiness and index completeness remain separate from source freshness.

Call/type hierarchy queries share one configured query timeout across source
snapshots, prepare, every related RPC, validation, and final freshness checks.
They return `TIMEOUT` on deadline expiry. Synchronous checks run between bounded
operations; an individual OS file read cannot be preempted by the async timer.
Prepare is limited to 128 items, with at most 4096 related items and 8192 call
ranges across the whole query. All prepared items are reserved before issuing a
related RPC. Retained result JSON is limited to 4 MiB, including keys, escaping,
and array separators; the compatible `result` and `hierarchy` copies together
are limited to 8 MiB of encoded JSON, excluding envelope and allocator overhead.
Counting streams into a bounded counter without allocating serialized copies.
Quota violations return `LIMIT_EXCEEDED` before further RPCs or result cloning;
they never become an empty successful hierarchy. Transport frame limits also
apply to each response, including the response that crosses the total quota.

Semantic token data is checked for five-integer encoding, valid legend indices
and modifier bits, document bounds, overlap, and UTF-16 boundaries. Validation
retains requested token boundaries and scans text once, rather than allocating
a tree node for every source character. Full/range requests are supported;
delta requests and dynamic capability registration are not negotiated.

## Diagnostic metadata

`diagnostics` retains the earlier `available`, `source`, and `report` fields and
adds document metadata, `freshness`, `status`, `version_verified`, normalized
`findings`, and `summary` counts. Each finding includes provider/source/rule ID,
relative path, range, severity, document version, message, and freshness status.
Only version-verified current findings receive a source hash.

`summary.complete` means a valid current report for this document. It does not
mean all workspace files were checked or a build passed. Backend errors and
malformed reports return errors. Pending, stale, and unversioned reports never
become a clean result. Historical stale ranges are not interpreted against the
new source. A current report with no findings is `status: "clean"` while
`clean_build_proven` remains false.

Push diagnostic cache insertion, lookup and invalidation use the same lexical
local-file URI identity. Percent escapes are decoded without filesystem I/O;
Windows drive letters are folded so `file:///C:/...` matches a backend's
`file:///c%3A/...`. Path-component case stays intact on every platform. This
cache identity does not authorize a path or change the source/version checks.
Authority/UNC URIs, query/fragment suffixes, malformed escapes, encoded
separators/NUL and non-UTF-8 paths are explicit protocol errors. The original
report URI is retained. Unversioned backend reports remain unverified even
when their URI matches; URI equivalence never supplies a document version.

## Evidence and boundaries

Legacy RPC/LSP/DAP starts delegate to new `*_with_sandbox` starts with `None`.
Explicit sandbox input is `Option<(&MxcSandboxConfig, &Path)>`; the path is an
existing trusted host policy directory. A configured sandbox uses core's
hash-pinned executor preparation and never retries without the sandbox.
The prepared command's scratch environment is preserved. Pipe setup and process
tree management still apply. Policy/scratch guards survive job/group closure
and parent reaping. Drop transfers a delayed reap to a background thread; it
retains the guard until that reap completes, including after Tokio shutdown.
Explicit cleanup waits are bounded; OS-level delayed background reaping has no
guaranteed completion deadline. Job assignment retains its documented spawn
window and does not imply atomic descendant termination visibility.

Transport status reports the selected isolation route and a verified executor
hash without exposing host paths. `os_boundary_verified` stays false: selecting
that route or completing an LSP handshake is not a filesystem/network denial
test. Native enforcement evidence is recorded by the OS sandbox integration
owner. File-specific document language IDs use the shared registry (for example
TSX maps to typescriptreact), while custom host profile IDs are retained.

`verification/protocol-ide-*` records actual package checks. Fixture peers only
prove protocol behavior, validation, and conflict handling. Multi-language real
backend and binary/tool-routing verification belongs to the integration owner;
passing these fixtures does not demonstrate IDE equivalence or full support for
every backend capability. Configured language servers are trusted executable
profiles; process lifetime controls are not OS security sandboxes.

Primary references: [LSP completion](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/language/completion.md),
[semantic tokens](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/language/semanticTokens.md),
[inlay hints](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/language/inlayHint.md).
