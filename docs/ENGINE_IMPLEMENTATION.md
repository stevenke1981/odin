# Odin 引擎補完與驗證範圍

原生工具契約、策略與可預覽修改目前共 86 個工具，由 `scripts/generate_catalog.py` 生成到兩份 `tools.json`；`scripts/generate_docs.py --api-only` 更新 API，不會改動另外管理的發布與對照文件。新增 IDE 查詢、格式化交易與 native inspections 詳見 [IDE_IMPLEMENTATION.md](IDE_IMPLEMENTATION.md)。Odin 不載入或呼叫 Serena。

## 已實作行為

`get_version`、`get_schemas`、`get_capabilities` 可查版本、完整工具參數與有效權限。只有 `probe:true` 才主動啟動已由 host 配置的 LSP，ready 來自真正 initialize 回覆及當前程序狀態；沒有配置或所有後端失敗時，符號查詢回明確錯誤。

`find_symbol` 支援 substring／exact／glob／regex 的符號名稱篩選，以及 case_sensitive、scope、include_kinds、exclude_kinds、depth、分頁、檔案分組和 include_body。資料來源仍是 LSP DocumentSymbol；regex 只篩選後端已回傳的名稱。depth 為名稱階層最大深度，最上層為 0；offset/limit 以當次排序的檔案與後端符號順序分頁，跨頁間來源變更可能改變結果。這不是持久快照游標。

`replacement_preview` 列出匹配的 hash、byte offset、1-based line、0-based UTF-16 column、before/after 和 immutable occurrence ID。ID 綁定檔案內容 hash、位置與搜尋選項。`replace_in_files` 可只選擇部分 ID，必須指定全部匹配數，並透過 Transactions 先預覽、後 CAS apply。literal replacement 不解釋 `$1`；regex replacement 支援 Rust regex captures。空匹配、重複 ID／檔案、舊 hash、舊 ID、數量不符均拒絕。

Move、Inline、QuickFix 與一般 CodeAction 共用相同交易流程：先取得完整受限來源快照，再向真正 LSP 請求 CodeAction，依唯一 title 與指定 kind 選取，必要時只呼叫 codeAction/resolve，將 edit 轉成 core Change 後預覽。QuickFix 帶入當前 diagnostics。disabled、含 command、缺 edit、錯誤 kind 或模糊 title 都拒絕。Move 還會核對 backend edit 實際修改 source 和指定 target。

記憶 export 回版本化 `bundle`，import 為 create-only，move_scope 核對所有 scope 權限及引用。兩者回 shared memory transaction preview，以專屬 memory_transaction_preview/apply 查看及提交；journal 綁定 initiating project，不能借另一專案讀取或套用。

FTS5 查詢先執行 core 增量 refresh，能看到新增、修改和刪除的檔案；它仍是詞彙索引，沒有 filesystem watcher 或向量搜尋。

flow 使用每個 `(project, session)` 的獨立 namespace mutex；全域 map 鎖不跨工具 await。pointer、take、map、filter、reduce(count/sum) 是有界 JSON 資料操作，每個步驟的工具仍經 host policy 與 project gate。單一 flow 受 host RPC timeout 約束；取消或錯誤只丟棄 namespace draft，已發生的唯讀後端 cache/audit 副作用仍存在。busy session 的 clear 回 CONFLICT。sum 使用有限 f64，沒有聲稱任意精度。

Flow 展開引用前會以借用走訪計算 escaped JSON bytes 和節點數，扣除同一 operation 的累計預算後才能 clone。單值、return、工具回傳與整個 namespace 均受 max_session_bytes、最多 65,536 個節點和 24 層深度限制；map 也先累計輸出再逐項 clone。超大工具結果即使稍後 pointer/take 會縮小也會先拒絕，避免先分配後截斷。namespace 在複製及插入前檢查，不為最後一步再保留額外副本。

文字替換先計 matches 與 host max_matches，再以 checked arithmetic 檢查結果不超過 max_file_bytes；regex replacement template 解析為借用片段，先計 capture 展開長度才配置輸出，沒有先呼叫無界 replace_all。template 最多 65,536 個片段。Occurrence preview 不建立修改後的檔案；每筆 metadata、escaped before/after 總量在展開前保守限制於 host max_output_bytes 與 16 MiB 較小值，超限請縮小檔案／匹配／replacement。多檔案的原始及目標文字合計另限 16 MiB，衝突與超限均不修改來源。

include_body 查詢於每檔 symbols RPC 前保留單一來源快照，回覆後核對現檔內容一致；RPC 期間有外部改寫便回 CONFLICT，不能把舊 range 套用到新 body。find_symbol 先跳過 offset，才擷取和複製該頁的 body，最多只保留 limit 筆及下一頁旗標；metadata 和 escaped body 在複製前累計限制 16 MiB，分組輸出會預留重複資料的預算。rename/CodeAction 的來源快照維持 64 MiB 上限，先檢查才加入 map。

debug_start 回 project-scoped session UUID。debug_batch 最多 32 個 allowlisted DAP requests，核對 session ID、每步回 request ID、pending ticket 或真實錯誤；全批次有 host RPC deadline，逾時明確標示 adapter-side outcome unknown。這不是 Python REPL。host 的 allow_debug 代表授權除錯器執行 target；adapter 繼承環境採 allowlist。可另以 debug_launches 固定 launch/attach/restart payload，並以明確的 MXC 配置限制 OS 權限。沒有這些配置時，保留原先明示的 privileged host debug 模式。

`debug_request` 的原生命令 `cancel_ticket` 接受 `arguments.ticket`，對 launch/attach ticket 請求 best-effort 取消，回 outcome_may_be_unknown，不宣稱 debuggee 一定沒啟動。debug_stop 會要求 terminateDebuggee 並收尾 adapter 程序；restart_language_server 先移除 client 再執行 shutdown/exit，沒有持 map mutex 跨 shutdown await。

shell 執行 host 固定 argv profile，使用 core 的環境 allowlist 與 ProcessTree。Windows Job Object／Unix process group 的生命周期管理不等於安全隔離；程序啟動與 Job 指派之間仍有窗口。

成功工具結果附 trace ID、elapsed_ms、加 trace 前的 JSON bytes/digest；metadata audit 使用同一 ID，不保存原始輸入輸出。schema 驗證前被拒絕的請求尚不產生 audit trace。大結果保留 exact bytes SHA-256、每塊 digest、到期時間，並依 project 隔離。固定保留 24 小時／最多 128 份／128 MiB，滿額拒絕新 spill；到期於 spill/read 時清理，沒有背景排程、加密或備份。

## 證據與限制

定向驗證命令為 `cargo fmt -p odin-engine -- --check`、`cargo test -p odin-engine --locked`、`cargo clippy -p odin-engine --all-targets --locked -- -D warnings`。實際命令、exit code 與來源 hashes 記錄於 `verification/engine-verification.json`；執行 `python verification/engine-verify.py` 可重跑。

`completion.rs` 中的 LSP／DAP 測試啟動 Python protocol peer，驗證真實子程序、協定分派、版本／scope／交易邊界；它們不證明 real language refactoring、真實 debuggee 或多語言功能等價。具體語言伺服器必須自己提供 `refactor.move`／`refactor.inline`／`quickfix` action；沒有這些能力會回明確錯誤，無文字替換 fallback。這次引擎切片不宣稱 JetBrains 私有 inspections、任意 Python 執行、Linux/macOS 原生驗收或完整平台等價。

預設 write、shell、debug 都不會因使用新工具而開啟。來源仍只有 Transactions 能修改；既有檔案必須 expected hash，外部衝突不覆寫。最終 workspace、binary smoke、真實後端與發布證據由整合階段另外驗收。
