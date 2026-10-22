# Home 預設 root 與安裝最後階段

新 Windows host 預設 root 為安裝使用者的 home（`%USERPROFILE%`），固定可信設定檔為 `home/.config/odin/host.toml`。CLI 省略 `--config` 也使用此固定路徑，不從 cwd 尋找 `odin.toml`。

`odin init` 省略 `--root` 時建立唯讀 workspace template：`default_root` 是實際 canonical home，`allowed_roots` 包含 home。它不建立 `projects.home`。明確 `init --root PATH` 保留登記專案的行為；`--writable` 必須同時明確提供 `--root`。init 拒絕覆寫已存在的設定。

```powershell
# 不指定 root：使用 home，所有執行與來源寫入權限關閉。
odin init
odin validate
odin serve --transport stdio
# 每次依 client 的 cwd 選專案；不同專案有不同 ID／state。
odin --workspace . serve --transport stdio
```

省略 `--workspace` 時，只有 host 明確宣告 `workspace.default_root` 才使用此預設；未宣告的舊設定保留原有多專案行為。明確的 `--workspace .` 依 launch cwd 選取專案，不被 default_root 覆蓋。不同磁碟的專案仍需操作者加入 `allowed_roots`；不自動授權任意位置。

## Home 的私有路徑

Home 包含應用設定與憑證，唯讀本身不會阻止洩漏。新 home policy 強制排除 host 設定所在目錄（含所有側檔、備份、receipt）、state、實際執行檔／安裝目錄，以及 `.config`、`.local`、`AppData`、`.codex`、`.claude`、`.ssh`、`.aws`、`.azure`、`.gnupg`、`.kube`、`.docker` 和常見 credential dotfiles。`workspace.protected_roots` 只能增加拒絕範圍，不能移除強制保護。

原生 SafeRoot 的讀取、列舉、搜尋、索引與 Transactions 共用這些拒絕規則；私有樹及其後代不能直接被選為 workspace。符號連結、reparse、hardlink 與 Windows identity 檢查保留。Home 暫態 view 亦排除已登記子專案，以保留那些專案的個別政策。

專案內 host 的窄例外僅限 OS home 下的固定 `.config/odin/host.toml`，其路徑不得經連結／reparse redirect；任意 repository 的 `odin.toml` 仍不能自證為可信 host。舊的未啟用 home policy 的顯式設定保留原 exact-host-file／state 保護。

`setup --apply` 只對固定 Codex／Claude／OpenCode 設定檔提供必要的 host-output 例外，仍需明確命令、預期 hash 與備份。Hook 或任意 home／其他專案輸出沒有這個例外。Go／C#／Lua runner 在 home cwd 使用 state 的例外也會核對固定 home host、default_root 與保護區；Lua 仍只接受固定內容的純 JSON 設定，不執行 repository Lua 設定。

這是 Odin 原生工具的路徑政策。LSP 是 host 程序，沒有因此自動啟用 OS sandbox；後端本身的檔案掃描與專案行為不由 deny globs 完全隔離。其餘 home 一般檔案在操作者授權的唯讀範圍內。

## 安裝最後階段

檔案複製完成後，安裝器使用獨立 LSP 檢查頁顯示真實階段：完整 payload hash、19 個 LSP／runtime 版本探測、再次完整性驗證、提交 host profiles。進度依實際檔案／元件計數，不以計時器模擬完成。固定 Node child 以受控環境啟動；UI 輪詢私有、具 run ID 的有界資料檔，持續處理視窗訊息。

每個版本命令最多 60 秒，worker 預設總驗證期限 900 秒。操作者可用 `/ODIN_SETUP_TIMEOUT_SECONDS=1..1800` 指定整數秒。『停止設定檢查』採協作停止：目前版本命令最多等待其剩餘期限；驗證取消或逾時不進入 host apply。通過最後檢查後，短暫的 expected-hash 提交階段不接受中止，避免強殺造成部分多檔替換。Job object 用於程序生命週期清理，不以強殺承諾原子回復。

停止／失敗可能已複製應用檔案；它不代表回滾整個 Inno 安裝。失敗完成頁與 silent installer exit code 會保留失敗狀態（取消 1223、逾時 1460），不能把複製完成當成 LSP 設定成功。安裝器成功頁須同時取得 worker 成功退出與同 run 的 done 訊息。

新 host 的 root 欄位預填 home，仍可明確改成其他既存目錄。重裝既有 host 且未給新的 root 時保留原 roots/default/grants；若操作者要切換既有預設，可在欄位或 `/ODIN_WORKSPACE_ROOT=` 明確選 home。隨附語言 profiles 的自訂衝突仍需個別決定；操作者自訂的 Rust／`.rs` profiles 始終保留，不能覆寫衝突取得成功結果。

版本／安裝檢查不等於語意支援或完整 IDE 驗收。本輪結果與未執行項見 `verification/home-root-installer-validation.json`；跨平台、乾淨 Windows、跨版本資料遷移與崩潰回復仍需各自證據。
