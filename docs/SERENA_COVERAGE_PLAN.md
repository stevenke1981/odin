# Serena → Odin 獨立實作涵蓋計畫

此表將 Serena v2 時期基準中的 **53 個參考工具** 映射至 Odin 的 **40 個實作需求**。**映射僅代表計畫的覆蓋範圍，不代表相容、已驗收或已完成。** Odin 不打包、不呼叫、不使用 Serena/JB plugin；JetBrains 特定能力以 Rust 協定與語言工具的相同行為目標替代，不承諾相同內部 API。

| Serena 參考功能 | Odin 0.2 原始碼狀態 | 預計 Odin 原生需求 | 說明 |
|---|---|---|---|
| `find_declaration` | NATIVE_SOURCE_PARTIAL | ODIN-007, ODIN-008 | 原生功能對齊；未驗收 |
| `find_implementations` | NATIVE_SOURCE_PARTIAL | ODIN-007, ODIN-008 | 原生功能對齊；未驗收 |
| `find_referencing_symbols` | NATIVE_SOURCE_PARTIAL | ODIN-008 | 原生功能對齊；未驗收 |
| `find_symbol` | NATIVE_SOURCE_PARTIAL | ODIN-008 | 原生功能對齊；未驗收 |
| `get_diagnostics_for_file` | NATIVE_SOURCE_PARTIAL | ODIN-017 | 原生功能對齊；未驗收 |
| `get_diagnostics_for_symbol` | NATIVE_SOURCE_PARTIAL | ODIN-017 | 原生功能對齊；未驗收 |
| `get_symbols_overview` | NATIVE_SOURCE_PARTIAL | ODIN-008 | 原生功能對齊；未驗收 |
| `insert_after_symbol` | NATIVE_SOURCE_PARTIAL | ODIN-006, ODIN-038 | 原生功能對齊；未驗收 |
| `insert_before_symbol` | NATIVE_SOURCE_PARTIAL | ODIN-006, ODIN-038 | 原生功能對齊；未驗收 |
| `rename_symbol` | NATIVE_SOURCE_PARTIAL | ODIN-009, ODIN-006 | 原生功能對齊；未驗收 |
| `replace_symbol_body` | NATIVE_SOURCE_PARTIAL | ODIN-006, ODIN-038 | 原生功能對齊；未驗收 |
| `restart_language_server` | NATIVE_SOURCE_PARTIAL | ODIN-007 | 原生功能對齊；未驗收 |
| `safe_delete_symbol` | NATIVE_SOURCE_PARTIAL | ODIN-016 | 原生功能對齊；未驗收 |
| `jet_brains_debug` | NOT_IMPLEMENTED | ODIN-013, ODIN-036 | 用例替代、非插件相容；未驗收 |
| `jet_brains_find_declaration` | NOT_IMPLEMENTED | ODIN-008 | 用例替代、非插件相容；未驗收 |
| `jet_brains_find_implementations` | NOT_IMPLEMENTED | ODIN-008 | 用例替代、非插件相容；未驗收 |
| `jet_brains_find_referencing_symbols` | NOT_IMPLEMENTED | ODIN-008 | 用例替代、非插件相容；未驗收 |
| `jet_brains_find_symbol` | NOT_IMPLEMENTED | ODIN-008 | 用例替代、非插件相容；未驗收 |
| `jet_brains_get_symbols_overview` | NOT_IMPLEMENTED | ODIN-008 | 用例替代、非插件相容；未驗收 |
| `jet_brains_inline_symbol` | NOT_IMPLEMENTED | ODIN-015 | 用例替代、非插件相容；未驗收 |
| `jet_brains_list_inspections` | NOT_IMPLEMENTED | ODIN-017, ODIN-035 | 用例替代、非插件相容；未驗收 |
| `jet_brains_move` | NOT_IMPLEMENTED | ODIN-014 | 用例替代、非插件相容；未驗收 |
| `jet_brains_rename` | NOT_IMPLEMENTED | ODIN-009 | 用例替代、非插件相容；未驗收 |
| `jet_brains_run_inspections` | NOT_IMPLEMENTED | ODIN-017, ODIN-035 | 用例替代、非插件相容；未驗收 |
| `jet_brains_safe_delete` | NOT_IMPLEMENTED | ODIN-016 | 用例替代、非插件相容；未驗收 |
| `jet_brains_type_hierarchy` | NOT_IMPLEMENTED | ODIN-018 | 用例替代、非插件相容；未驗收 |
| `execute_shell_command` | NATIVE_SOURCE_PARTIAL | ODIN-012 | 原生功能對齊；未驗收 |
| `activate_project` | NATIVE_SOURCE_PARTIAL | ODIN-004, ODIN-023 | 原生功能對齊；未驗收 |
| `get_current_config` | NATIVE_SOURCE_PARTIAL | ODIN-024 | 原生功能對齊；未驗收 |
| `open_dashboard` | NATIVE_SOURCE_PARTIAL | ODIN-025 | 原生功能對齊；未驗收 |
| `remove_project` | NATIVE_SOURCE_PARTIAL | ODIN-004, ODIN-024 | 原生功能對齊；未驗收 |
| `create_text_file` | NATIVE_SOURCE_PARTIAL | ODIN-005, ODIN-006 | 原生功能對齊；未驗收 |
| `delete_lines` | NATIVE_SOURCE_PARTIAL | ODIN-006, ODIN-038 | 原生功能對齊；未驗收 |
| `find_file` | NATIVE_SOURCE_PARTIAL | ODIN-005 | 原生功能對齊；未驗收 |
| `insert_at_line` | NATIVE_SOURCE_PARTIAL | ODIN-006 | 原生功能對齊；未驗收 |
| `list_dir` | NATIVE_SOURCE_PARTIAL | ODIN-005 | 原生功能對齊；未驗收 |
| `read_file` | NATIVE_SOURCE_PARTIAL | ODIN-005 | 原生功能對齊；未驗收 |
| `replace_content` | NATIVE_SOURCE_PARTIAL | ODIN-006, ODIN-038 | 原生功能對齊；未驗收 |
| `replace_in_files` | NATIVE_SOURCE_PARTIAL | ODIN-038 | 原生功能對齊；未驗收 |
| `replace_lines` | NATIVE_SOURCE_PARTIAL | ODIN-006, ODIN-038 | 原生功能對齊；未驗收 |
| `search_for_pattern` | NATIVE_SOURCE_PARTIAL | ODIN-021 | 原生功能對齊；未驗收 |
| `delete_memory` | NATIVE_SOURCE_PARTIAL | ODIN-020 | 原生功能對齊；未驗收 |
| `edit_memory` | NATIVE_SOURCE_PARTIAL | ODIN-020 | 原生功能對齊；未驗收 |
| `list_memories` | NATIVE_SOURCE_PARTIAL | ODIN-020 | 原生功能對齊；未驗收 |
| `read_memory` | NATIVE_SOURCE_PARTIAL | ODIN-020 | 原生功能對齊；未驗收 |
| `rename_memory` | NATIVE_SOURCE_PARTIAL | ODIN-020, ODIN-037 | 原生功能對齊；未驗收 |
| `write_memory` | NATIVE_SOURCE_PARTIAL | ODIN-020 | 原生功能對齊；未驗收 |
| `list_queryable_projects` | NATIVE_SOURCE_PARTIAL | ODIN-004, ODIN-018 | 原生功能對齊；未驗收 |
| `query_project` | NATIVE_SOURCE_PARTIAL | ODIN-018, ODIN-004 | 原生功能對齊；未驗收 |
| `serena_repl` | NATIVE_SOURCE_PARTIAL | ODIN-022 | 用例替代、非插件相容；未驗收 |
| `initial_instructions` | NATIVE_SOURCE_PARTIAL | ODIN-023 | 原生功能對齊；未驗收 |
| `onboarding` | NATIVE_SOURCE_PARTIAL | ODIN-023 | 原生功能對齊；未驗收 |
| `serena_info` | NATIVE_SOURCE_PARTIAL | ODIN-023 | 原生功能對齊；未驗收 |

## 特別不要求 1:1 等價的項目

- `serena_repl`：Odin 使用受限 Rust JSON DAG/flow；沒有 Python `exec/eval`；驗收看複合查詢成功及安全性，不看 Python 腳本語法相容。
- `jet_brains_*`：不使用 Serena JetBrains Plugin；以 LSP / DAP / Code Action / Linter adapters 實作相當的 user-facing workflows，能力受語言與 backend 限制。
- 一般 LSP `find_symbol`／`find_referencing_symbols`：要逐語言驗收，不能用 regex 冒充語意解析。
- 語言多樣性：按 `INSTALLED` / `CONFIGURED` / `VERIFIED` 及 individual capability 報告，不能把存在配置檔計算成已支援。

## 更新規則

完成任何需求先提交新測試工件，再更新 `requirements/features.json` 中的狀態與 `requirements/serena_coverage.json` 中的 `done` 欄位；若沒有真實證據，保持 `false`。執行 `python scripts/check_requirements.py` 檢查資料一致性。
