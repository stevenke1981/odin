# Odin 0.2.0 — 不依賴 Serena

## 結論

可以。Odin 核心、MCP、檔案、記憶、交易、LSP/DAP clients 均獨立實作，沒有 Serena bridge 或自動 fallback。Windows binary、stdio/HTTP、三種真實 LSP 及 debugpy 已另行驗收；最新證據見 verification/delivery-status.json，不把功能對照當逐參數等價。

## 此次實際變更

- 刪除 `crates/odin-protocol/src/bridge.rs`、其 module export、`BridgeConfig`、engine 的橋接 cache / 啟動 / dispatch / 權限分支。
- 刪除 `serena_tools`、`serena_call` 及 schema；58 個基礎 Odin 工具保留，目前擴充為 85 個原生契約。
- 刪除橋接設定、呼叫範例和上游 mock fixture。
- `status` 回報 standalone / external_tool_bridge；CLI doctor 檢查直接後端。
- 舊 bridge 設定由 deny_unknown_fields 拒絕；舊工具回未知工具。相關回歸已納入本輪 workspace 與 MCP smoke 執行，證據見最新交付狀態。
- 重新生成 API、53 項功能對照與契約；加入生產原始碼／Cargo 設定的去相依靜態檢查。

## 執行期依賴邊界

| 能力 | 執行期需求 | 是否需要 Serena |
|---|---|---|
| 檔案、詞彙搜尋、Markdown 記憶、修改日誌 | Odin 可執行檔及其系統需求 | 不需要 |
| MCP stdio / 本機 HTTP / Dashboard | Odin；HTTP 模式需 host token | 不需要 |
| 符號、引用、診斷、Rename | 另裝並配置相應 LSP 語言伺服器 | 不需要 |
| 除錯 | 另裝並配置 DAP debug adapter、debuggee 與授權 | 不需要 |
| Windows OS 沙箱／原生 inspections | host 明確配置 hash-pinned native MXC；inspections 另需對應 analyzer | 不需要 |
| Rust 核心 | 不需 Python runtime | 不需要 |
| 開發驗證腳本／fixture | Python；Rust tests 另需 Cargo | 不需要 |

外部後端可以是 Rust、Java、Node.js、Python 等實作；「Rust 核心」不等於每個外部語言後端都用 Rust。安裝了某個後端也不代表其所有能力都已驗證。

## 這次沒有補齊的功能

獨立性並不等於 Serena 全功能等價。Move／Inline 已有後端 edit-only 安全入口；完整逐語言語意重構、JetBrains 私有 inspections、其 persistent debug REPL、完整 Python REPL、40+ 語言專用適配、上游所有 YAML/Jinja/context/mode 語意仍未完整驗收。Clippy／Ruff／ESLint 已有獨立原生 providers，但普通 LSP diagnostics 不等於全部 IDE inspections；DAP primitives 不等於 JetBrains REPL；JSON flow 不等於 Python。

缺少上述能力時，本版不能保證完成對應任務，也不會偷偷呼叫 Serena。功能矩陣保留 NOT_IMPLEMENTED 和 NATIVE_SOURCE_PARTIAL，不以刪除橋接把缺口改標完成。

## 從 0.1.0 遷移

保留舊資料與設定備份，刪除 `[projects.<id>.serena]` 區塊及其子欄位；客戶端不再呼叫 `serena_tools` / `serena_call`。其他原生工具名稱保留；本次沒有刻意變更 journal / memory state 格式，但未做真實跨版本遷移測試。

完成 Rust 建置後，使用 `odin --config odin.toml doctor`、`call status`、MCP 工具列表以及 scripts/mcp_smoke.py 驗證。應在未安裝 Serena 的乾淨環境驗證符號與除錯工作；現在尚無此類執行證據。

## 驗證證據

`verification/source-check.json` 是原始碼／契約靜態檢查；`fixture-selftest.json` 只驗證 Python 模擬服務；`delivery-status.json` 記錄本版實際完成與 NOT_RUN。沒有 Rust／Cargo 就不能宣稱編譯、Rust 測試或效能通過。
