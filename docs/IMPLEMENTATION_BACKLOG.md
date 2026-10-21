# Odin 實作與驗收看板

**現行85契約；40項原始需求尚未全數接受。** 原58保留、前版72再新增13。完整原DoD與相依見 [FEATURE_SPEC.md](FEATURE_SPEC.md)，逐需求結果見 [feature-evidence.json](../verification/feature-evidence.json)。以下分開記錄歷史實跑與最終候選，不把不同來源／binary的結果合併成PASS。

最終 Windows 候選由來源 `69e769be9baa4e825139b72ce1a9b48bf8143114` 編譯，binary SHA-256 為 `83a81d14718308973697030fb19d86df7f64f482e2cb555b7899df7ea5d3ec44`。`cargo build --workspace --release --locked` exit0；metadata 的 CreateProcess 遭 AppControl 4551 拒絕，沒有原生 exit code。最終六語／十二專案矩陣、CLI runtime 與 installer 驗收均 **BLOCKED／NOT_RUN**，不能從編譯成功推論可執行。[最終 host block](../verification/language-installers-v3-final-host-block.json) 保留最初 invocation 與 stale LASTEXITCODE 的更正；[finalrun-plan](../verification/language-sandbox-v3-finalrun-plan.json) 是準備好的重跑計畫，不是執行收據。

歷史 `780c6ad` 候選（binary SHA-256 `371d4fbd36fffa334b0ef052d4d1157c1c799562a3dd8fd619eebfefe2b12451`）的 [continuation-01](../verification/language-installers-v3-continuation-01.json) 已實跑 CLI、MCP stdio／HTTP／取消、真實後端、native inspections、Clippy 與供應鏈等檢查；該獨立 continuation PASS 不取代 workspace gate。[workspace-all-targets-01](../verification/language-installers-v3-workspace-all-targets-01.json) 仍為 exit101／FAIL，五個 stdin 子程序 fixture 因4551未能啟動。相同780 binary的 [Windows candidate installer](../verification/package-v3-windows-candidate.json) 有22項PASS，涵蓋安裝、重裝、移除及保留使用者檔案；這是歷史候選的限定生命週期驗收，不能轉移到83a81d最終binary或視為完整乾淨機器／跨版升級矩陣。

語言實跑已超過「其餘五語NOT_RUN」的舊狀態：[source-bound evidence index](../verification/language-sandbox-v3-evidence-index-01.json) 核對18份收據與原始失敗，該index本身只是離線完整性驗證。歷史780 native MCP結果包括 [Go46 PASS／4 UNAVAILABLE](../verification/language-sandbox-v3-go-native-02.json)、[Python60 PASS／4 UNAVAILABLE](../verification/language-sandbox-v3-python-native-02.json) 與 [Ruff formatter18 PASS](../verification/language-sandbox-v3-ruff-native-01.json)。Python使用明確選取、逐檔hash核對的新私有backend副本，原runtime與來源仍RO；可寫副本不能在沙箱外執行或重用為可信SDK。Java與TypeScript已有native語意與CAS實跑，但未附document version的診斷仍為NOT_VERIFIED；[混合五語首輪](../verification/language-sandbox-v3-five-native-01.json) 的150 PASS／4 FAIL／4 UNAVAILABLE／12 NOT_VERIFIED原樣保留。C# [OmniSharp direct null-PID觀察](../verification/language-sandbox-v3-direct-csharp-nullpid-07.json) 僅證明限定初始化／symbols／shutdown路徑；不是Odin MCP雙專案IDE或rename/apply/rollback驗收，最終矩陣仍受上述host block阻擋。

Linux最終69e769b來源正在本機VM建置（**IN_PROGRESS**），尚無本輪完成的原生測試與installer收據，不能升PASS；macOS仍未驗收。ODIN-029整項狀態由root依Linux最終結果再更新。歷史c5084f4、72工具及早期失敗保留原scope；28個registry entries、初始化或通用gate都不代表完整語言／IDE等價。最終source與review binding由root及獨立reviewer管理。

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

## 剩餘切片與唯一owner

| Owner | Write scope | 驗收與停止條件 |
|---|---|---|
| root integration | shared整合、最終build/runtime report與delivery／feature evidence | 最終69e769b編譯PASS與83a81d執行前4551分開；workspace exit101與continuation PASS都保留。只有相同path／SHA獲管理員正式允許並完成source/hash preflight後，才依原計畫重跑適用Windows驗收，不搬移或改造候選繞過政策 |
| platform author | 六語各兩專案native MCP harness、明確host profiles與專屬證據 | 保留Go46／Python60／Ruff18及混合首輪失敗；最終十二專案矩陣目前BLOCKED。C#仍需完整MCP symbols／refs／rename CAS／rollback驗收；未廣告capability保留UNAVAILABLE，Java／TS未版本診斷不升current；私有Python副本每次從可信來源新建並核對hash |
| independent reviewer | immutable source/diff與review證據 | 最終combined source、native profile與來源／binary hash須正確綁定；歷史finding與更正保留，作者不自我核准，不用舊候選PASS取代新候選審查 |
| packaging author | 當輪Windows／Linux打包腳本、installer／runtime收據 | Windows780的22項PASS保留原binary綁定，最終Windows installer維持BLOCKED。Linux69e769b本機VM建置IN_PROGRESS；完成原生測試、封裝與生命週期驗證後交root更新ODIN-029，macOS及未測矩陣不升PASS |
| root release | 最終artifact manifest、公開revision與發布紀錄 | 區分歷史公開版本、780 installer與本輪最終binary；依適用review、runtime及平台收據決定發布範圍，明示BLOCKED／NOT_RUN與未簽章等限制 |

每切片唯一owner、明確write scope；共有檔案序列整合，不覆寫衝突取得綠燈。歷史報告、command／transcript／normalization與原始driver快照不改寫，離線index PASS也不增加任何backend執行次數。Linux／macOS完整原生驗收、全語言／IDE私有等價、完整installer升級／故障矩陣及benchmark仍未完成；只有全部原acceptance具適用實證才升整項PASS。

恢復Windows最終驗收時使用 [finalrun-plan](../verification/language-sandbox-v3-finalrun-plan.json) 的同path／SHA與固定profile，產生新的實跑收據；等待時間、管理員處理意向或已有歷史PASS都不能當作政策已變更或最終驗收已通過。這份看板不執行或重新授權任何候選。
