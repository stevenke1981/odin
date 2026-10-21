# 語言、IDE 能力與除錯規格 (RFC-INTEL-001)

**地位：原驗收目標加上本輪已實作範圍；不是40項全部完成公告。** 對應 ODIN-007/008/010/013/017/018/019/034/035/036/039。本輪共85工具契約；歷史72工具的144 tests/1 ignored維持原source/binary範圍，不能替代本輪final gates。

## 直接標準協定與後端分級

Odin 使用自己的 Rust LSP/DAP client，不啟動 Serena/JetBrains bridge，不自動fallback。Host 明確指定 executable、完整argv、runtime與政策；不把儲存庫檔案當可執行host設定。設定值是資料，不提供任意eval或server executeCommand。

`odin languages` 的28個profile是安裝／設定指引，**不是28種已安裝、已握手或已驗收的語言**。逐能力分開記錄：

| 層級 | 所需證據 | 能證明的範圍 |
|---|---|---|
| registry/guidance | 內建profile及安裝說明 | 可用來設定，無runtime主張 |
| INSTALLED | 真program/version/hash探查 | 執行檔存在與版本，不等於LSP可用 |
| CONFIGURED | 真initialize/capabilities/workspace log | 該次握手；未知readiness保持unknown |
| VERIFIED | 指定OS/server/source/binary、真專案逐功能正反例 | 只有實際執行的能力；fixture另標，不與真語言合併 |

語言adapter完整目標仍包括detect、resolve/version/provenance、workspace root、initialize、index readiness、health/restart/invalidate與能力矩陣；registry與通用傳輸不代表每項語言專用adapter都已完成。先完成Rust/Python/TypeScript，再擴Rust/Python/TypeScript/Go/Java/C#六語各兩專案；40+語言不得由名字數推論。

## IDE工具的現行契約

| 工具 | 行為與邊界 |
|---|---|
| completion / completion_resolve | LSP completion list/array與resolve；hash/version/freshness需吻合，command與建議edit保持惰性資料 |
| hover / signature_help | 真server資料、UTF-16位置與來源新鮮度；Markdown是不可信內容 |
| workspace_symbols | 明確language與project scope、受檢來源快照、limit/truncation；不讀取任意外部workspace |
| call_hierarchy | 真prepare/incoming/outgoing；range與來源核對；整體請求、時間及累計輸出必須有界 |
| format_document | document/range formatting只轉成expected-hash Transactions preview，需allow_write；apply另呼叫 |
| inlay_hints / inlay_hint_resolve | bounded結果與source hash；label commands/text edits不執行 |
| semantic_tokens | actual legend與UTF-16/full/range capability；缺能力明確拒絕 |

Document工具在RPC前後核對來源及同步version。外部改寫回CONFLICT，不將舊range套到新來源。`freshness:current`只表示來源相符，不證明server索引完整；`index_complete_proven`不得自動升true。缺capability、stale、pending、truncated、unlocated或unknown version不得回空清單假稱clean。

所有格式化、CodeAction、Move/Inline與inspection fix都先進odin-core Transactions；沒有regex語意替代、server command執行或隱含寫檔。Resource edits仍限regular UTF-8 files與既有父目錄，拒絕binary/directory/recursive/overwrite/需確認annotation。Move/Inline由backend提供對應edit；完整語言語意、編譯與rollback目標保留。

## Native inspections與safe fix

已新增Clippy/Ruff/ESLint輸出adapter及native tools。必須同時有host `allow_inspections`、固定analyzer/version argv profile與**實際native sandbox**；allow_shell不是inspection授權。沒有sandbox或無法證明版本／分析程序使用sandbox就拒絕，沒有unsandboxed重試。

來源是readonly grant。Writable cache/temp不能與任何configured source root或Odin state重疊；toolchain、dependency與host config另由host授read grants。Clippy需`--locked`並將build output放授權外部cache；Ruff需`--no-fix --no-cache`；ESLint停用repository executable config discovery，顯式可執行config必須為canonical的host外部路徑。Version argv同樣拒絕直接寫入flags。Clippy build scripts/proc macros與host選定plugins仍是sandbox內的可執行程式，不是純資料解析。

Findings正規化provider/source/rule_id/path/range/severity/message/version/source sha256/freshness/fix_available；獨立analyzer沒有LSP document version，標null並以hash識別。Clippy UTF-8 byte、Ruff Unicode scalar、ESLint UTF-16需轉換且保留BOM/CRLF，禁止切開字元。Symbol summary依numeric lsp_severity重算並保留unknown；不把pending/stale/unverified升為clean。

Snapshot、count、raw/normalized output與累計fix長度均有配額。版本未知／不符、source changed、異常exit、timeout、malformed/truncated、範圍不可定位或Cargo未完成使報告incomplete。`scope:host_fixed_profile`、`whole_project_verified:false`明列；`summary.complete`不是全專案規則覆蓋保證。LSP與native同缺陷去重及全provider規則矩陣仍是原驗收目標。

`prepare_inspection_fix`重跑同host profile、比對finding_id與caller expected hash；只接受Clippy MachineApplicable、Ruff safe或ESLint direct fix的單檔、非重疊、有界edits。不採用suggestions/cross-file fixes，也不以analyzer fixing flag修改來源。Preview不寫來源，apply再CAS，rollback不覆寫第三方修改。

## DAP生命週期與immutable host payload

狀態為created→initialized→launching/attaching→configured→running↔stopped→terminated；frame/scopes/variables/evaluate遵守狀態/capability/session。Launch/attach/restart回ticket，送出不等於完成；timeout/cancel可標outcome unknown，不自動重試mutation。停止要disconnect並回收process tree。

可選host `debug_launches.<adapter-profile>`固定完整launch/attach/restart JSON payload。Caller只可傳空或相同資料，不能增加env/argv；同一helper適用single/batch。這是不可變payload政策，不是每個adapter的完整語意schema。

**未配置此profile時**，host明確的allow_debug仍是privileged debugging，回`unrestricted_host_debug`；evaluate也有執行target的權限。OS隔離另由host選用MXC與grants決定。Immutable payload不等於OS沙箱，也不宣稱JetBrains private debug REPL/Python語法相容。

## 本輪證據與未驗收

- Windows MXC author兩個native tests已有PASS；guard-until-reap已由當輪native三測試實跑PASS並有持久log/exit0。詳見 [OS_SANDBOX.md](../OS_SANDBOX.md)。
- Protocol29 unit+22 fixtures、engine先前74測試成功另存；最新engine rerun為62通過、12因AppControl未執行。Symbol severity/status修正有一個targeted PASS。它們不是真六語證據。
- revision30a957a的真Odin MCP→MXC→Clippy/Ruff/ESLint有30 checks PASS，涵蓋readonly analysis、rule/range/hash/version、CAS preview/apply/rollback與scratch清理；只適用該binary，早於hierarchy review修復，不是final gate。
- 六語各兩專案的首輪candidate在Odin啟動前被Application Control 4551阻擋，實際MCP/LSP checks=0。Version probe與fixture生成成功不得升為語言支援。
- 本輪fe5996d已完成17gates、191tests/4ignored，另native3實跑PASS；hierarchy修復後的新head獨立review、六語矩陣/final analyzer replay與跨平台仍待完成。不得改host安全政策、移動被拒的相同binary、放寬grants或加fallback取得PASS。

實際source/report pointers位於 [feature-evidence.json](../../verification/feature-evidence.json) 的extension_evidence；歷史checks/digests不改寫。完整行為見 [IDE_IMPLEMENTATION.md](../IDE_IMPLEMENTATION.md)。原acceptance與成功／失敗案例仍按 [FEATURE_SPEC.md](../FEATURE_SPEC.md) 驗收。
