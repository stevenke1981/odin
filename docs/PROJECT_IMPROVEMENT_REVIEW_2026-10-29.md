# Odin 專案檢視與改善建議

檢視日期：2026-10-29；原始碼基準：`c78ca8d002a6f42ee95cad74712b0c943f0918a8`。
範圍為架構、已知限制、交付證據、待辦及相關程式碼的定向閱讀；本報告不是全面安全稽核或效能測試。優先順序是建議的工作排序，不代表已確認新的程式缺陷。

## 現況

Odin 採獨立 Rust 核心、直接 LSP/DAP、明確 host 權限及 expected-hash 交易，能力與驗收範圍已有紀錄。`verification/delivery-status.json` 的 `current_extension` 為 `PASS_WITH_DOCUMENTED_GAPS`：Windows native gates 和指定安裝生命週期有證據，但已安裝 MCP 語意仍有診斷版本缺口；40 項需求尚未全數驗收。歷史 Windows 候選的 BLOCKED／FAIL 僅適用其來源及 binary，不能套用為現行候選的結果。Linux VM 的歷史原生和封裝實證亦只適用其對應版本。

## 建議工作

### 1. P1：讓目前狀態與歷史證據更容易區分

- **依據：** `verification/delivery-status.json` 的 `current_extension` 明確指向 Home／expanded LSP，但同檔的舊 `evidence` 和 `docs/IMPLEMENTATION_BACKLOG.md` 仍保留過去候選的 BLOCKED 敘述，搜尋時容易誤讀。歷史紀錄有保存價值，不應改寫成新的結果。
- **建議：** 建立簡短的現行狀態索引，逐平台列出來源 revision、binary hash、測試環境、驗收範圍與未驗收項目，連到原始收據；舊看板頂部加清楚的歷史標記與現行索引連結。既有證據保持原樣。
- **驗收：** 讀者能從索引區分現行與歷史候選，所有 PASS 均能追到同一來源／產物的命令與結果；索引一致性檢查不宣稱重新執行產品測試。

### 2. P1：補齊真實後端與 client 的能力矩陣

- **依據：** `docs/KNOWN_LIMITATIONS.md` 第 6、18、19、23 項與交付狀態仍區分 profile、真實 IDE 能力及未版本化 diagnostics；現行記錄中的 client restart 仍為 NOT_RUN。
- **建議：** 先以使用頻率最高的 Rust、Python、TypeScript 為一批，列出實際 client／平台／後端版本及 navigation、references、diagnostics freshness、rename preview/apply/rollback；另外驗證現有 client 重連後的 workspace 綁定。後端不宣告的功能保留 UNAVAILABLE，未完成的保留 NOT_RUN 或 NOT_VERIFIED。
- **驗收：** 每個結果附 source／binary hash、backend 版本、專案及實際命令或操作收據；fixture、版本探測與真實 MCP 測試分開記錄。不要用 regex 或結構 hints 充作語意結果。

### 3. P1：補強安裝、升級與故障復原驗收

- **依據：** `docs/KNOWN_LIMITATIONS.md` 第 14、21、22 項列出乾淨機、跨版升級、取消／崩潰復原及多檔設定非瞬間原子提交等限制；目前已有的同版生命週期實證不涵蓋這些情境。
- **建議：** 在隔離測試機建立乾淨安裝、舊版升級、重裝、卸載保留設定，以及各明確中斷點的測試；記錄 host 設定、備份和檔案 hash，提出可執行的人工復原步驟。
- **驗收：** 每個中斷點均核對檔案、設定、備份與實際行為，外部修改衝突不得被覆寫；各平台、版本與簽章狀態分開呈現。

### 4. P1：建立資料保留與復原操作方式

- **依據：** `crates/odin-engine/src/results.rs` 對結果 spool 已有清理與配額；`docs/KNOWN_LIMITATIONS.md` 第 13 項指出 journal、audit、index 尚無完整保留／加密／備份策略。`crates/odin-core/src/transaction.rs` 的 journal 也承載交易復原資料。
- **建議：** 先提供狀態與磁碟用量檢視、備份／還原文件和明確的保留策略選項；清理前需保留未完成及需要復原的交易。加密與 ACL 改善可依實際部署需求規劃，不預設刪除既有紀錄。
- **驗收：** 使用測試副本驗證備份還原、保留期限與空間上限；Prepared／RecoveryRequired 交易及衝突情境不可因清理而失去復原依據。

### 5. P2：先量測索引與專案分析，再決定最佳化

- **依據：** `crates/odin-engine/src/analysis.rs` 每次呼叫 `search::files`，並回報截斷狀態；`crates/odin-core/src/search.rs` 使用目錄走訪。已知限制第 9、15、18 項指出查詢前掃描與 benchmark 尚未完成，尚無足夠數據斷定瓶頸或改善幅度。
- **建議：** 對小、中、大三種具可重現內容的專案測量冷／熱查詢、更新後查詢、P50/P95 延遲、CPU、記憶體與 I/O，並記錄 ignored files、檔案數與截斷情況。視結果再評估增量更新或 watcher；保留完整重掃及失效處理。
- **驗收：** 發布測試方法、硬體、樣本及原始數據；同時檢查建立、修改、刪除、ignore 規則變更及超過限制的結果正確性。未測前不宣稱效能優勢。

### 6. P2：改善診斷的來源與新鮮度呈現

- **依據：** `docs/ARCHITECTURE.md` 說明 diagnostics 優先 pull，push cache 需標示版本；`verification/delivery-status.json` 列出部分後端未驗證 document version 的診斷。
- **建議：** 在工具結果及使用文件清楚呈現來源、document version、內容 hash、pending／stale／unverified 狀態，並以真實後端測試修改文件前後的結果；無法驗證版本時保留不確定狀態。
- **驗收：** 同時驗證尚未收到結果、舊版本推送、版本缺失與目前版本四種情況，不把 pending 當成零診斷。

### 7. P2：整理供應鏈與發行身分

- **依據：** 已知限制第 24、25 項記錄 notice gaps、lock superset 非精確 linked SBOM、Slint upstream rebuild 未執行，且產物與 inventory 未簽章。
- **建議：** 對實際發布產物產生可追溯的依賴與 runtime 清單，逐項處理已列出的 notice 缺口；評估簽署與驗證發行產物，並在可重現環境執行 Slint 來源重建。
- **驗收：** 清單與實際 artifact hash 綁定；未能確認的依賴、重建與法律審查保留未完成狀態。簽章驗證和 hash 完整性分開報告。

## 建議執行順序與約束

先處理第 1 項，讓團隊能確認要驗證的具體候選；接著按使用需求執行第 2、3、4 項；第 5 項先量測，再決定是否實作效能改動。第 6、7 項可依產品發布需求排程。每項應切成獨立變更，依專案規範保留工具契約、政策、測試與文件的一致性；任何專案來源修改仍經 Transactions 與 expected hash，不新增預設 shell／debug／write 權限或自動 fallback。

## 本次驗證範圍

本次僅閱讀文件、交付狀態與相關程式碼並產生報告；未執行 cargo fmt、workspace tests、clippy、binary smoke、真實後端、安裝、benchmark 或安全稽核，均為 **NOT_RUN**。文中既有結果是專案保存的歷史證據，不是本次重跑結果。
