# Odin

## 獨立 Rust 程式碼工具服務 · Windows 原生預覽 0.2.0

Odin 提供 **72 個原生工具契約**，以 MCP stdio、本機 HTTP 或 CLI 供 Coding
Agent 使用。四個 Rust crates 共用專案政策、SHA-256 交易、直接 LSP／DAP、
Markdown 記憶與受限 JSON 流程。產品沒有 Serena bridge、Python REPL、
任意 eval 或自動上游 fallback。

這次已從未編譯的原始碼預覽，推進到 Windows x64 MSVC 的實際建置、測試與
二進位後端驗收。**40 項規格尚未全數驗收；目前不是跨平台穩定版。** 最新命令、
exit code、來源／binary hash 以 [交付狀態](verification/delivery-status.json)、
[本機驗收](verification/local-validation.json) 與
[逐需求證據](verification/feature-evidence.json) 為準。舊紀錄保留為歷史，
不能覆蓋最新結果，也不能把 protocol fixture 當真實語言支援。

### 可用功能

- 檔案、文字與行編輯；精確選擇 occurrence；多檔案交易的預覽、提交、回復與衝突拒絕。
- 直接 LSP 符號／引用／階層、篩選與分頁；rename，以及後端提供 edit 的 Move／Inline／QuickFix。
- 有序 WorkspaceEdit Create／Rename／Delete regular UTF-8 files，全部轉成 CAS 交易。
- Markdown 記憶、引用維護、版本化匯出／匯入、受權限及引用完整性保護的 scope move。
- 查詢前增量更新的 FTS5 詞彙索引；按 session 隔離、有配額的 JSON map/filter/reduce 流程。
- 具狀態與 session 檢查的 DAP、非阻塞 launch/restart、取消與固定 host argv 命令。
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
與 MCP 取消。另有三種真實 LSP 及 debugpy 端到端驗收；使用
`tests/real_backends.py --help` 指定已安裝後端重跑。TypeScript language-server
6.x 需搭配 TypeScript 6 的 tsserver。後端版本及 binary digest 記在驗收報告。

Windows 提供 portable ZIP 與 Inno Setup recipe。安裝、同版重裝、卸載有隔離
smoke；乾淨主機、跨版升級、Defender/CFA、取消回復另行驗收。Linux/macOS
原生驗收與性能比較維持 NOT_RUN。CI 僅手動 self-hosted，推送不啟動 hosted CI。

### 邊界與後續工作

Windows Job Object／Unix process group 用於程序清理，**不是 OS 安全沙箱**。
Job 在 spawn 後指派，有啟動窗口。不可信任程式庫或後端仍需外部受限帳號／
VM／容器。進階語言適配、自有 inspection providers、動態 safe delete、完整
watcher、跨平台／客戶端與 benchmark 矩陣未被標成全部完成。

[已知限制](docs/KNOWN_LIMITATIONS.md) · [安全邊界](docs/SECURITY.md) ·
[架構](docs/ARCHITECTURE.md) · [40 項規格](docs/FEATURE_SPEC.md) ·
[工作分派](docs/dispatch-plan.md) · [MIT](LICENSE) ·
[第三方授權文字](docs/THIRD_PARTY_LICENSES.txt)
