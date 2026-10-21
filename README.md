# Odin

## 獨立 Rust 程式碼工具服務 · Windows 原生預覽 0.2.0

Odin 提供 **85 個原生工具契約**，以 MCP stdio、本機 HTTP 或 CLI 供 Coding
Agent 使用。四個 Rust crates 共用專案政策、SHA-256 交易、直接 LSP／DAP、
Markdown 記憶與受限 JSON 流程。產品沒有 Serena bridge、Python REPL、
任意 eval 或自動上游 fallback。

這次已從未編譯的原始碼預覽，推進到 Windows x64 MSVC 的實際建置、測試與
二進位後端驗收。**40 項規格尚未全數驗收；目前不是跨平台穩定版。** 最新命令、
exit code、來源／binary hash 以 [交付狀態](verification/delivery-status.json)、
[本機驗收](verification/sandbox-ide-release-validation.json) 與
[逐需求證據](verification/feature-evidence.json) 為準。舊紀錄保留為歷史，
不能覆蓋最新結果，也不能把 protocol fixture 當真實語言支援。

### 可用功能

- 檔案、文字與行編輯；精確選擇 occurrence；多檔案交易的預覽、提交、回復與衝突拒絕。
- 直接 LSP 符號／引用／階層、篩選與分頁；rename，以及後端提供 edit 的 Move／Inline／QuickFix。
- 有序 WorkspaceEdit Create／Rename／Delete regular UTF-8 files，全部轉成 CAS 交易。
- Markdown 記憶、引用維護、版本化匯出／匯入、受權限及引用完整性保護的 scope move。
- 查詢前增量更新的 FTS5 詞彙索引；按 session 隔離、有配額的 JSON map/filter/reduce 流程。
- 具狀態與 session 檢查的 DAP、非阻塞 launch/restart、取消與固定 host argv 命令。
- Completion、hover、signature、呼叫階層、semantic tokens、inlay hints；格式化先產生 CAS 交易。
- 可選 Windows MXC OS 沙箱，固定 executor hash／檔案授權／網路封鎖，配置失敗就拒絕執行。
- 強制沙箱的 Clippy／Ruff／ESLint findings 與 safe fix preview；來源修改一律經 Transactions。
- 收緊權限的 context/mode、client setup 乾跑／備份、doctor、唯讀 Dashboard、trace 與期限結果分塊。

Move／Inline 是真正 LSP edit 的安全入口；後端未提供對應 action 時明確拒絕。
能力與限制詳見 [API](docs/API.md)、[核心](docs/CORE_IMPLEMENTATION.md)、
[協定](docs/PROTOCOL_IMPLEMENTATION.md)、[引擎](docs/ENGINE_IMPLEMENTATION.md)、
[平台](docs/PLATFORM_IMPLEMENTATION.md)。

### 建置與執行

需要 Rust **1.90.0**、rustfmt、Clippy 與 Windows MSVC C/C++ linker。
`Cargo.lock` 已由真實 Cargo 產生並納入版本控制，一般建置使用 `--locked`。
Python 僅用於開發驗收；Odin 的檔案／記憶功能不需要 Python 或 Serena。

```powershell
cargo build --workspace --release --locked
.\target\release\odin.exe metadata
.\target\release\odin.exe languages
.\target\release\odin.exe --config C:\Odin\host.toml init --root . --name workspace
.\target\release\odin.exe --config C:\Odin\host.toml validate
.\target\release\odin.exe --config C:\Odin\host.toml serve --transport stdio
```

先建立 `C:\Odin`，或選擇自己管理的 host 目錄。`init` 拒絕覆寫，預設不授予
write、shell、debug。命令與除錯需操作者在可信任 host 設定明確開啟；不要把
不可信儲存庫的檔案當可執行 host 配置。

```powershell
# 只輸出設定預覽，不修改現有 client 設定。
.\target\release\odin.exe --config C:\Odin\host.toml setup --client codex
.\target\release\odin.exe --config C:\Odin\host.toml doctor --probe-lsp
.\target\release\odin.exe --config C:\Odin\host.toml doctor --probe-sandbox
```

`setup` 支援 Codex、Claude、OpenCode。真正寫入需 `--apply --output`；
既有檔案還需 preview hash，並保存備份。生成 entry 的 MCP handshake
已有獨立 smoke，不表示已在每個真實 client 應用中完成安裝。

### 來源修改：先看，再寫

1. `read_file` 取得內容與 SHA-256。
2. 編輯工具或 `transaction_prepare` 建立 immutable preview，尚不修改來源。
3. 審查 `transaction_preview`，再 `transaction_apply`。
4. hash 不符就拒絕；中斷以明確 recover 處理，不覆寫第三方編輯。
5. 實際執行測試，檢查 `success`／`exit_code`；handler 成功不等於測試成功。

記憶匯入／scope move 使用專屬 `memory_transaction_preview/apply`，綁定發起
project。交易是逐檔原子替換與可恢復日誌，沒有多檔瞬間原子可見性。

### 本機 HTTP

以安全亂數在環境變數 `ODIN_TOKEN` 設定 32–512 visible ASCII bytes token，
執行 `serve --transport http`，開啟 `http://127.0.0.1:9280/`。資料 API 與 MCP
要求 Bearer，檢查 Host／Origin，僅綁 loopback。頁面不持久保存 token。
這不是 OAuth、公網或多租戶服務。

### 驗證與封裝

```powershell
.\scripts\verify.ps1
.\scripts\package.ps1
```

驗收包含 fmt、workspace tests、嚴格 Clippy、release build、CLI、stdio、HTTP
與 MCP 取消。原有三種真實 LSP 及 debugpy 端到端驗收可使用
`tests/real_backends.py --help` 指定已安裝後端重跑。TypeScript language-server
6.x 需搭配 TypeScript 6 的 tsserver。新增 `tests/real_ide_backends.py` 提供六語、
每語兩專案的獨立驗收，`tests/real_native_inspections.py` 測實際 MXC 與三種
analyzer。提供測試程式不代表測試通過；後端版本、範圍及 binary digest 記在驗收報告。

目前候選的來源、binary、逐語言與安裝包結果列在
[交付狀態](verification/delivery-status.json)。每份報告只驗證其綁定的來源／執行檔；
歷史 PASS 不替代新候選驗收。Windows AppControl 4551 阻止啟動的測試保留
實際非零結束碼；其餘獨立檢查可以繼續執行，不能因此把完整 gate 改為 PASS。
未提供的後端能力與 TypeScript／Java 無版本診斷不算驗證通過。
詳見 [語言設定與驗收](docs/LANGUAGE_BACKENDS.md) 及
[語言支援範圍](docs/LANGUAGE_SUPPORT.md)。

Python 可由 host 明確選擇 [Ruff stdin formatter](docs/NATIVE_FORMATTERS.md)，
在原生沙箱產生 CAS 預覽；沒有自動 LSP fallback。OS 路徑查詢可另行明確配置
enumeration-only grants，不會自動授予父目錄內容讀寫權限。

Windows 使用 portable ZIP／Inno Setup 7，Linux 使用原生 `.deb`／portable tar；
見 [編譯、封裝與生命週期驗證](docs/PACKAGING_WINDOWS_LINUX.md)。安裝範例的
所有執行及寫入權限預設關閉。編譯、安裝測試、乾淨主機、跨版本升級、
Defender/CFA 與中斷回復是不同驗收；Linux VM 的結果也不代表完整 Linux IDE
或 Linux OS 沙箱支援。macOS 與性能比較尚未驗收。
CI 僅手動 self-hosted，推送不啟動 hosted CI。

### 邊界與後續工作

Windows OS 沙箱需明確配置 native MXC 與最小檔案授權；見
[設定範例](examples/odin.sandbox.toml) 與 [沙箱邊界](docs/OS_SANDBOX.md)。
未配置的外部程序只有 Job／process group 清理。`odin languages` 的 28 個
profile 是設定指引，不是 28 語驗收成功。IDE 功能依 backend capabilities
回傳支援或明確錯誤；參見 [IDE 契約](docs/IDE_IMPLEMENTATION.md)。
動態 safe delete、完整 watcher、跨平台／客戶端與 benchmark 矩陣仍有缺口。

[已知限制](docs/KNOWN_LIMITATIONS.md) · [安全邊界](docs/SECURITY.md) ·
[架構](docs/ARCHITECTURE.md) · [40 項規格](docs/FEATURE_SPEC.md) ·
[工作分派](docs/dispatch-language-installers-v3.md) · [MIT](LICENSE) ·
[第三方授權文字](docs/THIRD_PARTY_LICENSES.txt)
