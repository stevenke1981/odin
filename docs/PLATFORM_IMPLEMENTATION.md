# CLI、HTTP 與本機封裝

Odin 的 CLI、MCP stdio、HTTP 共用 engine；平台入口不直接修改專案內容。`metadata`、`tools` 不需要設定；`validate`、`setup`、`hook` 不建立 engine state，也不啟動後端。

## CLI

```powershell
odin metadata
odin --config C:\Odin\host.toml validate
odin --config C:\Odin\host.toml --context reviewer --mode readonly validate
odin --config C:\Odin\host.toml doctor --probe-lsp
odin --config C:\Odin\host.toml onboard --project workspace
```

`metadata` 回傳版本、編譯時 Git revision、tracked source dirty flag、Rust compiler、target/profile。這些是建置識別，不是通過測試、簽章或供應鏈證明。沒有 Git metadata 的來源封存須保留 UNKNOWN。`--context`、`--mode` 只套用 host 宣告的 data-only policy，不能增加 host 已拒絕的權限。

doctor 預設只判斷 configured program 是否存在，明確保留 integration NOT_RUN；`--probe-lsp` 透過 engine 的 `get_capabilities` 執行真正 initialize handshake，缺程式、協定失敗或不 ready 時回非零 exit。握手不是 indexing 完成或語言全功能證明。DAP 整合與 MCP transport probe 分開驗收；doctor 不為檢查寫入權限而修改工作區。

onboard 讀取 bounded project evidence；不執行 README、AGENTS.md 或其他儲存庫內容。使用者仍須判讀 evidence 和實際執行建置／測試。

## 客戶端與 hooks

```powershell
odin --config C:\Odin\host.toml setup --client codex
odin --config C:\Odin\host.toml setup --client claude --output C:\Odin\client.json
odin --config C:\Odin\host.toml setup --client opencode --output C:\Odin\opencode.json
```

預設回 DRY_RUN JSON，`content` 是生成的 managed entry；不顯示既有無關設定值。三種輸出分別採 Codex `mcp_servers.odin` TOML、Claude `mcpServers.odin` JSON、OpenCode `mcp.odin` local argv 陣列。argv 使用獨立字串，包含絕對 binary／host config 路徑與選定 context/mode。設定格式依 [Codex 官方文件](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)、[Claude Code 官方文件](https://code.claude.com/docs/en/mcp)、[OpenCode 官方文件](https://opencode.ai/docs/mcp-servers/)。

要寫入明確指定的 host file，使用 `--apply --output <path>`。新檔只允許 create_new；既有檔須指定 preview 回傳的 `--expected-sha256 <hash>`。寫入前再比對原內容，先保存 `<name>.odin-backup-<full-sha256>`，然後同目錄暫存檔＋rename。不同設定值會保留；JSON/TOML 的空白、排序、註解不保證原樣。JSONC 明確拒絕，不能先刪註解再假裝無損。寫入不會授權 Odin shell/debug/project writes。這條 host setup 路徑拒絕 configured project root 內的輸出，工作區改動仍走 Transactions。

symbolic-link output 被拒絕；setup 專用 create_new lock 防止合作中的 setup 程序互撞。外部、不使用該 lock 的寫入仍有最後比對與 rename 間的競態窗口；此 host 設定寫入機制不宣稱 OS sandbox 或跨程序完整交易。中斷留下的 `.odin-setup.lock` 需 operator 檢查，不自動強行回收。備份可能包含原有 secrets，應保持 host storage 私有。

`hook --project workspace` 產生短提示，並不自動註冊到任何代理。`hook --output <host-file>` 預覽 managed marker 區塊；明確 apply 同樣要求 CAS 與備份。`--disable` 只移除自己的完整 marker 區塊，保留前後文字；不完整或重複 marker 會拒絕。子 agent 沒有自己的 MCP 連線時，不宣稱共享工具。hook 內容沒有 shell script、eval 或 repository-supplied executable configuration。

生成設定的 smoke 會實際執行該 entry 的 command，完成 initialize 和 tools/list。這證明 generated argv 與 Odin 的連線；不是已在真正 Codex／Claude／OpenCode 應用中完成整合。

## HTTP 與 Dashboard

HTTP 只綁 loopback，要求 32–512 visible ASCII bytes 的 Bearer token。`/mcp` 和所有資料 API 共用 Host、Origin、Bearer、8 MiB body／10 秒讀取上限。未驗證 token 不得讀專案資料。

| API | 資料 |
|---|---|
| GET `/api/metadata` | Binary build identity，model token usage UNKNOWN |
| GET `/api/projects` | 設定的 project boundaries／permission flags |
| GET `/api/status` | 現有 capability snapshot，不啟動 LSP；backend 狀態與 policy 白名單 |
| GET `/api/audit/{project}` | 最近 100 筆 metadata-only audit |
| GET `/api/audit/{project}/export` | 相同資料的 JSONL |

Dashboard 僅提供讀取和下載；操作紀錄允許欄位為時間、project、tool、ok、elapsed_ms、error_code、trace_id（若有）。不輸出 arguments、fingerprints、code、後端 error message 或 command/env。build／status 明列 UNKNOWN，沒有推算 LLM token／resource usage。token 只留頁面記憶體，不放 URL、storage 或下載連結。資料 API no-store；靜態頁面採 no-store、no-referrer、nosniff 和限制來源的 CSP。

這不是公網／OAuth／多租戶服務。status 無法取代實際 LSP／DAP 功能測試，稽核也不是完整 OpenTelemetry export。process RSS／model token 數目前沒有來源，不會虛構。

MCP `notifications/cancelled` 會取消 handler 等待的 engine future；同時收到取消與 gate／backend 可用時，先處理取消。仍排隊等待 project gate 的寫入會被丟棄，pending LSP／DAP request 依 protocol 的 drop guard 傳遞取消。已開始的同步 transaction apply 無法由 async cancellation 中斷；取消不代表 rollback，也不證明沒有改檔。handler 使用 -32800／REQUEST_CANCELLED 與 UNKNOWN_IF_STARTED 說明，操作者應先核對 journal、phase 與檔案 hash 再決定是否重試。rmcp 3.6 對已取消的 request ID 會抑制 wire response，不能以沒有 response 推定交易結果。`mcp_cancel_smoke.py` 以真 binary 加明確設定的慢 LSP 協定 fixture 驗證取消排隊寫入、Draft journal 保持不變及 pending backend 取消；此 fixture 不是語言語意支援的證據。

## 打包與驗證

```powershell
.\scripts\package.ps1
# 已有固定 binary 時可指定 -SkipBuild -Binary <path>；不覆寫既有 zip。
.\scripts\verify_installer.ps1 -Installer <setup.exe> -ExpectedBinarySha256 <binary-hash>
```

Windows script 以 --locked release build 產生 ZIP、per-file SHA-256 inventory 和 build metadata；會在 PATH、Program Files 6/7 與 uninstall registry 偵測已安裝的 Inno Setup，也接受明確 `-Iscc`。不自動下載工具。找不到 compiler 時 installer_compile 維持 NOT_RUN；portable 成功不會改稱 installer 成功。

installer 為每使用者安裝，不改 PATH、MCP client config 或 Odin policy。個人設定／state 由操作者放在安裝目錄外；卸載不遞迴刪除使用者新增檔案。QA 使用明確 `/ODIN_QA=1`、新建 dist 子目錄與不註冊 uninstall key 的模式；實測安裝、同版重裝、binary hash、metadata 和卸載後保留使用者檔案。中斷／取消 rollback、跨版升級與乾淨機器仍分開記錄，不由此 smoke 推論通過。安裝產物未簽章。

Linux/macOS 用 `bash scripts/package.sh [output-dir]` 在實際平台編譯並建立 tar.gz。Windows 不執行這些平台的 binary；沒有 runtime 證據時維持 NOT_RUN。

`cli_smoke.py`、`mcp_smoke.py`、`http_smoke.py` 都只接受真實 binary，記錄其 SHA-256。兩份 catalog 及 live tools/list 的 name／schema 必須一致，工具數量不硬編碼。HTTP smoke 真的執行 initialize、initialized、tools/list、read_file，驗證 auth、metadata allowlist 與 JSONL；不是完整 MCP conformance suite。`check_source.py` 只聲明本檢查沒有編譯／執行 Rust，不覆寫其他真實 command 的證據。

公開發布 scripts 預設 dry-run，執行時需明確 reviewed SHA、乾淨 main、正確帳號／origin、requirements／manifest 檢查及 manual self-hosted workflow policy。它們不自動 stage／commit／force-push／啟 CI，推送後再核對 public visibility 和 remote HEAD。
