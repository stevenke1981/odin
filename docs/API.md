# Odin 工具 API 0.2.0 Standalone

此表由 `schemas/tools.json` 生成；來源包含實作分派，不代表已編譯或通過測試。

所有工具以明確 `project` 範圍執行；CLI 在提供 `--project` 時可注入該欄位。`read` / `state` / `write` / `execute` 是內部策略分類，不是 OS 隔離保證。read 工具仍可能啟動語言伺服器、寫 audit/cache。

`write`：來源 edits 先回 transaction preview，需另 apply；記憶變更則直接經 journal commit。`execute`：要求相應 shell/debug 權限。本版本沒有外部工具橋接。

LSP position 為 0-based UTF-16；檔案行數為 1-based inclusive。參數細節、範圍與 additionalProperties 規則請讀 JSON schema。此版 schema validator 實作契約需要的有限 JSON Schema 子集，非通用 JSON Schema 驗證器。

| Tool | Policy | 必填參數 | 作用 |
|---|---|---|---|
| `status` | read | `project` | Effective standalone project status and backend boundaries |
| `get_version` | read | `project` | Return Odin version and native schema format version |
| `get_schemas` | read | `project` | Return tool contracts or one named tool schema |
| `get_capabilities` | read | `project` | Report effective tool policy and optionally initialize configured language servers to probe capabilities |
| `list_projects` | read | `project` | List operator-registered projects without changing a global active project |
| `get_current_config` | read | `project` | Redacted effective host configuration |
| `initial_instructions` | read | `project` | Return Odin's project and transaction usage instructions |
| `onboarding` | read | `project` | Inspect project files and return evidence and memory guidance; never invent build results |
| `list_dir` | read | `project` | List project files respecting ignore rules and path policy |
| `find_file` | read | `project`, `file_mask` | Find paths using a glob, respecting ignore rules |
| `read_file` | read | `project`, `relative_path` | Read bounded UTF-8 text; start/end lines are 1-based and inclusive |
| `search_for_pattern` | read | `project`, `pattern` | Live regex search; returned line numbers are 1-based |
| `index_build` | state | `project` | Rebuild a local SQLite FTS5 lexical snapshot (not vector semantic search) |
| `index_query` | read | `project`, `query` | Refresh changed/new/deleted files then query local FTS5 lexical search |
| `get_symbols_overview` | read | `project`, `relative_path` | File symbols from a configured LSP with kind, scope and hierarchy depth filters |
| `find_symbol` | read | `project`, `query` | Find real LSP name paths with exact/substring/glob/regex matching, filters and pagination |
| `find_referencing_symbols` | read | `project`, `relative_path`, `name_path` | Find LSP references and enclosing project symbols |
| `find_implementations` | read | `project`, `relative_path`, `name_path` | Find implementations using server capability negotiation |
| `find_declaration` | read | `project`, `relative_path`, `position` | Find definition/declaration at a zero-based UTF-16 position |
| `get_type_hierarchy` | read | `project`, `relative_path`, `name_path`, `direction` | Query LSP type hierarchy if the server advertises support |
| `get_diagnostics_for_file` | read | `project`, `relative_path` | Get pull/push diagnostics; pending or unversioned is not a clean-build proof |
| `get_diagnostics_for_symbol` | read | `project`, `relative_path`, `name_path` | Filter file diagnostics to a symbol range |
| `code_actions` | read | `project`, `relative_path`, `range` | List LSP code actions only; never auto-execute a server command |
| `prepare_code_action` | write | `project`, `relative_path`, `range`, `action_title`, `expected_sha256` | Prepare exactly one edit-only LSP code action as a CAS transaction; reject commands and ambiguous titles |
| `apply_quick_fix` | write | `project`, `relative_path`, `range`, `action_title`, `expected_sha256` | Preview one real LSP quickfix WorkspaceEdit; transaction_apply is still required |
| `move_symbol` | write | `project`, `relative_path`, `name_path`, `action_title`, `expected_sha256`, `target_path` | Preview backend-provided refactor.move edit-only code action; no text fallback or server command execution |
| `inline_symbol` | write | `project`, `relative_path`, `name_path`, `action_title`, `expected_sha256` | Preview backend-provided refactor.inline edit-only code action; no text fallback or server command execution |
| `restart_language_server` | state | `project` | Drop project LSP processes; lazily start clean instances on the next query |
| `transaction_prepare` | write | `project`, `changes` | Stage full-file changes guarded by expected SHA-256; null means create-only |
| `transaction_preview` | read | `project`, `transaction_id` | Read a durable transaction preview |
| `transaction_apply` | write | `project`, `transaction_id` | Apply a staged transaction with revalidation and recovery journal |
| `transaction_rollback` | write | `project`, `transaction_id` | Undo a committed transaction using a new conflict-checked transaction |
| `transaction_list` | read | `project` | List journal phases |
| `transaction_recover` | write | `project` | Restore interrupted transactions without overwriting external modifications |
| `create_text_file` | write | `project`, `relative_path`, `content`, `expected_sha256` | Prepare a file create/overwrite; explicit transaction_apply is required |
| `replace_content` | write | `project`, `relative_path`, `needle`, `replacement`, `expected_sha256` | Prepare guarded literal/regex replacement, preserving full-file hash precondition |
| `replacement_preview` | read | `project`, `files`, `needle`, `replacement` | List source-hash-bound occurrence IDs and before/after text without changing source |
| `replace_in_files` | write | `project`, `files`, `needle`, `replacement`, `expected_total_matches` | Prepare selected immutable occurrence IDs or all matches with exact source hashes and total count |
| `replace_lines` | write | `project`, `relative_path`, `start_line`, `expected_sha256`, `end_line`, `content` | Prepare line edit; line indices are 1-based and end is inclusive |
| `delete_lines` | write | `project`, `relative_path`, `start_line`, `expected_sha256`, `end_line` | Prepare line edit; line indices are 1-based and end is inclusive |
| `insert_at_line` | write | `project`, `relative_path`, `start_line`, `expected_sha256`, `content` | Prepare line edit; line indices are 1-based and end is inclusive |
| `replace_symbol_body` | write | `project`, `relative_path`, `name_path`, `body`, `expected_sha256` | Prepare a symbol-bound edit using UTF-16 ranges and an exact content hash |
| `insert_before_symbol` | write | `project`, `relative_path`, `name_path`, `body`, `expected_sha256` | Prepare a symbol-bound edit using UTF-16 ranges and an exact content hash |
| `insert_after_symbol` | write | `project`, `relative_path`, `name_path`, `body`, `expected_sha256` | Prepare a symbol-bound edit using UTF-16 ranges and an exact content hash |
| `rename_symbol` | write | `project`, `relative_path`, `name_path`, `new_name` | Ask LSP for a workspace rename, validate all text edits, return a transaction preview |
| `safe_delete_symbol` | write | `project`, `relative_path`, `name_path`, `expected_sha256` | Prepare deletion only when LSP reports no static references; dynamic uses remain outside the guarantee |
| `list_memories` | read | `project` | List Markdown memory names without loading content |
| `read_memory` | read | `project`, `name` | Read Markdown memory and its current hash |
| `write_memory` | write | `project`, `name`, `content`, `expected_sha256` | Write a Markdown memory with compare-and-swap; null hash is create-only |
| `edit_memory` | write | `project`, `name`, `needle`, `replacement`, `expected_sha256` | Guarded literal replacement in an existing Markdown memory |
| `delete_memory` | write | `project`, `name`, `expected_sha256` | Delete a Markdown memory guarded by hash; use check_memories to inspect dangling references |
| `rename_memory` | write | `project`, `old_name`, `new_name`, `expected_sha256` | Rename Markdown and exact mem: references with a shared recovery journal |
| `memory_export` | read | `project` | Export versioned Markdown notes with SHA-256 without writing repository files |
| `memory_import` | write | `project`, `bundle` | Prepare create-only import of a checked versioned memory bundle; apply separately |
| `memory_move_scope` | write | `project`, `name`, `destination_project`, `destination_name`, `expected_sha256` | Prepare permission-checked memory move to a configured project or global scope; reject dangling references |
| `memory_transaction_preview` | read | `project`, `transaction_id` | Preview a memory transaction owned by this initiating project |
| `memory_transaction_apply` | write | `project`, `transaction_id` | Apply a memory transaction after rechecking initiating project, affected scopes and source hashes |
| `check_memories` | read | `project` | Validate explicit mem: reference integrity |
| `search_memories` | read | `project`, `pattern` | Search names/body by regex without returning every memory body |
| `recover_memories` | write | `project` | Recover an interrupted memory journal; shared across configured projects |
| `query_project` | read | `project`, `target_project`, `tool`, `arguments` | Call a read-only native tool against another configured project |
| `flow_run` | read | `project`, `session`, `steps` | Run session-isolated bounded JSON dataflow with pointer/map/filter/count/sum; no eval, imports or arbitrary Python |
| `flow_vars` | read | `project`, `session` | List retained per-project flow variable names and sizes |
| `flow_clear` | state | `project`, `session` | Clear this per-project flow session |
| `execute_shell_command` | execute | `project`, `profile` | Run a fixed operator-defined argv profile; no caller-supplied shell string |
| `debug_start` | execute | `project`, `profile` | Start a configured DAP adapter; requires allow_debug |
| `debug_request` | execute | `project`, `command`, `arguments` | Send an allowlisted DAP request; launch/attach return asynchronous tickets |
| `debug_events` | execute | `project` | Drain DAP events and completed launch tickets |
| `debug_stop` | execute | `project` | Disconnect debuggee with termination requested and drop the adapter |
| `debug_batch` | execute | `project`, `session_id`, `commands` | Run 1..32 allowlisted DAP requests in an explicit project session with per-step request IDs |
| `result_read` | read | `project`, `result_id` | Read a spilled large result by byte chunks; offsets remain UTF-8 aligned |
| `audit_log` | read | `project` | Read redacted tool audit records without input/output source contents |

## 結果／錯誤

MCP 回 content + structuredContent。業務錯誤使用 isError 與 `{error:{code,message}}`。常見 code：INVALID_ARGUMENT、PERMISSION_DENIED、CONFLICT、CAPABILITY_UNAVAILABLE、MISSING_BACKEND、BACKEND_ERROR、RECOVERY_REQUIRED、LIMIT_EXCEEDED、RESULT_EXPIRED、RESULT_NOT_FOUND、SESSION_CLOSED。

成功結果包含 `trace.id`、`trace.result_sha256`、`trace.result_bytes`、`trace.elapsed_ms`；digest 是加上 trace 前的 JSON bytes。audit 記錄同一 trace ID 與錯誤 code，不保存工具輸入輸出內容。未經真實後端驗證的功能仍需分別驗收。

大型結果回 result_id、bytes、SHA-256、expires_ms；以 result_read 分塊讀取，整份 digest 和 chunk_sha256 可校驗。輸出 chunk 為 UTF-8 byte offsets，不是字數。結果依 project 隔離，保留 24 小時，最多 128 份／128 MiB；達配額拒絕新 spill，不提前驅逐未到期結果。清理於 spill/read 到期時進行，沒有背景排程。

`replacement_preview` 列出 source hash 與搜尋選項綁定的 occurrence IDs；`replace_in_files.selected_occurrence_ids` 可只選部分命中。expected_total_matches 是所有匹配數，selected_matches 是實際選取數；空匹配、重複檔案或 ID、錯誤 hash 均拒絕。Regex flags 為 case_sensitive、multiline、dot_matches_new_line；行為由 Rust regex 決定，這是文字編輯。

`prepare_code_action`／`move_symbol`／`inline_symbol`／`apply_quick_fix` 只接受實際 LSP 提供的 edit-only CodeAction，必須以唯一 title 選擇，先回交易預覽。需要 resolve 的 action 只呼叫 codeAction/resolve；含 server command、disabled、kind 不符、缺少 edit 均拒絕。Move 必須真的修改指定 source 和 target。沒有後端相應語意能力就回明確錯誤。

`memory_export` 回 `bundle`；`memory_import` 與 `memory_move_scope` 先回 shared memory transaction preview，以 memory_transaction_apply 套用。所有 affected scope 與既有 hash 重新核對；現存目標不覆寫。舊 write/edit/delete/rename_memory 保持直接 journal commit。

flow 的 namespace 依 (project, session) 各自序列化；不同 session 可進行獨立流程，每次 native tool 仍經 project gate 和 policy。pointer/take/map/filter/reduce 是有界資料操作，sum 使用有限 f64，無任意程式執行。整條 flow 使用 host RPC timeout，失敗／取消不提交 namespace；busy session 的 flow_clear 回 CONFLICT。

`debug_start` 回 session_id；debug_batch 必填該 ID，每步最多 32 個 allowlisted DAP request，回每步 request_id 和真實 outcome。launch/attach 是 pending ticket，需配置完成並 poll 才能判斷啟動完成。allow_debug 由 host 明確授權 debugger target execution；它不是 OS 沙箱，也沒有宣稱限制 launch payload 的所有除錯器特有欄位。

FTS5 查詢先增量 refresh 當前檔案快照；這是詞彙搜尋，不是語意分析，也沒有 watcher 或 embeddings。詳見 [引擎實作與驗證界線](ENGINE_IMPLEMENTATION.md)。
