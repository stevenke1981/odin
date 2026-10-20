# 操作、安全與發行規格 (RFC-OPS-001)

**地位：設計目標，非完成公告。** 參閱 ODIN-001/002/011/012/023/024/025/027/028/029/030/031/032/033。

## 安全模型（P0，不可跳過）

攻擊者可能控制 workspace 檔案與 symlinks、untrusted build scripts、LSP stderr/stdout、Agent 提示詞、HTTP 請求、競爭的編輯程序。防護包括拒絕未知 root、嚴格工具分權、路徑 handle 防競態、body/output 大小限制、OS 級 process isolation、network loopback、cache 及日誌保密。動態工具使用 **最小權限，失敗時拒絕（fail closed）**。

允許 LSP/Debug 啟動時需要檢查其 program、argv、working dir、env、來源版本。不能因為 `--readonly` 就允許不可信 server 讀寫本機任意路徑。Host 機器未受信任時需實際 Windows Job Objects/OS sandbox 或隔離容器，不假設 Rust memory safety 會阻止惡意 process。

## 發行證據

- v0.2.0 所有舊的靜態測試與 mock 結果僅標為 `SOURCE_PREVIEW`。
- Windows x86_64 MSVC、Linux x86_64、macOS arm64 均需實機 `cargo build/test`。
- 各語言 LSP+DAP 要記錄 server version、OS、命令、exit code、可重現 fixture。
- 相容性測試含 clean machine **without Python/Serena**。Python 只可存在於 dev test harness，不能為 Odin runtime mandatory。
- `cargo audit`/SBOM/secret scan/license 檢查須成功，不得略過已知漏洞。
- 編譯、單元、整合、安裝/卸載、升級、異常關機、可復原、無孤兒程序均經驗收後才能標 Beta/Stable。

## GitHub Actions 成本策略

只保留 `workflow_dispatch`，使用可信 self-hosted runner；不能讓不受信任公眾 PR 自動執行任意腳本，亦不得擅自啟動 GitHub-hosted 額度。記錄 runner identity、開始/結束、commit、artifact hash。PR 可先在 review 無 CI 狀態下分析，合併之前再手動驗證。

## 客戶端整合

Codex、Claude Code、OpenCode setup CLI 需在變更設定前生成 diff 與備份；僅對明確授權的 profile 安裝 MCP entry；不能把 JSON 產出宣稱已接線成功，必須工具列舉成功。Uninstall 僅回復自己新增的設定。

## Windows Inno Setup

需安裝、再安裝、升級、靜默安裝、修改安裝路徑、保留個人 config/state、取消與 rollback、沒有 Serena 時執行、卸載資料保護等 lifecycle tests。未執行 Windows 原生 compiler + installer 不能發穩定 EXE。
