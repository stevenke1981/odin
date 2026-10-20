# Serena → Odin 功能對照

基準：Serena tools catalog / v2 時期 main commit `abb0a0f01b94bc8050086f0fb860077d17730324`。本表是設計與原始碼映射，不是實測相容認證。

- NATIVE_SOURCE_PARTIAL：有原生對應或刻意改變的工作流；不表示逐參數等價、已編譯或已測試。
- NOT_IMPLEMENTED：本版缺少相對應實作，沒有橋接補足。
- NONE_ON_SERENA：程式不啟動、載入或呼叫 Serena；不是全部功能等價的宣告。
- 所有本表真實後端驗證均為 **NOT_RUN**。

| Serena tool | Odin 原生對應 | 實作分類 | 差異／依賴 |
|---|---|---|---|
| `find_declaration` | `find_declaration` | NATIVE_SOURCE_PARTIAL | 原生接受 LSP position；沒有上游 regex-occurrence 完全同形 API。 |
| `find_implementations` | `find_implementations` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 |
| `find_referencing_symbols` | `find_referencing_symbols` | NATIVE_SOURCE_PARTIAL | 原生位置＋可取得的 enclosing symbol；結果受 LSP / workspace 影響。 |
| `find_symbol` | `find_symbol` | NATIVE_SOURCE_PARTIAL | 原生 name_path substring；未重現全部 kind/depth/grouping 選項。 |
| `get_diagnostics_for_file` | `get_diagnostics_for_file` | NATIVE_SOURCE_PARTIAL | 原生 pull/push report；不等同 IDE inspection 或已完成 build。 |
| `get_diagnostics_for_symbol` | `get_diagnostics_for_symbol` | NATIVE_SOURCE_PARTIAL | 原生 range 篩選；未遞迴檢查全部 referencing symbols。 |
| `get_symbols_overview` | `get_symbols_overview` | NATIVE_SOURCE_PARTIAL | 原生 DocumentSymbol flatten；不重現全部上游輸出格式。 |
| `insert_after_symbol` | `insert_after_symbol` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生來源修改先 preview 再 apply。 |
| `insert_before_symbol` | `insert_before_symbol` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生來源修改先 preview 再 apply。 |
| `rename_symbol` | `rename_symbol` | NATIVE_SOURCE_PARTIAL | 原生文字 WorkspaceEdit → journal；resource operations 明確拒絕。 |
| `replace_symbol_body` | `replace_symbol_body` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生來源修改先 preview 再 apply。 |
| `restart_language_server` | `restart_language_server` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 |
| `safe_delete_symbol` | `safe_delete_symbol` | NATIVE_SOURCE_PARTIAL | 原生只檢查 LSP 靜態引用；不含動態使用或 cascading delete。 |
| `jet_brains_debug` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_find_declaration` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_find_implementations` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_find_referencing_symbols` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_find_symbol` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_get_symbols_overview` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_inline_symbol` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_list_inspections` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_move` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_rename` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_run_inspections` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_safe_delete` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `jet_brains_type_hierarchy` | `—` | NOT_IMPLEMENTED | 未提供 JetBrains 專用功能；不再透過 Serena 補足。一般語意／除錯可使用 Odin 原生 LSP／DAP，但不等價。 |
| `execute_shell_command` | `execute_shell_command` | NATIVE_SOURCE_PARTIAL | 原生僅固定 host argv profile；不接受上游任意 shell 字串。 |
| `activate_project` | `explicit project argument` | NATIVE_SOURCE_PARTIAL | 原生每次指定 project ID；不允許任意 root 動態註冊。 |
| `get_current_config` | `get_current_config` | NATIVE_SOURCE_PARTIAL | 原生 host TOML 與 flags；不含上游 YAML/Jinja contexts/modes。 |
| `open_dashboard` | `HTTP /` | NATIVE_SOURCE_PARTIAL | 原生手動開 loopback / 唯讀頁；沒有完整上游 UI 或自動 browser-open API。 |
| `remove_project` | `host configuration only` | NATIVE_SOURCE_PARTIAL | 原生編輯 host config 後重啟；沒有 Agent 動態刪配置。 |
| `create_text_file` | `create_text_file` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生來源修改先 preview 再 apply。 |
| `delete_lines` | `delete_lines` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生來源修改先 preview 再 apply。 |
| `find_file` | `find_file` | NATIVE_SOURCE_PARTIAL | 原生 glob 對專案相對路徑；參數與上游不完全相同。 |
| `insert_at_line` | `insert_at_line` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生來源修改先 preview 再 apply。 |
| `list_dir` | `list_dir` | NATIVE_SOURCE_PARTIAL | 原生 files_only；不列獨立空目錄。 |
| `read_file` | `read_file` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 |
| `replace_content` | `replace_content` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生來源修改先 preview 再 apply。 |
| `replace_in_files` | `replace_in_files` | NATIVE_SOURCE_PARTIAL | 原生全檔 hash＋match count＋preview；無逐 occurrence selection。 |
| `replace_lines` | `replace_lines` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生來源修改先 preview 再 apply。 |
| `search_for_pattern` | `search_for_pattern` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 |
| `delete_memory` | `delete_memory` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生使用 central Markdown storage。 |
| `edit_memory` | `edit_memory` | NATIVE_SOURCE_PARTIAL | 原生 literal replace＋CAS；非上游全部 regex 選項。 原生使用 central Markdown storage。 |
| `list_memories` | `list_memories` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生使用 central Markdown storage。 |
| `read_memory` | `read_memory` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生使用 central Markdown storage。 |
| `rename_memory` | `rename_memory` | NATIVE_SOURCE_PARTIAL | 原生同 scope 改名及引用更新；無跨 scope move。 原生使用 central Markdown storage。 |
| `write_memory` | `write_memory` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 原生使用 central Markdown storage。 |
| `list_queryable_projects` | `list_projects` | NATIVE_SOURCE_PARTIAL | 原生 list_projects；只有 host 已配置的範圍。 |
| `query_project` | `query_project` | NATIVE_SOURCE_PARTIAL | 原生只接受 nonrecursive native read tools；不啟動 Serena Project Server。 |
| `serena_repl` | `flow_run (not Python)` | NATIVE_SOURCE_PARTIAL | 原生 flow_run 是受限 JSON，不是 Python REPL；沒有任意 Python REPL；此差異不會以橋接補足。 |
| `initial_instructions` | `initial_instructions` | NATIVE_SOURCE_PARTIAL | 有原生對應原始碼；參數／輸出不保證與 Serena 同形。 |
| `onboarding` | `onboarding` | NATIVE_SOURCE_PARTIAL | 原生收集結構與證據；讓 Agent 決定記憶，不宣稱自動理解／測試。 |
| `serena_info` | `initial_instructions + docs` | NATIVE_SOURCE_PARTIAL | 原生文件及 initial_instructions；提供 Odin 自己的工具說明，不提供上游主題 API。 |

## 不只工具名稱的功能

| 領域 | 原生現況 | 尚未對齊／依賴界線 |
|---|---|---|
| 40+ 語言與專用適配 | Generic LSP transport / 可配置伺服器；未驗證所有語言 | 必須逐語言原生適配與驗收；不包含上游 adapters |
| REPL facade `lsp / jb / ide / fs / edit / mem / cfg / ext / shell` | 同領域部分 native tools；不模擬 Python objects/facades | 沒有 Python / JetBrains REPL；待獨立設計，不回退上游 |
| YAML、Jinja、contexts、modes | host TOML、每 project flags / excluded_tools | 原生無完全相同設定語意，沒有上游配置入口 |
| 記憶 topic / ref / rename | Markdown + mem: + CAS + read-only patterns | 原生沒有上游全部 ignore / cross-scope / repo-local 行為 |
| index | SQLite snapshot lexical + LSP 自有語意索引 | 不把兩者說成 Serena 相同的 symbol cache |
| 多專案 | explicit scope，禁止重疊 roots | 直接按專案管理 LSP / DAP process，沒有上游 active_project |
| Dashboard / logs / analytics | 唯讀專案與 metadata audit | 不含上游完整設定 UI、日誌串流與 token analytics |
| DAP vs IDE debug | 原生 generic DAP primitives | 不提供 JetBrains persistent debug REPL 語法 |
| stdio / Streamable HTTP | 原生 official rmcp server + loopback Bearer | Odin 自己提供 tools；沒有外部工具轉發 |
| 安裝／更新／clients hooks | 本地建置腳本、手動 MCP config | 沒有原生重寫 Serena setup/hook/marketplace 流程 |

**整體結論：Odin 0.2.0 不依賴 Serena；功能缺口仍存在。獨立運作與全功能等價是不同的驗收條件。**
