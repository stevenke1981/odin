# 已知限制與未驗收項目

Windows 建置與真實後端測試已執行；以 verification 最新報告為準。
以下項目不能因工具或測試數增加而視為完成。

1. 40 項需求尚未全數驗收。Linux x64、macOS arm64、乾淨 Windows 與三個真正 client 應用的完整矩陣仍未驗收；特定 VM／安裝包測試不等於完整平台驗收。
2. OS 沙箱目前選用 Windows MXC ProcessContainer、固定 base-container tier 與 executor hash；host 必須明確配置。未配置的 LSP／DAP／命令仍無 OS 隔離；inspections 則拒絕執行。Linux/macOS sandbox、BFS fallback、完整 UI／registry／device 隔離矩陣未驗收。詳見 OS_SANDBOX.md。
3. I/O 使用 no-follow directory capabilities；Windows 大小寫、8.3、junction／hardlink 有負例。CAS 最後檢查與替換之間的外部競態不是 OS 全域原子 CAS。
4. WorkspaceEdit 限 regular UTF-8 files 與既有父目錄。directory、recursive、binary、overwrite、需確認 annotations 明確拒絕。
5. Move／Inline／QuickFix 只接受後端提供的 edit；沒有對應 action、只回 command 時拒絕。沒有 JetBrains 私有 inspection／REPL 相容或任意 Python eval。
6. 語言 profile registry 是安裝／設定指引；`odin languages` 不代表已安裝或功能通過。已執行的逐語言／IDE 結果以 source-bound verification 報告為準；不宣稱 40+ 語言、所有 diagnostics／refactors、宏／反射與 workspace 特例都已驗證。
7. Safe Delete 只檢查靜態引用；公開 API、動態 consumers、級聯刪除與語言專用 reachability 未完整處理。
8. 記憶是 host central storage；import 為 create-only，無法表達跨 project local 引用時拒絕。沒有自動 repo-local migration、完整 ignored patterns／YAML/Jinja 相容。
9. 索引是查詢前掃描的增量 lexical FTS5，沒有背景 watcher、embeddings 或向量 RAG；大型專案掃描成本未完成 benchmark。
10. flow 是 bounded JSON，沒有任意 eval；不是完整 DAG scheduler／Python 語法相容。取消不會回復已發生的 cache/audit 副作用。
11. DAP evaluate 與未配置 immutable debug_launches 的 launch／attach／restart 是 privileged debug 輸入。有 debug_launches 時只接受 host 固定 payload，caller 不得擴充 env/argv。OS 權限由另外配置的 MXC grants 決定；固定 payload 不代表全部 adapter 的語意 schema 已驗收。
12. Dashboard 是單操作者唯讀檢視。沒有 OAuth、租戶 ACL、LLM token、RSS 或完整 OpenTelemetry 來源，未知值維持 UNKNOWN。
13. Spool 有 24 小時／128 份／128 MiB project 配額，於讀寫時清理；journal、audit、index 沒有完整 retention／加密／備份。Windows ACL 未自動強化。
14. Windows 候選已有 portable／Inno 安裝、同版重裝與卸載證據；每份結果只適用其執行檔 hash。Windows／Linux 最新產物與真實驗收見 delivery-status 及 package-v3 報告；腳本或編譯本身不等於可執行／已安裝。未驗收跨版升級、乾淨機器、取消／崩潰 rollback、Defender／SmartScreen／CFA；套件未簽章。
15. 沒有速度、RSS、CPU、token 節省或 Serena 性能優勢主張。三規模／20 任務 benchmark、formal security audit 維持 NOT_RUN。
16. Native inspections 目前解析 Clippy、Ruff、ESLint。必須有 inspection host grant 與唯讀來源的 native sandbox；fix 僅先回 CAS preview。報告只描述固定 profile 的範圍，不保證全專案／全部規則覆蓋。見 IDE_IMPLEMENTATION.md。
17. Native Ruff formatting 需另外明確的 host formatter/write grants、固定 stdin profile 與唯讀來源 sandbox；不自動回退，也不支援 caller range／format options。Windows AppControl 擋住的編譯測試不算通過；真實後端測試不能替代被擋住的完整 Rust gate。

18. `--workspace .` 依 server 啟動 cwd 綁定，不是 session 中途任意切 root。client 若固定 cwd，需明確指定 root/重啟。新 workspace 僅唯讀、需 host allowed_roots；不向上推測 repository 根。`project_analysis` 是每次掃描的結構/設定 hints，不是語意分析、依賴 parser 或背景 watcher；有掃描/evidence 限額，structure digest 不涵蓋所有來源內容，讀取非原子。所有真實 client 生命週期與 Linux/macOS 新功能仍需個別驗收。見 CURRENT_WORKSPACE.md。

Transaction before/after snapshots 總量上限 32 MiB；來源快照、記憶與結果另有
配額。超限應縮小操作，不可覆寫衝突或移除配額取得成功。
逐需求狀態見 `verification/feature-evidence.json`。
