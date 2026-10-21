# Odin 實作與驗收看板

**現行85契約；38 PARTIAL、2 NOT_RUN。** 本輪原生 OS sandbox/language/IDE 來源已整合，但完整40需求未完成。原58工具保留，前版72再新增13。需求及原DoD見 [FEATURE_SPEC.md](FEATURE_SPEC.md)，實際範圍見 [feature-evidence.json](../verification/feature-evidence.json)。

前版72工具的144 tests/1 ignored只屬其 source-bound baseline。本輪 native MXC author2測試PASS、protocol29+22、engine prior74／latest62+12 blocked、symbol targeted1；實際三 analyzer 的30 checks PASS綁revision30a957a，早於hierarchy修復。六語首輪0 MCP/LSP checks，不能稱六語或28語言已驗收。當輪fe5996d已完成17gates、191tests/4ignored，另native3實跑；六語矩陣及final analyzer replay仍pending，不用舊成功覆蓋。

## M0 · 12 個需求

| ID | 優先 | 工作 | 驗收 | 依賴 |
|---|---|---|---|---|
| [ODIN-001](FEATURE_SPEC.md#odin-001) | P0 | 可重現的 Rust 建置與工具鏈 | `PARTIAL` | 無 |
| [ODIN-002](FEATURE_SPEC.md#odin-002) | P0 | 標準 MCP stdio 與 Streamable HTTP | `PARTIAL` | ODIN-001 |
| [ODIN-003](FEATURE_SPEC.md#odin-003) | P0 | 85 個原生工具契約、版本與能力協商 | `PARTIAL` | ODIN-002 |
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

## 當輪後續切片與唯一 owner

| Owner | Write scope | 驗收與停止條件 |
|---|---|---|
| protocol author | protocol hierarchy 累計預算與對應測試 | 修復 SIR-001 的累計bytes/requests/deadline；以新 immutable head 獨立 review |
| root integration | shared core/engine 整合、專屬當輪 report/logs | 17gates、191tests/4ignored、native3及standard binary smoke已完成；補六語/final analyzer replay、review與package，記新SHA/digest，不覆蓋baseline |
| platform author | 六語各兩專案 harness／證據 | 原4551嘗試保留FAIL/0checks；有實際握手與功能結果才升相應能力；不改host安全政策取得綠燈 |
| independent reviewer | immutable diff與唯讀檢查 | 作者不自我核准；未解阻擋發現不發布；fixture不當真語言證據 |
| root release | 候選docs/artifact manifest與已授權發布 | 完成新候選所有適用gate再打包、核對remote/public；本輪未執行的發布及cloudCI不預填成功 |

只有實際對應全部原acceptance才可把需求升PASS；來源已實作、profile registry、後端版本存在、準備好harness都不等於驗收。共有檔案序列整合，一位作者一個worktree，不覆寫衝突取得綠燈。Linux/macOS、全語言／IDE等價、完整安裝升級／故障矩陣、benchmark仍未完成。
