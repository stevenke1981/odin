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
18. Client 自動載入的驗證範圍有限。七種 entry 的生成與 MCP 啟動由 `scripts/client_setup_smoke.py` 驗證（127/127），不啟動任何 client 應用。真實應用的工具呼叫只有 Claude Code 與 Codex CLI；Codex 以 `-c` 覆寫測試，未讀取寫入的 config.toml。Gemini CLI、Cursor、VS Code 未以真實應用驗證，格式依其文件化 MCP 設定。真實應用紀錄綁定 `0cc7d50` 的 binary（`e1820f38…`）；之後移除 `\\?\` 前綴、專案層級建議移除、`project add` 欄位與保護等變更未經真實應用重測。見 [CLIENT_SETUP.md](CLIENT_SETUP.md)。
19. OpenCode 只驗證到連線（`opencode mcp list` 為 connected），工具呼叫 NOT_RUN：其 `opencode run` 即使在無關目錄、不用工具時也因模型供應商 UnknownError 失敗。AGY 工具執行 NOT_RUN：headless print 模式自動拒絕 `mcp` 權限，需 AGY permissions.allow 規則或互動核准。
20. Session default project 只是 MCP instructions 的建議。client 可能忽略它，模型也可能選擇其他已配置專案；它不授予權限，也不取代工具呼叫的 `project`。`--project-from-cwd` 依 server 啟動時的工作目錄選擇；若 client 不以 repo 目錄啟動 server，則無預設專案，只在 stderr 出現一行 warning。工作目錄的選擇已實測於 Claude Code 與 Codex CLI；Cursor、VS Code、AGY、Gemini CLI、OpenCode 未驗證（其中 AGY、OpenCode 工具呼叫 NOT_RUN，其餘未實測）。錯誤或未知的工作目錄只改變公告的預設專案，不改變權限；不確定時使用 `--default-project`。
21. `project add` 只追加新的 project 表，不修改、不刪除既有專案，Odin 也沒有移除專案的命令。新增 root 會擴大 host 的可讀範圍，輸出 `host_permissions_changed: true`；`--writable` 另授予寫入。root 不得位於 state 目錄內。目錄名稱不含 ASCII 英數字時需以 `--name` 指定。`--detect-languages` 只依標記檔寫入語言 guidance，`languages_installed` 為 NOT_PROBED。新專案沒有命令、shell 與 debug。
22. 索引 schema 2 升級：版本低於 2 的資料庫只刪除舊 `files`／`code_fts` 表並建立新索引表，新表於下一次 `index_query` 增量填入，`audit` 保留。版本高於本版的資料庫回 `SCHEMA_TOO_NEW` 且不被修改。較舊 binary 共用同一 state 目錄時，其 legacy 表不被新版查詢讀取。以上由 `store.rs` 單元測試覆蓋；未另做真實資料庫的跨版本升級實測。
23. 客戶端設定建議只列使用者層級位置（`default_outputs`）。Odin 不建議、也不生成專案層級檔案（`.mcp.json`、`opencode.json`、`.vscode/mcp.json`、`.cursor/mcp.json`、`.gemini/settings.json`）中的 entry：這些檔位於 repo 內，agent 可寫入時可能改指向 `--config`。若操作者自行把 Odin entry 放入此類檔，本專案的保護不涵蓋該檔的內容。host config 位於 project root 內時，host config 本身與其 `<config>.odin-*` 側檔（備份、lock、pending）對 agent 皆為 deny；但同目錄其他檔案仍依該 root 的 policy，建議把 host config 放在所有 project root 之外。

Transaction before/after snapshots 總量上限 32 MiB；來源快照、記憶與結果另有
配額。超限應縮小操作，不可覆寫衝突或移除配額取得成功。
逐需求狀態見 `verification/feature-evidence.json`。
