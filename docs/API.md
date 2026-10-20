# Odin 工具 API 0.2.0 Standalone

此表由 `schemas/tools.json` 生成；來源包含實作分派，不代表已編譯或通過測試。

所有工具以明確 `project` 範圍執行；CLI 在提供 `--project` 時可注入該欄位。`read` / `state` / `write` / `execute` 是內部策略分類，不是 OS 隔離保證。read 工具仍可能啟動語言伺服器、寫 audit/cache。

`write`：來源 edits 先回 transaction preview，需另 apply；記憶變更則直接經 journal commit。`execute`：要求相應 shell/debug 權限。本版本沒有外部工具橋接。

LSP position 為 0-based UTF-16；檔案行數為 1-based inclusive。參數細節、範圍與 additionalProperties 規則請讀 JSON schema。此版 schema validator 實作契約需要的有限 JSON Schema 子集，非通用 JSON Schema 驗證器。

| Tool | Policy | 必填參數 | 作用 |
|---|---|---|---|
| `status` | read | `project` | Effective standalone project status and backend boundaries |
| `list_projects` | read | `project` | List operator-registered projects without changing a global active project |
| `get_current_config` | read | `project` | Redacted effective host configuration |
| `initial_instructions` | read | `project` | Return Odin's project and transaction usage instructions |
| `onboarding` | read | `project` | Inspect project files and return evidence and memory guidance; never invent build results |
| `list_dir` | read | `project` | List project files respecting ignore rules and path policy |
| `find_file` | read | `project`, `file_mask` | Find paths using a glob, respecting ignore rules |
| `read_file` | read | `project`, `relative_path` | Read bounded UTF-8 text; start/end lines are 1-based and inclusive |
| `search_for_pattern` | read | `project`, `pattern` | Live regex search; returned line numbers are 1-based |
| `index_build` | state | `project` | Rebuild a local SQLite FTS5 lexical snapshot (not vector semantic search) |
| `index_query` | read | `project`, `query` | Query FTS5; stale file hits are omitted; new files require reindexing |
| `get_symbols_overview` | read | `project`, `relative_path` | Hierarchical file symbols from a configured LSP |
| `find_symbol` | read | `project`, `query` | Find matching LSP name paths; never report regex results as semantic symbols |
| `find_referencing_symbols` | read | `project`, `relative_path`, `name_path` | Find LSP references and enclosing project symbols |
| `find_implementations` | read | `project`, `relative_path`, `name_path` | Find implementations using server capability negotiation |
| `find_declaration` | read | `project`, `relative_path`, `position` | Find definition/declaration at a zero-based UTF-16 position |
| `get_type_hierarchy` | read | `project`, `relative_path`, `name_path`, `direction` | Query LSP type hierarchy if the server advertises support |
| `get_diagnostics_for_file` | read | `project`, `relative_path` | Get pull/push diagnostics; pending or unversioned is not a clean-build proof |
| `get_diagnostics_for_symbol` | read | `project`, `relative_path`, `name_path` | Filter file diagnostics to a symbol range |
| `code_actions` | read | `project`, `relative_path`, `range` | List LSP code actions only; never auto-execute a server command |
| `restart_language_server` | state | `project` | Drop project LSP processes; lazily start clean instances on the next query |
| `transaction_prepare` | write | `project`, `changes` | Stage full-file changes guarded by expected SHA-256; null means create-only |
| `transaction_preview` | read | `project`, `transaction_id` | Read a durable transaction preview |
| `transaction_apply` | write | `project`, `transaction_id` | Apply a staged transaction with revalidation and recovery journal |
| `transaction_rollback` | write | `project`, `transaction_id` | Undo a committed transaction using a new conflict-checked transaction |
| `transaction_list` | read | `project` | List journal phases |
| `transaction_recover` | write | `project` | Restore interrupted transactions without overwriting external modifications |
| `create_text_file` | write | `project`, `relative_path`, `content`, `expected_sha256` | Prepare a file create/overwrite; explicit transaction_apply is required |
| `replace_content` | write | `project`, `relative_path`, `needle`, `replacement`, `expected_sha256` | Prepare guarded literal/regex replacement, preserving full-file hash precondition |
| `replace_in_files` | write | `project`, `files`, `needle`, `replacement`, `expected_total_matches` | Prepare a single guarded cross-file replacement journal; expected_total_matches is mandatory |
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
| `check_memories` | read | `project` | Validate explicit mem: reference integrity |
| `search_memories` | read | `project`, `pattern` | Search names/body by regex without returning every memory body |
| `recover_memories` | write | `project` | Recover an interrupted memory journal; shared across configured projects |
| `query_project` | read | `project`, `target_project`, `tool`, `arguments` | Call a read-only native tool against another configured project |
| `flow_run` | read | `project`, `session`, `steps` | Run a bounded JSON dataflow; no eval, imports, arbitrary Python, shell, or external tool forwarding |
| `flow_vars` | read | `project`, `session` | List retained per-project flow variable names and sizes |
| `flow_clear` | state | `project`, `session` | Clear this per-project flow session |
| `execute_shell_command` | execute | `project`, `profile` | Run a fixed operator-defined argv profile; no caller-supplied shell string |
| `debug_start` | execute | `project`, `profile` | Start a configured DAP adapter; requires allow_debug |
| `debug_request` | execute | `project`, `command`, `arguments` | Send an allowlisted DAP request; launch/attach return asynchronous tickets |
| `debug_events` | execute | `project` | Drain DAP events and completed launch tickets |
| `debug_stop` | execute | `project` | Disconnect debuggee with termination requested and drop the adapter |
| `result_read` | read | `project`, `result_id` | Read a spilled large result by byte chunks; offsets remain UTF-8 aligned |
| `audit_log` | read | `project` | Read redacted tool audit records without input/output source contents |

## 結果／錯誤

成功不強制單一業務 envelope；MCP 回 content + structuredContent。業務錯誤使用 isError 與 `{error:{code,message}}`。常見 code：INVALID_ARGUMENT、PERMISSION_DENIED、CONFLICT、CAPABILITY_UNAVAILABLE、BACKEND_ERROR、RECOVERY_REQUIRED、LIMIT_EXCEEDED。

大型結果回 result_id、bytes、SHA-256；以 result_read 分塊讀取。輸出 chunk 為 UTF-8 byte offsets，不是字數。原生限制避免把巨大完整結果直接塞給模型，但沒有實測 token 節省比例。

索引與 source edits 是不同資料流；原生 edits 後 LSP 會 refresh，但 FTS5 需要明確重建快照。
