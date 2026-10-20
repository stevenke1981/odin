# Direct protocol implementation

Odin talks directly to configured LSP servers and DAP adapters. Serena is used by
the development agents for code navigation; it is not a product dependency or a
runtime fallback. A backend capability is checked before a request is sent.

## WorkspaceEdit and transactions

`LspClient::changes(edit, snapshots)` returns `Vec<odin_core::transaction::Change>`.
It never writes source files. Engine callers must prepare, preview, and apply the
result through core `Transactions`.

- `changes` text edits and ordered `documentChanges` are supported. Text edits
  retain UTF-16 positions, CRLF/CR/LF line endings, and reject overlap.
- CreateFile, RenameFile and DeleteFile replay against an in-memory file state.
  Text edits following a create/rename use the preceding operation's content.
  Each final path appears once in the core changeset, including hash guards for
  existing files that a sequence leaves unchanged.
- Every existing affected file needs a pre-request text snapshot and exact hash.
  New targets use `expected_sha256: null`. Planning validates current contents;
  core repeats the check at prepare and apply. No conflict is overwritten.
- Windows file URIs are compared to the URL-normalized canonical root, accounting
  for the canonical verbatim prefix. Core path policy still applies. External
  URIs, protected paths, path aliases, directories, missing parent directories,
  recursive delete, `overwrite: true`, and confirmation-required annotations fail
  explicitly. Binary resource operations and directory moves are unsupported.
- Transactions remain recoverable per-file replacements, without multi-file
  atomic visibility. A rename is a journaled text transfer, not an OS rename:
  inode identity, alternate streams, and source metadata are not preserved.

`changes` and `documentChanges` in the same edit are rejected. Empty/no-effect
edits are explicit errors. Backend commands embedded in code actions are not
executed. `resolve_action` only uses capability-gated `codeAction/resolve`;
`actions_with_diagnostics` lets callers provide the actual diagnostic context.

## LSP synchronization and lifecycle

The client follows the advertised open/close and full/incremental sync options.
Incremental synchronization uses one complete old-document UTF-16 range.
Document versions increase across close/reopen; file changes invalidate cached
push diagnostics. Known stale versions report pending/unavailable, never a clean
result. Unversioned push diagnostics remain explicitly unverified. Pull reports
must be full reports and the file/version must still match the request.

`status()` separates initialization, active progress, and backend-reported
quiescence; absent backend readiness evidence remains `unknown`. It never proves
index completeness or a successful build. `is_closed()` supports explicit engine
restart handling. `shutdown()` sends shutdown/exit, waits, then terminates and
reaps a backend that ignores graceful shutdown.

Dynamic capability registration, arbitrary server commands, and implicit
workspace/applyEdit remain disabled. A server requiring unsupported capabilities
must return an explicit error; there is no automatic upstream retry.

## DAP and RPC

Launch/attach/restart returns a ticket after the frame is sent, allowing the client to
wait for `initialized`, configure breakpoints, and send `configurationDone`.
State guards reject duplicate launches, configuration before initialization,
frame/variable/evaluate/step requests outside stopped state, and unsupported
capabilities. Running/stopped/terminated events update the observed state.
`poll()` includes phase, generation, events, and launch outcomes.
Restart can therefore receive a new initialized/configurationDone cycle without
blocking the command path. A step's expected running transition happens before
send; a stopped event received before the response remains authoritative.

RPC timeouts remove pending requests and send cancellation where supported.
Dropping a caller removes its registration and sends best-effort cancellation.
A cancelled/failed pipe write poisons the connection rather than reusing partial
framing. Mutating requests are never retried automatically. DAP ticket
cancellation reports an unknown target outcome; disconnect/new session resolves
that uncertainty. Successful disconnect kills and reaps the adapter tree.

Adapter launches use the core child environment allowlist and ProcessTree guard.
Windows Job Object assignment occurs after spawn; Unix process groups can be
escaped. These are process lifetime controls, not an OS sandbox. A trusted host
`allow_debug` decision authorizes backend-specific debuggee execution and launch
arguments. Reverse terminal/execution requests remain denied.

Fixtures test protocol behavior and conflict handling, not real language or
debugger parity. Real backend verification must name each installed backend and
exercise that backend separately. See `verification/protocol-*` for command
evidence; unsupported OS/backend combinations stay NOT_RUN.

References: [LSP WorkspaceEdit](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/types/workspaceEdit.md),
[LSP synchronization](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/textDocument/didChange.md),
[DAP lifecycle](https://microsoft.github.io/debug-adapter-protocol/overview).

## Verification on 2026-10-21

On Rust 1.90.0, Windows x64 MSVC, the package run executed 26 unit tests and
13 fixture integration tests successfully. The separately selected real
rust-analyzer 1.90.0 document-symbol smoke also passed. These are distinct
evidence levels. Strict package Clippy and scoped rustfmt checks passed; the
workspace/binary/other-language release checks belong to the integration owner.

Logs and source hashes are in `verification/protocol-verification.json` and its
referenced text files. The first Clippy failure is preserved separately. An
earlier test failure exposed Windows verbatim-prefix URI comparison, now covered
by successful native WorkspaceEdit transaction tests.
