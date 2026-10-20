# Odin 實作與驗收看板

Windows 原生預覽已有真實 Rust build、package tests、binary MCP/HTTP/CLI 與三語後端報告；72 個工具不代表 40 項需求全部完成。每項保留原 DoD 與明確未驗收條件。

資料來源：[features.json](../requirements/features.json)、[逐需求證據](../verification/feature-evidence.json)。`PARTIAL` 不能移入 Done；只有同一候選的全部適用驗收均有實際零退出證據才能標 PASS。Linux/macOS、OS sandbox、廣泛語言／IDE 等價及性能基準未完成。

## M0 · 12 個需求

| ID | 優先 | 工作 | 驗收 | 依賴 |
|---|---|---|---|---|
| [ODIN-001](FEATURE_SPEC.md#odin-001) | P0 | 可重現的 Rust 建置與工具鏈 | `PARTIAL` | 無 |
| [ODIN-002](FEATURE_SPEC.md#odin-002) | P0 | 標準 MCP stdio 與 Streamable HTTP | `PARTIAL` | ODIN-001 |
| [ODIN-003](FEATURE_SPEC.md#odin-003) | P0 | 72 個原生工具契約、版本與能力協商 | `PARTIAL` | ODIN-002 |
| [ODIN-004](FEATURE_SPEC.md#odin-004) | P0 | 每次呼叫專案隔離與生命週期 | `PARTIAL` | ODIN-001 |
| [ODIN-005](FEATURE_SPEC.md#odin-005) | P0 | 跨平台路徑與檔案安全 | `PARTIAL` | ODIN-004 |
| [ODIN-006](FEATURE_SPEC.md#odin-006) | P0 | 預覽、提交與 crash-safe 修改交易 | `PARTIAL` | ODIN-005 |
| [ODIN-011](FEATURE_SPEC.md#odin-011) | P0 | 零 Serena 相依的乾淨主機驗證 | `PARTIAL` | ODIN-001 |
| [ODIN-012](FEATURE_SPEC.md#odin-012) | P0 | 實際 OS 隔離、程序樹與權限 | `PARTIAL` | ODIN-005 |
| [ODIN-027](FEATURE_SPEC.md#odin-027) | P0 | 可重現驗收及 CI 成本限制 | `PARTIAL` | ODIN-001 |
| [ODIN-031](FEATURE_SPEC.md#odin-031) | P0 | 套件供應鏈、機密與對外發布治理 | `PARTIAL` | ODIN-001 |
| [ODIN-032](FEATURE_SPEC.md#odin-032) | P0 | 競態、崩潰與資料完整性驗證 | `PARTIAL` | ODIN-006 |
| [ODIN-033](FEATURE_SPEC.md#odin-033) | P0 | 完整架構文件、開發契約與產品邊界 | `PARTIAL` | ODIN-003 |

## M1 · 4 個需求

| ID | 優先 | 工作 | 驗收 | 依賴 |
|---|---|---|---|---|
| [ODIN-007](FEATURE_SPEC.md#odin-007) | P0 | LSP 初始化、同步、關閉及依賴管理 | `PARTIAL` | ODIN-004 |
| [ODIN-008](FEATURE_SPEC.md#odin-008) | P0 | 完整符號查找與引用關係 | `PARTIAL` | ODIN-007 |
| [ODIN-009](FEATURE_SPEC.md#odin-009) | P0 | WorkspaceEdit 完整支援 | `PARTIAL` | ODIN-006, ODIN-007 |
| [ODIN-010](FEATURE_SPEC.md#odin-010) | P0 | 真實語言伺服器三語驗收 | `PARTIAL` | ODIN-007, ODIN-008, ODIN-009 |

## M2 · 15 個需求

| ID | 優先 | 工作 | 驗收 | 依賴 |
|---|---|---|---|---|
| [ODIN-013](FEATURE_SPEC.md#odin-013) | P1 | 通用 DAP 生命週期與除錯行為 | `PARTIAL` | ODIN-012 |
| [ODIN-014](FEATURE_SPEC.md#odin-014) | P1 | 語意 Move refactoring | `PARTIAL` | ODIN-009, ODIN-008 |
| [ODIN-015](FEATURE_SPEC.md#odin-015) | P1 | 語意 Inline refactoring | `PARTIAL` | ODIN-009 |
| [ODIN-016](FEATURE_SPEC.md#odin-016) | P1 | Safe Delete 與級聯刪除 | `PARTIAL` | ODIN-008, ODIN-009 |
| [ODIN-017](FEATURE_SPEC.md#odin-017) | P1 | 診斷／Linter／IDE inspection 正規化 | `PARTIAL` | ODIN-007, ODIN-006 |
| [ODIN-018](FEATURE_SPEC.md#odin-018) | P1 | Type Hierarchy、外部依賴與跨專案查詢 | `PARTIAL` | ODIN-008 |
| [ODIN-020](FEATURE_SPEC.md#odin-020) | P1 | Markdown 記憶與引用完整性 | `PARTIAL` | ODIN-006 |
| [ODIN-021](FEATURE_SPEC.md#odin-021) | P1 | 增量索引與變更監聽 | `PARTIAL` | ODIN-005 |
| [ODIN-022](FEATURE_SPEC.md#odin-022) | P1 | 安全批次流程與可重用查詢 | `PARTIAL` | ODIN-003, ODIN-004 |
| [ODIN-023](FEATURE_SPEC.md#odin-023) | P1 | CLI / Client setup / Onboarding | `PARTIAL` | ODIN-002 |
| [ODIN-024](FEATURE_SPEC.md#odin-024) | P1 | Context／Mode／Project 配置策略 | `PARTIAL` | ODIN-004 |
| [ODIN-034](FEATURE_SPEC.md#odin-034) | P1 | 語言後端安裝與能力健康檢查 | `PARTIAL` | ODIN-007 |
| [ODIN-038](FEATURE_SPEC.md#odin-038) | P1 | 精確文字編輯與多命中選擇 | `PARTIAL` | ODIN-006 |
| [ODIN-039](FEATURE_SPEC.md#odin-039) | P1 | 後端 debugger 設定及啟動安全 | `PARTIAL` | ODIN-012, ODIN-013 |
| [ODIN-040](FEATURE_SPEC.md#odin-040) | P1 | 工作證據、結果分塊與可追溯執行 | `PARTIAL` | ODIN-003 |

## M3 · 9 個需求

| ID | 優先 | 工作 | 驗收 | 依賴 |
|---|---|---|---|---|
| [ODIN-019](FEATURE_SPEC.md#odin-019) | P1 | 逐語言適配與 40+ 語言宣告策略 | `PARTIAL` | ODIN-010 |
| [ODIN-025](FEATURE_SPEC.md#odin-025) | P1 | Dashboard / 操作日誌 / 使用量 | `PARTIAL` | ODIN-002 |
| [ODIN-026](FEATURE_SPEC.md#odin-026) | P2 | Hooks / Agent 策略引導 | `PARTIAL` | ODIN-023 |
| [ODIN-028](FEATURE_SPEC.md#odin-028) | P1 | Windows x64 MSVC 安裝、升級與卸載 | `PARTIAL` | ODIN-001 |
| [ODIN-029](FEATURE_SPEC.md#odin-029) | P1 | Linux/macOS 原生打包與驗證 | `NOT_RUN` | ODIN-001 |
| [ODIN-030](FEATURE_SPEC.md#odin-030) | P1 | 效能、Token 與品質基準 | `NOT_RUN` | ODIN-010 |
| [ODIN-035](FEATURE_SPEC.md#odin-035) | P2 | 純原生檢查與 Quick Fix 引擎 | `PARTIAL` | ODIN-017 |
| [ODIN-036](FEATURE_SPEC.md#odin-036) | P2 | 持久式除錯命令會話（非 Python REPL） | `PARTIAL` | ODIN-013 |
| [ODIN-037](FEATURE_SPEC.md#odin-037) | P2 | 記憶 schema 升級與 import/export | `PARTIAL` | ODIN-020 |

## 下一個可驗收切片

- 整合 owner：Windows 候選已完成 16 個零退出 gates、144 Rust tests/1 ignored 並綁定 source/binary hash；下一步重新打包最新文件、核對 artifact manifest 與獨立 review。
- 安全／平台 owner：保留預設 deny，另行實作真正 OS sandbox；在乾淨 Windows、Linux、macOS 做 lifecycle／敵意案例，不以 Job Objects 當沙箱。
- 協定／語言 owner：補齊三語 diagnostics/失敗案例、20 個真實符號查詢，再擴六語言；對 Move/Inline/QuickFix 逐後端測編譯與 rollback。
- 資料／效能 owner：repo-local memory migration、背景 watcher、完整 DAG、三規模／20 任務 benchmark。
- 發佈 owner：獨立 review 後只推已審查 commit；核對 public visibility 與 remote HEAD，不預寫已發布，不 dispatch 雲端 CI。

每切片指定唯一 owner 與 write scope；共用 engine/catalog/config 檔案須序列整合。一位作者一個工作樹，不能把整個 src 同派給多人。

## 狀態流與證據

Ready → In progress → Review → Verification → Done。測試宣告、準備中的 runner、protocol fixture、cross compile 和檔案存在，都不替代各自目標的實際驗收。第一個失敗保留；修復產生新 snapshot 後，舊證據不可自動套用。
