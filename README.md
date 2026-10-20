# Odin
## Rust Code Intelligence · Standalone Source Preview 0.2.0

Odin 是以 Rust 編寫、供程式代理使用的本機工具服務。交付包含 **4 個 Rust crates、58 個工具契約、交易式修改、直接 LSP、直接 DAP、Markdown 記憶、MCP 與唯讀 Dashboard。程式沒有 Serena 橋接與自動回退**。

> **交付狀態：原始碼預覽，不是已驗收的正式軟體。**
> 本次交付環境沒有 `rustc` / `cargo`，因此 Rust 編譯、Rust 測試、真實 MCP / LSP / DAP 整合及 Windows 執行均為 **NOT_RUN**。沒有提供執行檔，也沒有虛構 `Cargo.lock`。實際執行的檔案／契約檢查與 Python fixture 自測在 `verification/`；它們不能代替 Rust 編譯或產品驗收。

### 不依賴 Serena，與全功能等價是兩件事

這份版本不需要安裝、啟動或呼叫 Serena，也不需要它的 JetBrains 外掛。`bridge.rs`、`BridgeConfig`、`serena_tools`、`serena_call`、橋接程序管理與範例已移除；舊的 `[projects.<id>.serena]` 設定會因未知欄位而被拒絕，沒有隱藏轉發。

語意功能由 Odin 自己的 Rust LSP client 直接連接語言伺服器；除錯由 Rust DAP client 直接連接除錯配接器。檔案、記憶與詞彙搜尋不需這兩種後端。Odin 核心不需 Python runtime；Python 僅用於交付檢查／測試，外部語言伺服器或除錯器可能有自己的 runtime 要求。

**本次移除了相依，沒有把原先 bridge-only 的功能全部補寫完成。** 語意 Move／Inline、JetBrains inspections、任意 Python REPL、跨語言專用適配與完整設定相容仍有缺口。沒有用一般文字替換冒充進階重構；也沒有宣稱已編譯、已通過整合或比 Serena 更快。

變更與遷移詳見 `docs/STANDALONE.md`；完整差異在 `docs/SERENA_PARITY.md`。


## 完整待實作規格（公開開發契約）

Odin 0.2.0 的**40 項必需／擴充需求**已拆成可驗收的規格（P0/P1/P2），每項都有相依、測試案例 ID 與 DoD。這些是**待實作工作清單**，不是已完成的功能。對照 Serena 53 項參考工具只為覆蓋分析，Odin 本身沒有任何 Serena 依賴。

- [必需功能逐項規格](docs/FEATURE_SPEC.md)
- [53 項參考功能 → Odin 原生實作映射](docs/SERENA_COVERAGE_PLAN.md)
- [開發與驗收 Backlog](docs/IMPLEMENTATION_BACKLOG.md)
- [核心協定 RFC](docs/specs/CORE_PROTOCOL.md)
- [交易／Rename／Move／Inline RFC](docs/specs/REFACTOR_TRANSACTION.md)
- [語言、LSP、DAP、Inspections RFC](docs/specs/LANGUAGE_DEBUG_INSPECTIONS.md)
- [安全、部署與發行 RFC](docs/specs/OPERATIONS_RELEASE.md)
- [機器可讀 JSON 需求](requirements/features.json)；執行 `python scripts/check_requirements.py` 檢查覆蓋。

## 目錄導讀

| 內容 | 位置 |
|---|---|
| 架構、資料流、模組界線 | `docs/ARCHITECTURE.md` |
| 58 個工具與參數契約 | `docs/API.md`、`schemas/tools.json` |
| Serena 逐項覆蓋與差異 | `docs/SERENA_PARITY.md`、`docs/serena-parity.csv` |
| 安全邊界與已知限制 | `docs/SECURITY.md`、`docs/KNOWN_LIMITATIONS.md` |
| 獨立原生使用範例 | `examples/`、`docs/USAGE.md` |
| 建置與驗收腳本 | `scripts/bootstrap.*`、`scripts/verify.*` |
| 證據與待完成門檻 | `verification/`、`docs/RELEASE_GATES.md` |

## 建置

需要 Rust 1.90.0 toolchain（含 rustfmt / clippy）、Python 3.11+、網路存取 Cargo registry，以及平台原生 C/C++ linker。Windows 使用 MSVC 建置環境；本次沒有實測安裝或編譯。

```powershell
# 在解壓後的 odin 目錄；腳本不執行系統套件安裝命令；rustup shim 可能依 rust-toolchain.toml 下載缺少的 toolchain。
.\scripts\bootstrap.ps1
.\scripts\verify.ps1
```

```bash
# Linux / macOS
bash scripts/bootstrap.sh
bash scripts/verify.sh
```

首次 bootstrap 會解析相依並產生 **真實 Cargo.lock**、執行 rustfmt，再以 `--locked` 建置。任何失敗都會終止，不會改標成 PASS。請把第一次通過驗收的 lockfile 納入版本控制；目前包內只有直接相依的精確版本，尚未鎖定遞迴相依。

手動步驟：

```bash
cargo generate-lockfile
cargo fmt --all
cargo test --workspace --locked
cargo build --workspace --release --locked
```

## 第一個原生工作區

先完成建置，再執行：

```powershell
.\target\release\odin.exe --config odin.toml init --root . --name workspace
.\target\release\odin.exe --config odin.toml doctor
.\target\release\odin.exe --config odin.toml call list_dir --project workspace
.\target\release\odin.exe --config odin.toml serve --transport stdio
```

`init` 拒絕覆寫既有設定，預設停用原生寫入、Shell 與除錯。需要編輯時，在可信任 host 設定開啟 `allow_write`，或初始化新設定時明確傳 `--writable`。**這不是 OS 沙箱**；LSP 子程序本身仍可能執行建置腳本。

純檔案功能不需語言伺服器。符號功能另需安裝 `rust-analyzer`，或設定其他實作 LSP 的伺服器；Odin 沒有打包／自動下載語言伺服器，沒有宣稱已驗證四十多種語言。

## 修改檔案：先看，再寫

1. `read_file` 取得內容與 SHA-256。
2. `replace_content`、`replace_symbol_body` 或 `transaction_prepare` 產生 immutable journal + diff，**此時不修改原始碼**。
3. 審查 `transaction_preview`，再呼叫 `transaction_apply`。
4. apply 前重新核對全部檔案；版本不符則拒絕。中途中斷可明確執行 `transaction_recover`。
5. 執行授權的測試 profile 並檢查 `success` / `exit_code`。**工具成功不等於測試通過。**

多檔案操作是「逐檔原子替換＋可恢復日誌」，不是整個 changeset 對外瞬間原子可見，也不是分散式交易。不可把此機制當作備份或 Git 的替代品。

## 本機 HTTP 與 Dashboard

```powershell
# 使用 PowerShell/.NET 密碼學亂數產生 token；不寫入命令列參數。
$bytes = New-Object byte[] 32
$rng = [Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$rng.Dispose()
$env:ODIN_TOKEN = [Convert]::ToBase64String($bytes)
.\target\release\odin.exe --config odin.toml serve --transport http
```

瀏覽 `http://127.0.0.1:9280/`。Dashboard 的 token 只存在頁面記憶體；MCP 與資料 API 需要 Bearer。HTTP 僅綁 loopback，檢查 Host / Origin；沒有 OAuth、多租戶或公網部署支援。

## 從 0.1.0 遷移

保留舊設定與資料備份，再移除 host 設定中的 `[projects.<id>.serena]` 區塊；客戶端移除 `serena_tools` / `serena_call` 呼叫。不要把舊工具直接改名當作同等能力。原生 MCP 入口與其餘工具名稱保持不變；舊 state 格式沒有刻意更動，但跨版本資料遷移仍未做執行驗證。

同名 `serena_*` 呼叫在本版應回 UNKNOWN_TOOL／MCP method-not-found；缺少 LSP capability 仍回明確錯誤，不重新啟動上游。

## 已採取的設計方向

每次呼叫明確帶專案 ID；設定檔拒絕重疊專案根目錄。原生寫入集中經過 SHA-256 / journal；命令只能選 host 設定的固定 argv profile。原生批次介面使用 bounded JSON dataflow，而不是 Python eval。大型結果留在本機分塊讀取，稽核只記工具、輸入摘要與結果狀態。

這些是設計與原始碼層的取捨，**沒有與 Serena 進行速度、記憶體、準確率或 token 使用量的實測比較**。

## 授權

此交付新增的 Odin 原始碼採 MIT，未內嵌 Serena 應用程式原始碼。Rust crates 與自行安裝的語言伺服器／除錯器遵守各自授權；參考 `THIRD_PARTY_NOTICES.md`。這不是正式法律審查或商用散布合規認證。

## GitHub 公開發布

本交付未取得建立 GitHub Repository 的 API 權限，**尚未發布到 GitHub**。使用已登入 `stevenke1981` 的本機 `gh`，執行 `scripts/publish_public.ps1`（Windows）或 `scripts/publish_public.sh`（Linux/macOS）即可在保護性檢查後建立**新的公開** `stevenke1981/odin` 並推送完整原始碼。請先閱讀 [公開發布與阻礙說明](docs/PUBLISH_GITHUB.md)。
