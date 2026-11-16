# Odin 0.2.0 功能規格與逐項驗收

**現行85工具契約；40需求為39 PARTIAL、1 NOT_RUN。** 原58工具保留，前版72再新增13個IDE/inspection契約。原DoD不降低，也不因通用gate成功改成全需求接受。

本輪產品來源為 `0fe03ba30da9600c25588d506d7e094df89cf56f`。Linux 本機 Debian 12 VM 的完整 workspace 為211 PASS／0 FAIL／3 ignored，fmt、嚴格 Clippy、release 與 CLI／MCP／HTTP／取消檢查通過；同一 ELF 的 `.deb`／tar 已實跑安裝、重裝、移除。首輪文件來源a839999的功能驗證保留；補入Rust標準庫／native CRT notices的新版另綁當輪報告的packaging_revision，同一ELF不重新編譯。Windows 保留69來源的83a81d候選，metadata遭AppControl4551阻擋，最終六語矩陣仍NOT_RUN。這些結果不代表Linux OS沙箱、macOS或完整IDE等價。下列c5084f4與extension_evidence記錄維持其歷史scope；最新逐條件狀態見language_installers_v3及 [Linux實跑報告](../verification/package-v3-linux-notices-final.json)。

歷史clean product revision `c5084f4947fc4a07c9ab381fa9998ba1a1e2246d` 已執行 **17/17 gates exit0、200 workspace tests PASS/6 ignored**；其中3個native測試另外opt-in實跑PASS，其餘3個真LS probe仍ignored。source_changed=[]、tracked_source_dirty=false。Release binary SHA-256為 `f7e57a5a25229426b62be622770ac2d6b8f7cfd3fcbf47dbf25254aa739eb875`。

同一binary的原三LS/debugpy29 checks及真MCP→nativeMXC→Clippy/Ruff/ESLint30 checks均PASS；analyzer完整command另有exit0/log/digest。六語各兩專案的host-mode baseline已實際執行：**PARTIAL、exit 2，260 PASS／14 UNAVAILABLE／12 NOT_VERIFIED／0 FAIL，12/12 initialized**。TypeScript與Java診斷缺document version；TypeScript native MXC profile04已實跑PARTIAL（42 PASS／2 UNAVAILABLE／6 NOT_VERIFIED／0 FAIL，exit2）。28 registry entries只是設定指引，不能推論28或40+語言通過。

前版72工具/144-test、fe5996d/191-test、author切片及AppControl失敗均保留原source/binary/digest，與新結果分層。六語首輪0交換仍是歷史失敗；本次真rename/apply/rollback涵蓋JDTLS兩專案。17gates保留當時QA driver快照；六語新driver `d47fa6daa7c96aa8d5bc7b7c2603755b4047c4e3ed0c3d740b358f9e931a6e09` 將initial-valid觀察前移，root/reviewer確認AST驗收條件未變；53個production crates來源hash仍與release report相符。Hierarchy安全預算與summary已修復；最新review/source binding由root/reviewer管理。

需求來源：[features.json](../requirements/features.json)；實際report/command/log/hash與未驗收條件：[feature-evidence.json](../verification/feature-evidence.json) 的extension_evidence。Serena只作開發工具；產品無bridge/fallback、不以regex冒充語意，不自動授權host政策。

<a id="odin-001"></a>

## ODIN-001 — 可重現的 Rust 建置與工具鏈

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** build, release

**目標：** 從只有靜態檢查的 0.2.0 到可執行產品。

**設計目標：** 固定 rust-toolchain 與 Cargo.lock；解決 rmcp SDK 的實際型別/feature 相容；所有 crate 必須能獨立及 workspace 編譯；建置工件標記 Git SHA。

**已觀察範圍：** Linux0fe03ba已實跑locked workspace211 PASS／0 FAIL／3 ignored、fmt、嚴格Clippy與clean release metadata。Windows69編譯exit0但metadata被4551阻擋；歷史c5084f4的17/17 gates與200 tests保留原scope，不能轉移為Windows最終驗收。

**原驗收條件：**

1. 在全新工作樹執行 cargo generate-lockfile、cargo build --workspace --locked 成功且 Cargo.lock 納入 Git。
2. cargo fmt --all -- --check、cargo clippy --workspace --all-targets --locked -- -D warnings、cargo test --workspace --locked 均退出 0。
3. Release binary 的 --version 與 commit metadata 可讀，沒有 false PASS 或預製空 binary。

**尚未驗收／完成：**

- A1：Linux0fe03ba已在核對來源的VM checkout實跑locked build/metadata；完整平台矩陣、macOS及最終Windows runtime仍未完成。

**測試案例 ID：** M0-compile-linux, M0-compile-win-msvc, M0-compile-macos。 **前置需求：** 無。

**證據：** `verification/feature-evidence.json#ODIN-001`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-002"></a>

## ODIN-002 — 標準 MCP stdio 與 Streamable HTTP

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-agent, mcp

**目標：** 讓現有 Coding Agent 不使用 Serena 即能連線。

**設計目標：** 使用官方 Rust MCP SDK；JSON-RPC Initialize / initialized / tools/list / tools/call；stdio 僅允許 JSON-RPC stdout；HTTP loopback + Bearer + Host/Origin；失敗使用明確 tool error。

**已觀察範圍：** c5084f4標準binary17gates與same-binaryanalyzer30checks PASS；六語兩專案baseline已實跑PARTIAL/exit2（260/14/12/0），TS native sandbox04已實跑PARTIAL（42 PASS／2 UNAVAILABLE／6 NOT_VERIFIED／0 FAIL，exit2）。

**原驗收條件：**

1. 官方 MCP 測試客戶端完成初始化、列舉所有宣告工具、成功與拒絕呼叫。
2. stdio 的 cargo/run logging 永不寫入 stdout；stderr logging 不污染 frame。
3. HTTP 無 token、錯誤 token、惡意 Origin/Host 均被拒絕；預設只綁 127.0.0.1。

**尚未驗收／完成：**

- A1：使用專案測試 client，尚未完成官方 MCP client/conformance suite。
- A2：Windows smoke 檢查 frame；完整 disconnect/logging 壓力矩陣未驗收。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M0-mcp-smoke, M0-http-auth-negative, M0-mcp-disconnect。 **前置需求：** ODIN-001。

**證據：** `verification/feature-evidence.json#ODIN-002`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-003"></a>

## ODIN-003 — 85 個原生工具契約、版本與能力協商

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-engine, schemas

**目標：** 避免有 tool 名字但工具不支援、或參數不一致。

**設計目標：** 單一來源 schema → CLI/MCP/flow；工具包含 required project、輸入限制、read/write/execute 標記；新增 get_capabilities 與 schema_version；不支援回 CAPABILITY_UNAVAILABLE。

**已觀察範圍：** 原 58、前版 72、本輪 85 契約；新增 13 個 IDE/inspection tools 已有 catalog/policy/dispatch/tests/docs 切片，完整真後端矩陣未驗收。

**原驗收條件：**

1. 原有 58 個工具保留，擴充後 85 個 tools 全數列在 coverage 表，schema/dispatcher 逐項有功能測試。
2. 錯參數、未知專案、未知工具與後端失聯各得到不同可機械判別 error code。
3. 新版本修改破壞 API 時更新 schema_version 並保留遷移文件。

**尚未驗收／完成：**

- A1：85 catalog 有切片一致性證據；每個新 API 的真後端正反例及完整工具矩陣未完成。
- A2：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A3：schema version 已提供；破壞式升版及遷移相容矩陣尚未實際升版驗收。

**測試案例 ID：** M0-contract-schema, M0-error-mapping, M0-capabilities。 **前置需求：** ODIN-002。

**證據：** `verification/feature-evidence.json#ODIN-003`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-004"></a>

## ODIN-004 — 每次呼叫專案隔離與生命週期

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-core, odin-engine

**目標：** 杜絕不同 Agent 的 active project 互相踩踏。

**設計目標：** 請求明確 project_id；路徑正規化、重疊 root 拒絕；每個專案獨立 LSP/DAP/SQLite/journal 與結果儲存；熱切換須顯式受控。

**已觀察範圍：** 每次 project scope、跨 project 唯讀查詢、後端/session 與結果隔離已有定向測試。

**原驗收條件：**

1. 兩個 Agent 同時對 A、B 專案讀寫，不會交換記憶、結果或語言伺服器。
2. 未知 project_id 和 workspace 外部的任何路徑一律拒絕。
3. 停止 project 能停止其子程序、釋放鎖和資源，其他 project 繼續服務。

**尚未驗收／完成：**

- A1：已有隔離負例；完整雙 Agent 多 project 同時讀寫壓力矩陣未驗收。
- A2：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A3：LSP/DAP 可明確停止；停止整個 project 並驗證其他 project 持續服務的完整生命週期未驗收。

**測試案例 ID：** M0-project-concurrency, M0-cross-scope-negative, M0-process-shutdown。 **前置需求：** ODIN-001。

**證據：** `verification/feature-evidence.json#ODIN-004`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-005"></a>

## ODIN-005 — 跨平台路徑與檔案安全

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-core, paths

**目標：** 解決 symlink/junction、目錄競態與非法路徑造成的越權。

**設計目標：** canonicalize 與 handle-relative open；Windows reparse point/junction、UNC、drive prefix、UTF-16；Linux/macOS symlink、hardlink；已寫入檔案不可被替換成不可信目標。

**已觀察範圍：** 已使用 no-follow directory capabilities，Windows aliases/reparse/hardlink 有原生回歸紀錄。

**原驗收條件：**

1. 包含相對 ..、絕對路徑、symlink/junction escape、Unicode NFC/NFD、大小寫差異的測試全部拒絕或按安全策略處理。
2. 攻擊測試在 authorize 與 open 之間交換 symlink，無 workspace 外部檔案被修改。
3. Windows 長路徑和 CRLF 的讀取/修改 roundtrip 字節一致。

**尚未驗收／完成：**

- A1：Unicode NFC/NFD、完整 UNC/long-path 及所有 OS 路徑矩陣尚未驗收。
- A2：最後 CAS 檢查與替換仍有外部寫入競態；不宣稱 OS 全域原子 CAS。
- A3：CRLF/UTF-16 有測試；完整 Windows 長路徑 byte-roundtrip 矩陣未驗收。

**測試案例 ID：** M0-path-traversal, M0-toctou-race, M0-windows-reparse。 **前置需求：** ODIN-004。

**證據：** `verification/feature-evidence.json#ODIN-005`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-006"></a>

## ODIN-006 — 預覽、提交與 crash-safe 修改交易

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-core, odin-engine

**目標：** 讓 Agent 修改可以審核、不中途破壞原檔。

**設計目標：** read hash → immutable draft → preview → CAS preflight → apply/journal fsync；單檔 atomic replace；中斷 explicit recover；對於多檔無瞬間原子性寫清楚。

**已觀察範圍：** Transactions 提供 immutable preview、全檔 CAS、journal、recover/rollback；外部衝突拒絕已有回歸紀錄。

**原驗收條件：**

1. 外部先行修改檔案時，apply 回 CONFLICT，外部修改保持不變。
2. 在每個 journal phase 模擬 kill/crash 後可安全判定完成、恢復或需要人工介入。
3. rollback 永不覆蓋未知第三方資料；同 request_id 不同 payload 被拒絕。

**尚未驗收／完成：**

- A1：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A2：真 process crash checkpoint 紀錄已存在；磁碟故障／斷電耐久性與全部平台仍未驗收。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M0-transaction-crash-matrix, M0-cas-race, M0-rollback-conflict。 **前置需求：** ODIN-005。

**證據：** `verification/feature-evidence.json#ODIN-006`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-007"></a>

## ODIN-007 — LSP 初始化、同步、關閉及依賴管理

**Priority:** P0 · **Milestone:** M1 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-protocol, lsp

**目標：** 直接對接語言伺服器，無須 Serena LSP。

**設計目標：** 初始化 initialize/initialized、capabilities negotiation、document didOpen/didChange/didClose、WorkspaceFolder；單調版本、UTF-16 索引、超時/取消/重啟、子程序 stderr 分離、按語言設定 stdout framing。

**已觀察範圍：** 新增IDE RPC/URI/JDTLS安全處理已實作；六語12/12真初始化、260PASS，14capability UNAVAILABLE及12診斷NOT_VERIFIED仍明列，TS沙箱profile04另有兩專案42 PASS的PARTIAL證據。

**原驗收條件：**

1. rust-analyzer 真實專案能索引兩個跨檔引用，檔案改動後讀到新符號。
2. UTF-16 surrogate pair、CRLF、非 ASCII 路徑映射無 off-by-one，版本單調。
3. 語言伺服器崩潰/掛起後超時明確，重啟並重新同步所有開啟的文件。

**尚未驗收／完成：**

- A1：六語12專案已實際initialize/查詢；14缺失capabilities與12無version診斷仍未全驗收，不再是0check/僅fixture。
- A2：UTF-16/CRLF及真六語source/range已測；全部非ASCII路徑/server sync模式與跨平台矩陣未完整驗收。
- A3：protocol crash/timeout/restart 有 fixture；各真實 server 的崩潰重同步矩陣未驗收。

**測試案例 ID：** M1-rust-analyzer-live, M1-utf16-cases, M1-lsp-restart。 **前置需求：** ODIN-004。

**證據：** `verification/feature-evidence.json#ODIN-007`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-008"></a>

## ODIN-008 — 完整符號查找與引用關係

**Priority:** P0 · **Milestone:** M1 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-protocol, odin-engine

**目標：** 符號查詢不能被 grep 冒充。

**設計目標：** 實作 documentSymbol/workspaceSymbol/definition/declaration/references/implementation；符號 name_path、kind、scope、depth、include_body、include/exclude kind、matches 分頁；限定只讀必要檔案。

**已觀察範圍：** 真六語兩專案baseline涵蓋symbols/references/IDE與rename流程並有transcript；整體PARTIAL/exit2，不將unsupported或未version診斷當全語等價。

**原驗收條件：**

1. 跨檔 Rust fixtures 逐項對照 rust-analyzer 輸出定位、kind、range，至少 20 個真實查詢通過。
2. 同名多符號可消歧；使用符號路徑與範圍，不因檔案中註解包含同字就命中。
3. 不可用功能需回 CAPABILITY_UNAVAILABLE；不默默降級為純文字結果。

**尚未驗收／完成：**

- A1：已有六語260PASS真查詢與transcript；原定20個Rust逐項對照及全部kind/range/disambiguation反例仍未單獨完整驗收。
- A2：消歧與 filters 有 protocol fixtures；完整真實同名/註解反例矩陣未驗收。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M1-symbol-20-cases, M1-symbol-collision, M1-capability-negative。 **前置需求：** ODIN-007。

**證據：** `verification/feature-evidence.json#ODIN-008`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-009"></a>

## ODIN-009 — WorkspaceEdit 完整支援

**Priority:** P0 · **Milestone:** M1 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-core, odin-protocol

**目標：** rename/refactoring 遇到 LSP resource operations 時不可失敗或誤成功。

**設計目標：** 完整處理 TextDocumentEdit、CreateFile、RenameFile、DeleteFile、版本限制與 changeAnnotations；對操作順序建 DAG；跨磁碟與 case-only rename 特別路徑；prepare/apply 共用 journal。

**已觀察範圍：** format_document/range formatting 的後端 edit 轉 expected-hash Transactions preview；有序 WorkspaceEdit/CAS 保留且不隱含 apply。

**原驗收條件：**

1. 跨檔 symbol rename 涵蓋 source edits + RenameFile，一次 preview 顯示全部變更。
2. Windows case-only rename、collision、無權限刪除、磁碟中斷在預定語意下安全停止/復原。
3. 遇到不支援 annotation 或檔案版本不符全部拒絕，不遺漏操作後回成功。

**尚未驗收／完成：**

- A1：protocol ordered resource edits 已測；真實 backend source+RenameFile 的重構整合案例未驗收。
- A2：case-only/collision 有回歸；權限拒絕、跨磁碟、磁碟中斷等矩陣未全驗收。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M1-resource-ops-fixture, M1-win-case-rename, M1-journal-fileop-crash。 **前置需求：** ODIN-006, ODIN-007。

**證據：** `verification/feature-evidence.json#ODIN-009`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-010"></a>

## ODIN-010 — 真實語言伺服器三語驗收

**Priority:** P0 · **Milestone:** M1 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** test, lsp

**目標：** 驗證 Odin 功能是真正可用而不是 mock-only。

**設計目標：** 首批 Rust rust-analyzer、Python basedpyright/Pyright、TypeScript typescript-language-server；固定 fixtures、backend version、安裝指令、工作區設定；公開能力矩陣。

**已觀察範圍：** 同f7e57a5 binary六語各兩專案已實跑：260PASS/14UNAVAILABLE/12NOT_VERIFIED/0FAIL，12初始化；TS/Java診斷缺document version，完整原驗收仍PARTIAL。

**原驗收條件：**

1. 每語言至少完成符號、跨檔 references、rename、diagnostics 的成功與失敗案例。
2. 每個語言都記錄版本、OS、commit、命令行與 stdout/stderr 原始 log。
3. 未驗證語言不能以「40+ 已支援」列為 PASS。

**尚未驗收／完成：**

- A1：六語baseline已PARTIAL實跑；TS/Java valid/error/recovered各6項診斷因缺document version保持NOT_VERIFIED，14capabilities unavailable，不推論完整原功能成功。
- A2：六語command exit2、binary/harness/transcript/backend/log與normalization hashes已具備；完整診斷freshness驗證及平台/版本矩陣仍未完成。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M1-rust-language, M1-python-language, M1-typescript-language。 **前置需求：** ODIN-007, ODIN-008, ODIN-009。

**證據：** `verification/feature-evidence.json#ODIN-010`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-011"></a>

## ODIN-011 — 零 Serena 相依的乾淨主機驗證

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** build, security

**目標：** 產品必須真正獨立，不是偷偷代理 Serena。

**設計目標：** CI/fixture 禁止 Serena / SolidLSP package、程式自動 fallback、子程序工具轉送；安裝套件與 Cargo dependency graph 也要檢查。

**已觀察範圍：** 真 binary MCP 顯示 standalone 並拒絕 serena_tools/serena_call；生產路徑沒有 bridge/fallback。

**原驗收條件：**

1. 乾淨隔離環境不安裝 Python/Serena 仍能完成核心檔案/MCP 功能。
2. 二進位與 lock/dependency tree 不含 Serena/SolidLSP；不存在 serena_tools/serena_call fallback。
3. 缺少 LSP/DAP 時明確提示必要後端，不會執行下載 Serena。

**尚未驗收／完成：**

- A1：目前主機供開發使用而安裝 Python/Serena；無這些工具的乾淨隔離主機驗收未執行。
- A2：lock/dependency/原碼掃描已有報告；最終 binary/dependency artifact 的獨立掃描仍待發佈整合。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M0-standalone-no-serena, M0-sbom-audit, M0-backend-missing。 **前置需求：** ODIN-001。

**證據：** `verification/feature-evidence.json#ODIN-011`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-012"></a>

## ODIN-012 — 實際 OS 隔離、程序樹與權限

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** security, process

**目標：** 拒絕把 read_only flag 當作沙箱。

**設計目標：** 預設最小權限執行；Windows Job Objects/restricted token 視 profile；Linux seccomp/namespace or container；macOS sandbox profile；限制子程序後代、resource/time/cpu/memory；執行命令僅 allowlisted profile。

**已觀察範圍：** Windows native3與same-binaryanalyzer30checks PASS；六語baseline為host mode，TS MXC profile01缺私有TEMP失敗、explicit固定bootstrap profile04已實跑PARTIAL（42 PASS／2 UNAVAILABLE／6 NOT_VERIFIED／0 FAIL），跨平台未完成。

**原驗收條件：**

1. 受測敵意 command/process 不能更改 workspace 外檔案；子孫程序可被完整終止。
2. 限制模式若該 OS 不支援，必須 fail closed 且明確出錯，不能靜默解除限制。
3. Shell、DAP launch、LSP executable 的信任邊界文件及 deny/allow tests 齊備。

**尚未驗收／完成：**

- A1：nativeMXC與analyzer已PASS；六語host-baseline不等於sandbox。TS profile01因privateTEMP未建立而FAIL，profile04已實跑PARTIAL（42 PASS／2 UNAVAILABLE／6 NOT_VERIFIED／0 FAIL）；其他OS及全資源矩陣未完成。
- A2：MXC 已 fail closed/無 fallback；unsupported OS/full profile failure matrix 未驗收，未配置的程序沒有自動沙箱。
- A3：native3與實際analyzer命令/log已綁定；其餘LSP/DAP完整矩陣及最新combined review由root再核對。

**測試案例 ID：** M0-windows-job-objects, M0-linux-sandbox, M0-macos-sandbox。 **前置需求：** ODIN-005。

**證據：** `verification/feature-evidence.json#ODIN-012`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-013"></a>

## ODIN-013 — 通用 DAP 生命週期與除錯行為

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-protocol, dap

**目標：** 讓除錯不用 JetBrains 特定 plugin。

**設計目標：** 完整 initialize/launch/attach/configurationDone、setBreakpoints、stack/scopes/variables/evaluate、step/continue/disconnect；支持 capability negotiation、events、cancel、timeouts。

**已觀察範圍：** LSP/DAP 可走 host MXC；選配 immutable debug_launches 固定 payload，未設 profile 時 allow_debug 仍為 explicit privileged mode。

**原驗收條件：**

1. 至少 Windows 與 Linux 在真實 debug adapter 啟動測試程式並命中指定 breakpoint。
2. continue/step、scopes/variables/evaluate 得到可核對的變數值，退出後無孤兒程序。
3. 不支援的 DAP 指令明確回 unsupported，不能把送出 request 當成功完成操作。

**尚未驗收／完成：**

- A1：Linux 真實 adapter breakpoint 仍 NOT_RUN。
- A2：Windows debugpy 有功能結果；各 adapter/OS 的 debuggee process-tree 清除矩陣尚未完成。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M2-dap-real-debuggee-win, M2-dap-real-debuggee-linux, M2-dap-timeout。 **前置需求：** ODIN-012。

**證據：** `verification/feature-evidence.json#ODIN-013`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-014"></a>

## ODIN-014 — 語意 Move refactoring

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** refactor, odin-protocol

**目標：** 搬移 symbol/file/dir 後關聯檔案仍可編譯。

**設計目標：** 優先使用 LSP willRenameFiles / CodeAction + WorkspaceEdit；不足則實作受語言限定 adapter（例如 rust-analyzer）；禁用 regex 字串搬移冒充；保留 import/visibility/namespace。

**已觀察範圍：** move_symbol 僅接受後端 refactor.move edit，驗證 source/target 並回 transaction preview；沒有文字搬移 fallback。

**原驗收條件：**

1. 跨模組移動公開符號後，所有 consumers 更新且 cargo test 通過。
2. 同名、同 module path、符號解析失敗時拒絕修改並輸出 unsupported reason。
3. 受影響的檔案、差異、引用更新及 rollback 全可檢查。

**尚未驗收／完成：**

- A1：目前成功路徑是 protocol fixture，沒有真語言 backend 跨模組 Move 後 cargo test 的驗收。
- A2：同名/module/解析失敗有拒絕機制；語言專用 import/visibility/consumer 反例未全驗收。
- A3：preview/交易已有測試；真實語意 Move 的引用更新與 rollback 尚未驗收。

**測試案例 ID：** M2-rust-cross-module-move, M2-unsupported-language-move, M2-move-rollback。 **前置需求：** ODIN-009, ODIN-008。

**證據：** `verification/feature-evidence.json#ODIN-014`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-015"></a>

## ODIN-015 — 語意 Inline refactoring

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** refactor, odin-protocol

**目標：** 將函式、變數 inline，不改變語義。

**設計目標：** 優先使用被支援的 LSP Code Action/rename；adapter 管理作用域、副作用、泛型與借用；不做任意字串展開；先提供 Rust/TypeScript 支援矩陣。

**已觀察範圍：** inline_symbol 僅接受具 refactor.inline kind 的後端 edit，拒絕 command-only/mixed/模糊 actions。

**原驗收條件：**

1. 對純函式與帶有副作用的 fixtures，替換前後測試結果一致。
2. 有巢狀 scope、名稱衝突、macro/external call 風險時拒絕自動重構。
3. 每一個 inline 能 preview 並可撤銷，不會產生編譯不過的 silent success。

**尚未驗收／完成：**

- A1：沒有真實語言純函式與副作用 fixtures 的重構前後測試等價證據。
- A2：未完成語言專用 scope/macro/泛型/借用與名稱衝突分析矩陣。
- A3：交易 preview 路徑已測；真實 backend Inline 的編譯與 rollback 尚未驗收。

**測試案例 ID：** M2-inline-pure, M2-inline-sideeffect, M2-inline-negative。 **前置需求：** ODIN-009。

**證據：** `verification/feature-evidence.json#ODIN-015`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-016"></a>

## ODIN-016 — Safe Delete 與級聯刪除

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** refactor

**目標：** 不讓只找不到靜態 refs 的刪除毀壞動態使用者。

**設計目標：** LSP static refs + workspace-wide dependency reachability，公開 API/反射/macro 風險拒絕或要求人工批准；遞迴移除確定不用的引用；關連刪除一同 preview。

**已觀察範圍：** Safe Delete 仍限 LSP 靜態 references 與 guarded transaction，沒有完整 reachability/cascade。

**原驗收條件：**

1. 跨檔仍在引用的 symbol 刪除先拒絕；可安全刪除的 private symbol 可編譯。
2. 動態 dispatch/reflection/宏造成不確定時回 NEED_REVIEW，不能聲稱安全。
3. safe delete 造成影響的檔案與 tests 資料可逐項對照。

**尚未驗收／完成：**

- A1：跨檔引用拒絕有原碼路徑；真實可刪 private symbol 的編譯案例未驗收。
- A2：公開 API、反射、macro/動態 dispatch 尚無完整 NEED_REVIEW 語言規則。
- A3：未完成級聯刪除與影響測試對照。

**測試案例 ID：** M2-safe-delete-references, M2-safe-delete-dynamic, M2-cascade-delete。 **前置需求：** ODIN-008, ODIN-009。

**證據：** `verification/feature-evidence.json#ODIN-016`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-017"></a>

## ODIN-017 — 診斷／Linter／IDE inspection 正規化

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** diagnostics

**目標：** 不把 LSP diagnostics、Build failure 與 IDE inspection 混成一件事。

**設計目標：** 統一 diagnostics schema: source、severity、range、rule、fix、observed_at、document_version、confidence；LSP pull/push，Cargo clippy/rustfmt，ESLint/TypeScript，Python Ruff；變成獨立 plugin provider。

**已觀察範圍：** Clippy/Ruff/ESLint provider與safe CAS已實作；c5084f4同binary30 checks/實際harness exit0，normalized rule/range/hash/version、readonly、preview/apply/rollback與scratch清理通過，完整去重/規則矩陣未完成。

**原驗收條件：**

1. 同一缺陷從 LSP 與 clippy 匯入可去重但保留來源。
2. 不存在的 rule/analysis 或未就緒結果回 PENDING/UNAVAILABLE，不回空 PASS。
3. quick fix 必須先預覽 WorkspaceEdit，再交由交易引擎執行。

**尚未驗收／完成：**

- A1：當輪三provider真分析/修復已PASS；LSP+native同缺陷去重與所有規則覆蓋未驗收。
- A2：normalization/版本/不完整報告有 unit fixtures；真 analyzer 版本及全部錯誤路徑未驗收。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M2-diagnostic-dedup, M2-inspection-cargo-clippy, M2-diagnostic-pending。 **前置需求：** ODIN-007, ODIN-006。

**證據：** `verification/feature-evidence.json#ODIN-017`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-018"></a>

## ODIN-018 — Type Hierarchy、外部依賴與跨專案查詢

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-protocol, index

**目標：** 完整理解跨 module/class 的繼承與依賴。

**設計目標：** TypeHierarchy prepare/supertypes/subtypes；reference in dependencies；受控外部 workspace activation 與 read-only Project Server；支持 resolver 快取失效。

**已觀察範圍：** Type/CallHierarchy與workspace symbols/freshness已實作並有資源預算修復；六語已實測但保留14capability UNAVAILABLE，完整跨專案/依賴矩陣未完成。

**原驗收條件：**

1. Type Hierarchy 能列出 fixture 的父類、子類，並正確含跨檔。
2. 跨專案查詢必須指定兩方 project_id，外部檔案不可被寫入。
3. 不支援 hierarchy 的 LSP 返回 capability unavailable 並標明缺失。

**尚未驗收／完成：**

- A1：已完成六語baseline中的支援查詢；14capability unavailable仍明列，全部Type/CallHierarchy與外部依賴跨專案矩陣未完整驗收。
- A2：read-only project query 已測；external workspace activation/Project Server/resolver invalidation 未完整實作。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M2-hierarchy, M2-external-project, M2-scope-negative。 **前置需求：** ODIN-008。

**證據：** `verification/feature-evidence.json#ODIN-018`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-019"></a>

## ODIN-019 — 逐語言適配與 40+ 語言宣告策略

**Priority:** P1 · **Milestone:** M3 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** adapters, lsp

**目標：** 跨語言能力必須可測、可擴展、可宣告。

**設計目標：** 語言適配介面：detect、resolve_server、environment、workspace_root、index_trigger、symbol_capabilities、healthcheck；先 Rust/Python/TypeScript，再 Go/C#/Java/JavaScript；剩餘依需求逐語言完成；將 installed/configured/verified 分級。

**已觀察範圍：** 28是registry指引；六語兩專案已實跑PARTIAL/exit2、12初始化與260PASS，14UNAVAILABLE/12NOT_VERIFIED仍保留，不稱六語全功能或28/40+已支援。

**原驗收條件：**

1. 首批 6 語言可在本機安裝直接後端並通過核心查詢。
2. 每種新增語言均有 2 個以上實際專案 fixture 與反例；未驗證不得宣稱支援。
3. 支援矩陣記錄每種語言的 find/reference/rename/diagnostics/refactor 狀態與證據。

**尚未驗收／完成：**

- A1：六語各兩專案已實跑PARTIAL/exit2，12初始化；TS/Java診斷未version、14能力unavailable，不等於六語全功能完成。
- A2：兩fixture的rename/apply/rollback真實成功包含JDTLS；完整專案多樣性、全部反例/版本/平台仍未達全部驗收。
- A3：逐語言結果已列260PASS/14UNAVAILABLE/12NOT_VERIFIED/0FAIL；NOT_VERIFIED不可改PASS，28/40+語言及全refactor/IDE矩陣未完成。

**測試案例 ID：** M3-six-language-smoke, M3-language-matrix, M3-server-discovery。 **前置需求：** ODIN-010。

**證據：** `verification/feature-evidence.json#ODIN-019`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-020"></a>

## ODIN-020 — Markdown 記憶與引用完整性

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-core, memory

**目標：** 持久知識可版本控制、跨 session，無須向量資料庫。

**設計目標：** project-local + global 兩範圍；topics / mem: references、CAS、rename propagation、readonly/ignored patterns、integrity check、import/export；read minimal context rather than auto injecting all memories。

**已觀察範圍：** central Markdown CAS/refs、版本化 bundle 匯入匯出、scope move 與 initiating-project owned journals 已有測試。

**原驗收條件：**

1. 一組具有循環引用、壞連結和 read-only 的記憶可被列出/檢查，rename 不遺失引用。
2. 跨 session 重啟後內容與 hashes 保持；無權限範圍不得寫入。
3. 從 0.2 central state 可備份遷移至 repo-local，失敗可回復。

**尚未驗收／完成：**

- A1：ref/readonly/rename 已測；完整 ignored patterns 與 repo-local layout 未實作。
- A2：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A3：尚未提供 central→repo-local 的自動備份遷移與回復驗收。

**測試案例 ID：** M2-memory-rename, M2-memory-migration, M2-readonly-integrity。 **前置需求：** ODIN-006。

**證據：** `verification/feature-evidence.json#ODIN-020`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-021"></a>

## ODIN-021 — 增量索引與變更監聽

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-core, index

**目標：** 大型 repo 不能每次重建 SQLite FTS5。

**設計目標：** watcher（notify）+ full rescan fallback；持久 hash、ignore/glob、rename/delete/update；快取一致性和索引狀態查詢；全文索引與 LSP symbol cache 明確區分。

**已觀察範圍：** index_query 前 live-hash scan 會增量更新新增/修改/刪除 FTS rows；沒有背景 watcher。

**原驗收條件：**

1. 新增/修改/刪除檔案後不需手動 index_build 即更新搜尋。
2. watcher 漏事件、斷線或 inode/rename 變動會觸發可追蹤的 rescan。
3. 冷啟與暖啟結果一致；不把 stale hit 或缺資料回成正常完成。

**尚未驗收／完成：**

- A1：查詢前增量更新已測；查詢以外的背景即時更新沒有實作。
- A2：沒有 watcher event queue、掉事件偵測／斷線 rescan 的驗收。
- A3：定向一致性已測；大型冷暖啟動及完整漏事件矩陣未完成。

**測試案例 ID：** M2-watcher-events, M2-index-rebuild, M2-index-consistency。 **前置需求：** ODIN-005。

**證據：** `verification/feature-evidence.json#ODIN-021`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-022"></a>

## ODIN-022 — 安全批次流程與可重用查詢

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-engine, flow

**目標：** 提供比多次工具往返更可控的組合介面，不依賴 Python。

**設計目標：** 受限 JSON DAG + variables + filters + map/reduce builtins + JSON Pointer + bounded concurrency per session；無任意程式執行；trace 所有工具；超時/取消。

**已觀察範圍：** flow 已有 per-session namespace locks、有界 map/filter/reduce、失敗 draft 不提交與 timeout。

**原驗收條件：**

1. 多工具查詢只回投影資料，於指定固定 fixture 比逐步呼叫減少輸出 bytes。
2. forbid write/execute/unknown call 即使透過 flow 包裹也仍拒絕。
3. 兩個 session 分開併行，單 session 失敗不污染 state；循環 DAG 不可執行。

**尚未驗收／完成：**

- A1：輸出投影已測；固定 fixture 的 bytes 對比 benchmark 尚未發布。
- A2：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A3：session 隔離已有測試；完整 DAG scheduler、cycle validation 與 bounded DAG concurrency 未實作。

**測試案例 ID：** M2-flow-projection, M2-flow-isolation, M2-flow-deny。 **前置需求：** ODIN-003, ODIN-004。

**證據：** `verification/feature-evidence.json#ODIN-022`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-023"></a>

## ODIN-023 — CLI / Client setup / Onboarding

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-agent, cli

**目標：** 可一個指令安裝 Odin MCP 於 Codex、Claude Code、OpenCode、AGY、Gemini CLI、Cursor、VS Code 等七種客戶端（`setup --client`），並讓 client 啟動時自動載入（`serve --project-from-cwd`）。只建議使用者層級設定位置，不建議放入 repo 內的專案層級檔案。

**設計目標：** setup 子命令可輸出/寫入受控 MCP client config，先備份並 diff；init/doctor/index/validate；提示詞描述 Odin 支援範圍，不偽稱 Serena API。

**已觀察範圍：** languages registry、runtime/setup guidance 與 doctor sandbox probe 有來源；七種 client entry 的生成與 MCP 啟動 smoke 127/127（`scripts/client_setup_smoke.py`，不啟動 client 應用）；真實應用見 A1。

**原驗收條件：**

1. 三個客戶端都可從設定檔啟動並成功 tools/list。
2. 覆寫既有配置要顯式同意，保證原有未關聯設定不變。
3. doctor 能區分 Rust binary、MCP 啟動、LSP missing、檔案權限與 DAP missing。

**尚未驗收／完成：**

- A1：真實應用狀態（證據 `verification/client-apps-local.json`，Claude Code／Codex 綁定 `794371d` binary `fb288ff1…`，OpenCode／AGY 綁定 `0cc7d50` binary `e1820f38…`）：Codex CLI 0.163.0 與 Claude Code 2.1.306 載入並完成 `status` 工具呼叫，PASS；OpenCode 1.18.35 僅 `opencode mcp list` connected，工具呼叫 NOT_RUN（模型供應商錯誤）；AGY 1.4.0 載入 PASS，工具呼叫 NOT_RUN（headless 拒絕 MCP 權限）；Gemini CLI、Cursor 本機未安裝，VS Code 未測 agent MCP，皆 NOT_RUN。OpenCode／AGY 未於 `794371d` 重測。
- A2：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A3：doctor 的 LSP probe 已實作；MCP transport、檔案權限、DAP health 的全分類矩陣尚未驗收。

**測試案例 ID：** M2-codex-setup, M2-claude-setup, M2-opencode-setup。 **前置需求：** ODIN-002。

**證據：** `verification/feature-evidence.json#ODIN-023`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-024"></a>

## ODIN-024 — Context／Mode／Project 配置策略

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-core, config

**目標：** 讓不同 Agent 用不同工具集與風險策略。

**設計目標：** 層級 host → project → per-client context → per-task mode，覆蓋優先序可預期；版控 TOML、local overrides、驗證 schema、秘密只走 env 或 OS keychain；不追求 Serena YAML/Jinja 語法。

**已觀察範圍：** allow_inspections、sandbox、inspection/debug_launches 為 host 純資料政策；context/mode 只能收緊，不能自動授予權限。

**原驗收條件：**

1. 相同配置來源優先序有 deterministic tests；無效 key fail closed。
2. readonly mode 不能授予 host policy 拒絕的寫入；任意 mode 不能繞過 OS sandbox。
3. 配置 export redacts tokens/secrets，且升版兼容有 migration。

**尚未驗收／完成：**

- A1：目前是 host 宣告 context/mode；完整 repo-local override/versioned precedence/migration 尚未實作。
- A2：可選 Windows MXC 是獨立 host OS 決策；mode 不得建立 grants 或把未配置程序標成隔離。Linux/macOS 未完成。
- A3：redaction 有定向測試；跨版本 migration 尚未驗收。

**測試案例 ID：** M2-config-precedence, M2-config-readonly, M2-config-redaction。 **前置需求：** ODIN-004。

**證據：** `verification/feature-evidence.json#ODIN-024`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-025"></a>

## ODIN-025 — Dashboard / 操作日誌 / 使用量

**Priority:** P1 · **Milestone:** M3 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-agent, dashboard

**目標：** 提供可檢查工作實況，而不等於假定任務完成。

**設計目標：** 本機唯讀 dashboard：project/LS/DAP status、tool traces、latency、errors、index health、memory stats、resource usage；可匯出 JSONL/OpenTelemetry traces；禁止洩漏 code/secrets。

**已觀察範圍：** 唯讀 Dashboard/API 提供 trace/elapsed/error metadata、後端 snapshot、JSONL export，未知 token/resource usage 保持 UNKNOWN。

**原驗收條件：**

1. 能顯示每個工具實際 wall-clock duration、error code 和 trace id。
2. 未經授權的 HTTP 請求無法讀到任何專案資料。
3. token 統計僅在來源有依據時顯示，否則標 UNKNOWN，不能推算使用者模型 token。

**尚未驗收／完成：**

- A1：schema 驗證前拒絕尚無 audit trace；index/memory/resource/完整 backend health 指標仍不完整。
- A2：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A3：token UNKNOWN 已驗；RSS/LLM token/完整 OpenTelemetry 沒有來源，未提供數值。

**測試案例 ID：** M3-dashboard-auth, M3-tracing-metadata, M3-log-redaction。 **前置需求：** ODIN-002。

**證據：** `verification/feature-evidence.json#ODIN-025`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-026"></a>

## ODIN-026 — Hooks / Agent 策略引導

**Priority:** P2 · **Milestone:** M3 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** integrations

**目標：** 避免 Agent 只使用文字工具而忽略語意分析。

**設計目標：** 可選 CLI hook、client prompts、對常見任務最佳操作建議；不自動更動使用者代理設定。

**已觀察範圍：** 可選 hook dry-run、CAS backup、managed marker disable/restore 已 smoke；不自動註冊代理。

**原驗收條件：**

1. 啟用 hooks 前提供 dry-run diff，停用能復原。
2. 長 session 下工具提示不重複灌入全量文件。
3. 沒有 MCP 支援的子 agent 不宣稱共享 Odin 工具。

**尚未驗收／完成：**

- A1：host file enable/disable 已測；真 client hook registration/lifecycle 未執行。
- A2：提示內容有界；長 session 不重複注入的真 client 驗收未執行。
- A3：文件明示子 agent 需自己的 MCP；實際多 client/子 agent 組合未驗收。

**測試案例 ID：** M3-hook-dry-run, M3-hook-disable。 **前置需求：** ODIN-023。

**證據：** `verification/feature-evidence.json#ODIN-026`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-027"></a>

## ODIN-027 — 可重現驗收及 CI 成本限制

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** ci, verification

**目標：** 所有 PASS 必須來自實際 command exit code 和 artifacts。

**設計目標：** 手動 self-hosted first；fmt、clippy、unit、integration、security、platform workflow；上傳 test log、toolchain、commit SHA、coverage；GitHub-hosted minutes 預設不啟用。

**已觀察範圍：** 17gates原始driver snapshot與200tests/6ignored+native3保留；六語新harness d47fa6…有actualexit2/PARTIAL並綁samebinary/一致productioncratehashes，不重寫舊gate。

**原驗收條件：**

1. 從全新 checkout 執行驗收有可審計 logs；失敗 gate 阻止 release。
2. fixture self-test 與真實 binary/LS/DAP 驗證分開標記。
3. Actions 只有 workflow_dispatch，不自動啟動付費或額度計費的工作流程。

**尚未驗收／完成：**

- A1：17gate與新六語PARTIAL command/log/hash均已具備；全新checkout及完整故障gate負例、跨平台仍未完成。

**測試案例 ID：** M0-ci-selfhosted-manual, M0-evidence-manifest, M0-failure-blocks-release。 **前置需求：** ODIN-001。

**證據：** `verification/feature-evidence.json#ODIN-027`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-028"></a>

## ODIN-028 — Windows x64 MSVC 安裝、升級與卸載

**Priority:** P1 · **Milestone:** M3 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** release, windows

**目標：** 在 Windows 用戶主機安裝並能回退。

**設計目標：** Inno Setup 安裝版與 portable ZIP；PATH/MCP 設定 opt-in；installer lifecycle 包含 silent install/uninstall/upgrade/rollback，版本一致；簽章 optional 而不假裝已簽。

**已觀察範圍：** portable ZIP 與 Inno Setup recipe 已編譯；該 recipe report 使用較早 revision，未視為最終候選。

**原驗收條件：**

1. 乾淨 Windows x64 MSVC 主機完整 install/start/uninstall 後無孤兒 service/目錄。
2. 以 v0.2 state 升級資料，遷移遇錯仍保有備份且可復原。
3. Windows Defender/SmartScreen 與 Defender Controlled Folder Access 限制須實測紀錄。

**尚未驗收／完成：**

- A1：乾淨 Windows 完整 install/start/uninstall lifecycle 未驗收。
- A2：跨版本 state 升級/失敗備份/rollback 未驗收。
- A3：Defender/SmartScreen/CFA 及簽章均未驗收或未提供。

**測試案例 ID：** M3-inno-lifecycle-14, M3-win-upgrade, M3-win-uninstall。 **前置需求：** ODIN-001。

**證據：** `verification/feature-evidence.json#ODIN-028`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-029"></a>

## ODIN-029 — Linux/macOS 原生打包與驗證

**Priority:** P1 · **Milestone:** M3 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** release, platform

**目標：** 沒有跨平台真機不能發 1.0。

**設計目標：** native CI/build on Linux x86_64 and macOS arm64；提供 tar.gz、install script checksum、卸載；路徑、信號與行程清理採各 OS 原生能力。

**已觀察範圍：** Linux0fe03ba在本機Debian 12 VM原生編譯／執行，workspace211 PASS／0 FAIL／3 ignored、fmt、嚴格Clippy及四類smokes通過。a839文件的deb／tar使用同一ELF，實跑安裝／重裝／移除並核對來源、payload及產物hash；macOS及專用signal/journal驗收未執行。

**原驗收條件：**

1. 每 OS 的 binary 在該 OS native test，不以 cross compile check 替代。
2. kill SIGTERM/SIGINT 後 LSP/DAP 子程序被清除，未完成 journal 有可檢查狀態。
3. 同版本 manifest、source SHA、archive checksum 一致。

**尚未驗收／完成：**

- A1（PARTIAL）：Linux x64 VM native build/test與metadata已有適用證據；macOS arm64及完整平台矩陣仍未驗收，cross compile不替代。
- A2：SIGTERM/SIGINT 後代清除與未完成 journal 的 native 行為尚未驗收。
- A3（PARTIAL）：Linux同版本source／package manifest、payload inventory與deb／tar checksum已核對；macOS產物仍未生成／核對。

**測試案例 ID：** M3-linux-native, M3-macos-native, M3-clean-shutdown。 **前置需求：** ODIN-001。

**證據：** `verification/feature-evidence.json#ODIN-029`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-030"></a>

## ODIN-030 — 效能、Token 與品質基準

**Priority:** P1 · **Milestone:** M3 · **Source:** `NOT_VERIFIED` · **Acceptance:** `NOT_RUN`

**Modules:** benchmark

**目標：** 證明快和省，而不是因 Rust 推論效能。

**設計目標：** 固定 repo/commit、backend/version、模型/prompt、cold/warm cache、成功條件、task suite；對比 Serena v2.1 只是性能 benchmark，產品本身不能依賴 Serena。

**已觀察範圍：** 沒有性能、CPU/RSS、token 節省、成功率或比 Serena 更快的實測主張。

**原驗收條件：**

1. 發布 P50/P95 latency、RSS、CPU、output bytes/tokens、任務成功率與重構後編譯通過率。
2. 至少 3 種規模 repo 及 20 個日常 coding 任務，多次執行且同環境。
3. 未達預定效能目標需列性能回歸，不能只報成功樣本。

**尚未驗收／完成：**

- A1：P50/P95/RSS/CPU/output bytes/tokens 與品質統計尚未量測。
- A2：三種 repo 規模、20 個 coding 任務、同環境多次執行尚未完成。
- A3：沒有完整失敗／性能回歸 benchmark suite 報告。

**測試案例 ID：** M3-benchmark-repro, M3-metrics-dashboard, M3-regression-suite。 **前置需求：** ODIN-010。

**證據：** `verification/feature-evidence.json#ODIN-030`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-031"></a>

## ODIN-031 — 套件供應鏈、機密與對外發布治理

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** security, release

**目標：** 公開 GitHub 不可包含私密憑證、個人資料或誤導授權。

**設計目標：** Cargo.lock、SBOM、licenses、advisories；依賴 pinning/checksum；secret scanning、路徑 allowlist；沒有 GitHub token、API keys、機器絕對路徑與私有專案碼。

**已觀察範圍：** c5084f4 audit/licenses/supply-chain/source/publish-policy均exit0；最新文件、最終封裝manifest與公開revision仍需據實同步。

**原驗收條件：**

1. 公開候選 repo 目錄通過 secret scan、license scanner 與 artifact allowlist。
2. README 不聲稱尚未執行的 tests 成功；所有 NOT_RUN 有出處。
3. 權限最小化；公眾 PR 不在無防護 self-hosted runner 上自動執行。

**尚未驗收／完成：**

- A1：當輪candidate掃描已完成；本次新增文件與最終package/artifact allowlist/manifest須最後核對。
- A2：Windows 最終命令與數量已更新；最終封裝內容、交付狀態及實際 GitHub 發布紀錄仍需同步。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M0-no-secret-scan, M0-sbom-license, M0-security-policy。 **前置需求：** ODIN-001。

**證據：** `verification/feature-evidence.json#ODIN-031`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-032"></a>

## ODIN-032 — 競態、崩潰與資料完整性驗證

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** test, recovery

**目標：** 以敵意條件驗證真實寫入安全。

**設計目標：** Crash injection/multi-agent writer contention/partial disk write/permission denied/renamed dirs/symlink race/long path/malicious MCP input；模型不可越權。

**已觀察範圍：** c5084f4 native3測試具持久log/exit0，含guard-until-reap與abort/timeout後代清理；完整磁碟/權限/跨OS敵意矩陣未完成。

**原驗收條件：**

1. 對所有 transaction 轉態注入 failpoint，能確定 state 的下一步。
2. 兩個獨立 process 同時修改同檔，其中一個必須安全拒絕或重新協商，不能靜默覆蓋。
3. 以實際 Windows NTFS 和 Linux ext4 或 equivalent 驗證，不只 mock FS。

**尚未驗收／完成：**

- A1：guard-until-reap與MXC native3測試已具持久log/exit0；partial disk write/permission denied/全部phase與跨OS敵意矩陣未完成。
- A2：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A3：Windows NTFS 有原生紀錄；Linux ext4 等 native FS 仍 NOT_RUN。

**測試案例 ID：** M0-crash-failpoints, M0-two-process-race, M0-permission-denied。 **前置需求：** ODIN-006。

**證據：** `verification/feature-evidence.json#ODIN-032`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-033"></a>

## ODIN-033 — 完整架構文件、開發契約與產品邊界

**Priority:** P0 · **Milestone:** M0 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** docs

**目標：** 讓後續 Agent 可依文件實作而非猜測。

**設計目標：** 中文需求規格、英語 API tags、每項 id/priority/source/status/deps/acceptance/test；修版時更新 machine-readable backlog 與 traceability；文件語義單一來源。

**已觀察範圍：** 40需求/85契約維持38PARTIAL2NOTRUN；追加六語PARTIAL真command/transcript/normalization，17gate與舊checks/digests保留，TSsandbox04已實跑PARTIAL（42/2/6/0），封裝及發布各有獨立scope。

**原驗收條件：**

1. 公開的 53 Serena 功能全部映射至少一項 Odin 原生需求。
2. 原有 58 個工具名稱保留並擴充為 85 個；新增 API 必須含 schema、錯誤碼與測試。
3. README 入口、ROADMAP、RELEASE_GATES 與驗收證據一致。

**尚未驗收／完成：**

- A1：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A2：原 58 保留並擴為 85；schema/policy/dispatch/tests/docs 已接線，真實全工具矩陣未驗收。
- A3：17gates與六語PARTIAL已分層記錄，TSsandbox04已有PARTIAL/exit2獨立證據；最終封裝NOT_RUN，公開revision依publication receipt同步。

**測試案例 ID：** M0-spec-coverage, M0-spec-validator, M0-doc-links。 **前置需求：** ODIN-003。

**證據：** `verification/feature-evidence.json#ODIN-033`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-034"></a>

## ODIN-034 — 語言後端安裝與能力健康檢查

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** lsp, cli

**目標：** 避免把 command 存在當成語意後端可用。

**設計目標：** 清楚安裝指引/可選受控 auto-install；版本 pin + hash；capability probe、workspace readiness、錯誤修復指引；不自動執行專案內不可信可執行檔。

**已觀察範圍：** 六語12/12真初始化和backends版本/log已實跑；registry/versionprobe不等於全部能力，14UNAVAILABLE與TS/Java12個診斷NOT_VERIFIED未升PASS。

**原驗收條件：**

1. 後端缺失回 MISSING_BACKEND，版本衝突回 BACKEND_UNSUPPORTED。
2. 使用者明確同意安裝後在隔離目錄且 hash 核對；失敗不留下可執行半成品。
3. 更換 server 版本會觸發必要 index invalidation。

**尚未驗收／完成：**

- A1：六語已實際initialize，沒有把profile名字當支援；unversioned診斷、unsupported能力與所有版本/ready狀態反例仍未全驗收。
- A2：沒有可選 hash-pinned backend auto-installer，不能宣稱安裝驗收成功。
- A3：server 版本更換所需 index invalidation 尚未完整驗收。

**測試案例 ID：** M2-backend-discovery, M2-installer-consent, M2-backend-readiness。 **前置需求：** ODIN-007。

**證據：** `verification/feature-evidence.json#ODIN-034`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-035"></a>

## ODIN-035 — 純原生檢查與 Quick Fix 引擎

**Priority:** P2 · **Milestone:** M3 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** diagnostics, adapters

**目標：** 取代 JetBrains inspections 的用例，而非聲稱 JetBrains 內部 API 等價。

**設計目標：** 檢查供應者 trait、靜態規則與 linter 解析、fix-to-WorkspaceEdit、批次規則選擇；依語言/檔案 scope 明確顯示 support。

**已觀察範圍：** c5084f4同binary真Clippy/Ruff/ESLint經nativeMXC的30 checks/commandexit0，涵蓋findings、readonly、safeCASpreview/apply/rollback與scratch清理；全規則/私有IDE等價未驗收。

**原驗收條件：**

1. 能在 Rust/Python/TypeScript fixture 取得 rule、location 與可預覽 quick fix。
2. 即使載入 JetBrains IDE 也不依賴 Serena 插件或授權。
3. 規則尚不支援時顯示未實作，不給空結果稱合格。

**尚未驗收／完成：**

- A1：c5084f4三個真provider已驗rule/location/safe preview；完整多專案/規則/版本/平台矩陣仍待驗收。
- A2：無 Serena/JetBrains 相依；不宣稱 JetBrains 私有 inspections/rules 或全部 IDE 等價。
- A3：unsupported/unknown/stale/truncated 會拒絕 clean/fix；真版本與所有 analyzer 失敗路徑未驗收。

**測試案例 ID：** M3-rules-rust, M3-rules-python, M3-quickfix-safe。 **前置需求：** ODIN-017。

**證據：** `verification/feature-evidence.json#ODIN-035`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-036"></a>

## ODIN-036 — 持久式除錯命令會話（非 Python REPL）

**Priority:** P2 · **Milestone:** M3 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** dap, flow

**目標：** 提供連續 evaluate、breakpoint 和 step 的批次交互。

**設計目標：** Rust 的 bounded debugger DSL / explicit session；透過 DAP 而不是 eval 任意 Python；命令結果有 request ID、timeout、取消，作用範圍為 session。

**已觀察範圍：** bounded DAP batch 新增 single/batch 共用 immutable payload 與 privileged metadata；當輪真 adapter 連續 session 未驗收。

**原驗收條件：**

1. 一次 session 中連續設置 breakpoint、launch、step、evaluate 成功且每步有證據。
2. 不能從 debug DSL 任意執行 host shell 或跨專案存取。
3. debuggee 結束會回 session closed，持續讀取不會假裝仍在執行。

**尚未驗收／完成：**

- A1：連續流程目前有 protocol fixtures；同一 debug_batch 在真 adapter 的 launch→step→evaluate 專項驗收未執行。
- A2：有 scope/host debug deny 測試；debuggee 是 privileged target，沒有 OS sandbox 保證。
- A3：狀態機與 terminated fixtures 已測；真 adapter session 結束/重啟矩陣未全驗收。

**測試案例 ID：** M3-debug-command-session, M3-debug-scope-deny。 **前置需求：** ODIN-013。

**證據：** `verification/feature-evidence.json#ODIN-036`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-037"></a>

## ODIN-037 — 記憶 schema 升級與 import/export

**Priority:** P2 · **Milestone:** M3 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** memory, migration

**目標：** 長期保存的記憶不因版本升級損毀。

**設計目標：** versioned manifest、repo-local export/import、global migrations、乾跑、備份、校驗、衝突標示；避免寫入外部 Serena 格式。

**已觀察範圍：** memory bundle 有 schema/version/digest、create-only import、scope ownership/refs 與 collision 拒絕。

**原驗收條件：**

1. 0.2.0 central memory 以 dry-run 顯示 diff，遷移後 refs 完整且可逆。
2. 資料損毀/衝突時不覆蓋現存目標。
3. 不要求安裝 Serena 或讀取 Serena 私有路徑。

**尚未驗收／完成：**

- A1：沒有 central→repo-local 自動 migration/dry-run/reverse 流程。
- A2：bundle corruption/collision 有測試；完整跨版本備份/恢復矩陣未驗收。
- A3：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。

**測試案例 ID：** M3-memory-roundtrip, M3-memory-conflicts。 **前置需求：** ODIN-020。

**證據：** `verification/feature-evidence.json#ODIN-037`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-038"></a>

## ODIN-038 — 精確文字編輯與多命中選擇

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-core, edit

**目標：** 補齊目前 replace_in_files 不能選指定 occurrence 的限制。

**設計目標：** 預覽每個匹配獲 immutable occurrence ID，明確 allow_multiple/count/regex flags，依文件 hash 選取，preview 套用同一交易機制。

**已觀察範圍：** immutable occurrence IDs、5 選 2、regex capture/CRLF、stale/duplicate/empty-match 拒絕已有 executed engine tests。

**原驗收條件：**

1. 同檔 5 命中可只改指定 2 個且衝突拒絕，不多改任何匹配。
2. Regex 多行與 CRLF 的預覽與實際套用位移一致。
3. 空匹配、重疊替換、數量不符回明確錯誤。

**尚未驗收／完成：**

- A1：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A2：歷史 72 工具 Windows 候選有 source/binary 綁定；當輪變更與此原條件的完整功能/正反例/平台矩陣仍需新證據。
- A3：已執行空匹配/duplicate/stale 與 count 負例；完整 overlapping replacement selection 的獨立案例清單仍需補齊。

**測試案例 ID：** M2-occurrence-select, M2-regex-multiline, M2-conflicting-replacements。 **前置需求：** ODIN-006。

**證據：** `verification/feature-evidence.json#ODIN-038`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-039"></a>

## ODIN-039 — 後端 debugger 設定及啟動安全

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** dap, config

**目標：** 不同語言 Debug adapter 可複製設定並安全啟動。

**設計目標：** named profiles、發現可執行檔與版本、launch config schema、stdin/stderr 管理、timeout、明確列出允許的 env，不繼承機密。

**已觀察範圍：** 可選 host debug_launches 固定 named adapter 的 launch/attach/restart payload，caller 不得擴充 env/argv；未設 profile 仍 explicit privileged，完整 adapter schema/OS matrix 未完成。

**原驗收條件：**

1. launch profiles 對不同 debugger 具有固定 schema 與測試 fixture。
2. 惡意 launch args/env 不能跳出 host 授權且沒有任意 shell 字串。
3. 停止會回收 debuggee 的 process tree。

**尚未驗收／完成：**

- A1：immutable host payload 不是每個 adapter 完整語意 schema/version discovery；真正反例未完成。
- A2：有 debug_launches 時 caller 不能擴 env/argv；未設定標 privileged，MXC 與真 debuggee 矩陣另外驗收。
- A3：author MXC 有後代 termination；integrated DAP/debuggee、跨 OS 矩陣及正式 lifecycle log 待補。

**測試案例 ID：** M2-profile-schema, M2-dap-process-tree。 **前置需求：** ODIN-012, ODIN-013。

**證據：** `verification/feature-evidence.json#ODIN-039`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

<a id="odin-040"></a>

## ODIN-040 — 工作證據、結果分塊與可追溯執行

**Priority:** P1 · **Milestone:** M2 · **Source:** `NATIVE_SOURCE_PARTIAL` · **Acceptance:** `PARTIAL`

**Modules:** odin-engine, observability

**目標：** 大型輸出保留證據而不是以 prompt 截斷造成失真。

**設計目標：** 每工具含 trace_id、result_id、sha256、truncated flag、後端版本、error code；大資料可分頁；log redaction、TTL/配額/清理。

**已觀察範圍：** c5084f4標準binary/三LS/analyzer與200tests已具source/hash綁定；全面backend provenance、retention/encryption/backup仍未完成。

**原驗收條件：**

1. 結果在保留期限內用 result_id 分頁取回並驗證 digest。
2. 過期或權限錯誤返回明確訊息，沒有跨 project 結果洩漏。
3. 不暴露秘密 token、絕對本機私密路徑或完整原始碼到公共 Dashboard log。

**尚未驗收／完成：**

- A1：分塊與 digest 有測試；每工具完整 backend version provenance 尚未覆蓋。
- A2：TTL/跨 project 有回歸；清理於讀寫時執行，沒有背景 retention service。
- A3：Dashboard metadata allowlist 有測試；journal/audit/index 的完整 retention/encryption/backup 未實作。

**測試案例 ID：** M2-result-chunks, M2-results-ttl, M2-trace-audit。 **前置需求：** ODIN-003。

**證據：** `verification/feature-evidence.json#ODIN-040`；歷史checks/digests原樣保留，本輪另看language_installers_v3；extension_evidence保留歷史scope。

本輪 TypeScript native sandbox 最終證據：`verification/language-ide-final-sandbox-typescript-04.json` 及其 command／transcript，actual exit2／PARTIAL、42 PASS／2 UNAVAILABLE／6 NOT_VERIFIED／0 FAIL。固定 host bootstrap 與 child flags 的明確 profile 成功；不是自動 fallback，沒有直接 child EPERM 證據。其他五語的實際 sandbox LSP 仍 NOT_RUN，kernel deny／程序清理由獨立 native opt-in tests 證明。
