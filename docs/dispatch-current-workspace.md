# Current workspace 與即時分析工作計畫

任務：ODIN-AUTO-PROJECT；base：8f2c264ff1900dff61b81bec88e0eca466adf168。
分支：feat/current-workspace-analysis。使用者已授權實作、commit、push；不建立 release、不啟動 cloud CI。

## 完成定義

- 同一 host/client entry 用 `--workspace .` 在不同 cwd 啟動，選擇該目錄的專案，不沿用 sigma。
- 既有註冊 root 內的 cwd 選該註冊專案；新增 root 必須在可信 host 的 workspace.allowed_roots 內，預設唯讀。
- root 於啟動時固定；不向上猜 root、不從 MCP 接收任意 root、不建立 shared active_project。
- 狀態依 canonical root hash 隔離；不從 repository 讀取 executable host config、不自行安裝或選擇執行後端。
- project_analysis 每次掃描 live files，清楚列出截斷、來源 hash、讀取限制與尚未執行的檢查。
- 工具 catalog/schema、policy、dispatch、測試、API 與安全/限制文件同步。
- 真實 fmt、workspace tests、Clippy、Windows binary CLI/MCP smoke，保留失敗與命令紀錄；獨立 review 後 commit/push。

## 實際 owner 與依賴

| Instance | 責任 | 寫入範圍 |
|---|---|---|
| /root | 技術組長、唯一實作者與整合、命令驗證、push | 本分支的 Rust、tests、生成 catalog/docs、計畫及驗收證據 |
| /root/project_binding_audit | API/隔離架構調查，後續獨立審查 | 唯讀 |
| /root/validation_audit | QA/現況調查，後續獨立審查 | 唯讀 |

原碼寫入經已安裝 Odin 的 transaction_prepare/preview/apply，使用讀取時的 expected SHA-256。
任務專用、repo 外 host config 明確只授予 E:/odin write；shell/debug 關閉。生成器與 rustfmt 在外部 staging 執行後再交易套用。
架構/QA 調查 -> 單一作者實作 -> 實際 gates 與獨立 review -> 修正 -> 證據 -> commit/push。
團隊 instance 的調查與審查不代表多人 GitHub 核准或 OS 隔離。

## 非目標及初始狀態

不提供 session 途中任意新增 root、背景 watcher、語意分析替代器、權限提升或 backend parity 宣稱。
初始所有本次測試/實作驗收為 NOT_RUN；舊 delivery-status PASS 僅屬原快照。

## 最終交付

Windows fmt、workspace tests、嚴格 Clippy、release build：實際 exit 0。
Rust：233 passed / 0 failed / 6 ignored；workspace/CLI/stdio/HTTP/cancellation
smoke：35/64/24/36/8 個檢查 PASS。真正已配置的 Codex entry 在 Odin/sigma
啟動兩個獨立 MCP；Odin 專案取得 20 個真正 Rust LSP symbols。
兩位獨立 agent review 與最終證據核對 PASS，四個發現均有修正及回歸。
本機 host/Codex 設定已 CAS 更新並保留備份；目前已開啟的 session 未重載。

交付索引：[本次驗收](../verification/current-workspace-validation.json)、
[獨立審查](../verification/current-workspace-review.json)、
[交接](../verification/current-workspace-handoff.json)、
[使用方法](CURRENT_WORKSPACE.md)。Commit/push 結果於最後的 publication receipt
及 Git remote 查核另行記錄，不把本機 PASS 當成遠端或 cloud CI 結果。
