# 已知限制與未驗收項目

Windows 建置與真實後端測試已執行；以 verification 最新報告為準。
以下項目不能因工具或測試數增加而視為完成。

1. 40 項需求尚未全數驗收。Linux x64、macOS arm64、乾淨 Windows 與三個真正 client 應用的完整矩陣保持 NOT_RUN。
2. Job Objects／process groups 是生命週期控制，沒有 AppContainer／restricted token、namespace／seccomp 或 macOS sandbox。Job 指派有 spawn 後的窗口。
3. I/O 使用 no-follow directory capabilities；Windows 大小寫、8.3、junction／hardlink 有負例。CAS 最後檢查與替換之間的外部競態不是 OS 全域原子 CAS。
4. WorkspaceEdit 限 regular UTF-8 files 與既有父目錄。directory、recursive、binary、overwrite、需確認 annotations 明確拒絕。
5. Move／Inline／QuickFix 只接受後端提供的 edit；沒有對應 action、只回 command 時拒絕。沒有 JetBrains 私有 inspection／REPL 相容或任意 Python eval。
6. 真實三語驗收涵蓋符號、引用、rename／rollback；不代表 40+ 語言、所有 diagnostics／refactors、宏／反射與 workspace 特例都已驗證。
7. Safe Delete 只檢查靜態引用；公開 API、動態 consumers、級聯刪除與語言專用 reachability 未完整處理。
8. 記憶是 host central storage；import 為 create-only，無法表達跨 project local 引用時拒絕。沒有自動 repo-local migration、完整 ignored patterns／YAML/Jinja 相容。
9. 索引是查詢前掃描的增量 lexical FTS5，沒有背景 watcher、embeddings 或向量 RAG；大型專案掃描成本未完成 benchmark。
10. flow 是 bounded JSON，沒有任意 eval；不是完整 DAG scheduler／Python 語法相容。取消不會回復已發生的 cache/audit 副作用。
11. DAP launch／attach／evaluate 是 privileged debug 輸入。host adapter profile 與程序清理不是 debuggee 的 OS 權限限制；逐 adapter launch schema 還需擴充。
12. Dashboard 是單操作者唯讀檢視。沒有 OAuth、租戶 ACL、LLM token、RSS 或完整 OpenTelemetry 來源，未知值維持 UNKNOWN。
13. Spool 有 24 小時／128 份／128 MiB project 配額，於讀寫時清理；journal、audit、index 沒有完整 retention／加密／備份。Windows ACL 未自動強化。
14. Installer smoke 不含跨版升級、乾淨機器、取消／崩潰 rollback、Defender／SmartScreen／CFA；未簽章。其他平台打包腳本不等於原生驗收。
15. 沒有速度、RSS、CPU、token 節省或 Serena 性能優勢主張。三規模／20 任務 benchmark、formal security audit 維持 NOT_RUN。

Transaction before/after snapshots 總量上限 32 MiB；來源快照、記憶與結果另有
配額。超限應縮小操作，不可覆寫衝突或移除配額取得成功。
逐需求狀態見 `verification/feature-evidence.json`。
