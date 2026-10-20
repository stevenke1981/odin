# Odin 實作順序與驗收看板

**未實作不等於不存在原始碼；SOURCE_PREVIEW/NATIVE_SOURCE_PARTIAL/NOT_VERIFIED/NOT_IMPLEMENTED 皆不是 DONE。** 必須依需求 ID 自我驗收，未取得實際編譯/測試結果前不可蓋章。這份工作清單不啟動背景工作或雲端 CI。

## M0 · 12 個需求

| ID | 優先 | 工作 | 原始碼現況 | 依賴 |
|---|---|---|---|---|
| [ODIN-001](FEATURE_SPEC.md) | P0 | 可重現的 Rust 建置與工具鏈 | `SOURCE_PREVIEW` | — |
| [ODIN-002](FEATURE_SPEC.md) | P0 | 標準 MCP stdio 與 Streamable HTTP | `NATIVE_SOURCE_PARTIAL` | ODIN-001 |
| [ODIN-003](FEATURE_SPEC.md) | P0 | 58 個工具契約、版本與能力協商 | `NATIVE_SOURCE_PARTIAL` | ODIN-002 |
| [ODIN-004](FEATURE_SPEC.md) | P0 | 每次呼叫專案隔離與生命週期 | `NATIVE_SOURCE_PARTIAL` | ODIN-001 |
| [ODIN-005](FEATURE_SPEC.md) | P0 | 跨平台路徑與檔案安全 | `NATIVE_SOURCE_PARTIAL` | ODIN-004 |
| [ODIN-006](FEATURE_SPEC.md) | P0 | 預覽、提交與 crash-safe 修改交易 | `NATIVE_SOURCE_PARTIAL` | ODIN-005 |
| [ODIN-011](FEATURE_SPEC.md) | P0 | 零 Serena 相依的乾淨主機驗證 | `SOURCE_STATIC_ONLY` | ODIN-001 |
| [ODIN-012](FEATURE_SPEC.md) | P0 | 實際 OS 隔離、程序樹與權限 | `NOT_IMPLEMENTED` | ODIN-005 |
| [ODIN-027](FEATURE_SPEC.md) | P0 | 可重現驗收及 CI 成本限制 | `SOURCE_PREVIEW` | ODIN-001 |
| [ODIN-031](FEATURE_SPEC.md) | P0 | 套件供應鏈、機密與對外發布治理 | `SOURCE_PREVIEW` | ODIN-001 |
| [ODIN-032](FEATURE_SPEC.md) | P0 | 競態、崩潰與資料完整性驗證 | `NOT_VERIFIED` | ODIN-006 |
| [ODIN-033](FEATURE_SPEC.md) | P0 | 完整架構文件、開發契約與產品邊界 | `SOURCE_PREVIEW` | ODIN-003 |

## M1 · 4 個需求

| ID | 優先 | 工作 | 原始碼現況 | 依賴 |
|---|---|---|---|---|
| [ODIN-007](FEATURE_SPEC.md) | P0 | LSP 初始化、同步、關閉及依賴管理 | `NATIVE_SOURCE_PARTIAL` | ODIN-004 |
| [ODIN-008](FEATURE_SPEC.md) | P0 | 完整符號查找與引用關係 | `NATIVE_SOURCE_PARTIAL` | ODIN-007 |
| [ODIN-009](FEATURE_SPEC.md) | P0 | WorkspaceEdit 完整支援 | `NATIVE_SOURCE_PARTIAL` | ODIN-006, ODIN-007 |
| [ODIN-010](FEATURE_SPEC.md) | P0 | 真實語言伺服器三語驗收 | `NOT_VERIFIED` | ODIN-007, ODIN-008, ODIN-009 |

## M2 · 15 個需求

| ID | 優先 | 工作 | 原始碼現況 | 依賴 |
|---|---|---|---|---|
| [ODIN-013](FEATURE_SPEC.md) | P1 | 通用 DAP 生命週期與除錯行為 | `NATIVE_SOURCE_PARTIAL` | ODIN-012 |
| [ODIN-014](FEATURE_SPEC.md) | P1 | 語意 Move refactoring | `NOT_IMPLEMENTED` | ODIN-009, ODIN-008 |
| [ODIN-015](FEATURE_SPEC.md) | P1 | 語意 Inline refactoring | `NOT_IMPLEMENTED` | ODIN-009 |
| [ODIN-016](FEATURE_SPEC.md) | P1 | Safe Delete 與級聯刪除 | `NATIVE_SOURCE_PARTIAL` | ODIN-008, ODIN-009 |
| [ODIN-017](FEATURE_SPEC.md) | P1 | 診斷／Linter／IDE inspection 正規化 | `NATIVE_SOURCE_PARTIAL` | ODIN-007, ODIN-006 |
| [ODIN-018](FEATURE_SPEC.md) | P1 | Type Hierarchy、外部依賴與跨專案查詢 | `NATIVE_SOURCE_PARTIAL` | ODIN-008 |
| [ODIN-020](FEATURE_SPEC.md) | P1 | Markdown 記憶與引用完整性 | `NATIVE_SOURCE_PARTIAL` | ODIN-006 |
| [ODIN-021](FEATURE_SPEC.md) | P1 | 增量索引與變更監聽 | `NATIVE_SOURCE_PARTIAL` | ODIN-005 |
| [ODIN-022](FEATURE_SPEC.md) | P1 | 安全批次流程與可重用查詢 | `NATIVE_SOURCE_PARTIAL` | ODIN-003, ODIN-004 |
| [ODIN-023](FEATURE_SPEC.md) | P1 | CLI / Client setup / Onboarding | `NATIVE_SOURCE_PARTIAL` | ODIN-002 |
| [ODIN-024](FEATURE_SPEC.md) | P1 | Context／Mode／Project 配置策略 | `NATIVE_SOURCE_PARTIAL` | ODIN-004 |
| [ODIN-034](FEATURE_SPEC.md) | P1 | 語言後端安裝與能力健康檢查 | `NOT_IMPLEMENTED` | ODIN-007 |
| [ODIN-038](FEATURE_SPEC.md) | P1 | 精確文字編輯與多命中選擇 | `NATIVE_SOURCE_PARTIAL` | ODIN-006 |
| [ODIN-039](FEATURE_SPEC.md) | P1 | 後端 debugger 設定及啟動安全 | `NOT_IMPLEMENTED` | ODIN-012, ODIN-013 |
| [ODIN-040](FEATURE_SPEC.md) | P1 | 工作證據、結果分塊與可追溯執行 | `NATIVE_SOURCE_PARTIAL` | ODIN-003 |

## M3 · 9 個需求

| ID | 優先 | 工作 | 原始碼現況 | 依賴 |
|---|---|---|---|---|
| [ODIN-019](FEATURE_SPEC.md) | P1 | 逐語言適配與 40+ 語言宣告策略 | `NOT_IMPLEMENTED` | ODIN-010 |
| [ODIN-025](FEATURE_SPEC.md) | P1 | Dashboard / 操作日誌 / 使用量 | `NATIVE_SOURCE_PARTIAL` | ODIN-002 |
| [ODIN-026](FEATURE_SPEC.md) | P2 | Hooks / Agent 策略引導 | `NOT_IMPLEMENTED` | ODIN-023 |
| [ODIN-028](FEATURE_SPEC.md) | P1 | Windows x64 MSVC 安裝、升級與卸載 | `NOT_IMPLEMENTED` | ODIN-001 |
| [ODIN-029](FEATURE_SPEC.md) | P1 | Linux/macOS 原生打包與驗證 | `NOT_VERIFIED` | ODIN-001 |
| [ODIN-030](FEATURE_SPEC.md) | P1 | 效能、Token 與品質基準 | `NOT_VERIFIED` | ODIN-010 |
| [ODIN-035](FEATURE_SPEC.md) | P2 | 純原生檢查與 Quick Fix 引擎 | `NOT_IMPLEMENTED` | ODIN-017 |
| [ODIN-036](FEATURE_SPEC.md) | P2 | 持久式除錯命令會話（非 Python REPL） | `NOT_IMPLEMENTED` | ODIN-013 |
| [ODIN-037](FEATURE_SPEC.md) | P2 | 記憶 schema 升級與 import/export | `NOT_IMPLEMENTED` | ODIN-020 |

## Ready → In progress → Code review → Verification → Done

- **Ready：** 已有需求、owner、完整 acceptance/test IDs、無 blockers。
- **In progress：** source + regression tests 同 PR，pre-commit 僅測本次影響，不跑 hosted GitHub CI。
- **Code review：** 至少檢查安全、LSP/DAP 後端、跨平台/UX 降級、不誤報完成。
- **Verification：** 真實 runner 存出 command、exit code、commit SHA、OS/version、artifact hashes。
- **Done：** 所有 acceptance 都可重現；未達成則回 Ready/In progress，而不是以規格完成代替產品完成。

## Agent 建議分工（純角色，未啟動子代理）

`planner` 需求拆解／驗收設計；`rust-core` 安全與交易；`protocol` MCP/LSP/DAP；`refactor` symbol/WorkspaceEdit；`platform` Windows/Linux/macOS；`qa-security` 負例驗收；`release` GitHub/文件。每個角色各用 `AGENTS.md`/task file 執行，未有真實 worker 不宣稱多人正在工作。
