# 自動載入 Coding Agent（client setup）

本文說明如何讓 Coding Agent 像 Serena 一樣，在啟動時自動連上 Odin：client 自行啟動 `odin serve`（stdio），agent 從 MCP instructions 得知本次 session 的預設專案。

## 0. 邊界

- 工具 schema 未改；每次工具呼叫仍應帶 `project`。單一已配置專案時，既有 engine 會沿用該專案作為回退，多專案時必須明確指定。
- session 預設專案只在 server 啟動時決定一次，只能選 host config 已有的專案，只出現在 MCP instructions，不授予任何權限，也不建立 runtime root 或可變 active project。
- Odin 不啟動 client CLI，也不修改 client 設定；只有加上 `--apply --output` 才會寫檔。
- 生成的 entry 含 Odin 執行檔路徑、host config 路徑與 `serve` 參數，不含 token、環境變數或 shell 命令。
- 生成的 entry 與註冊指令使用一般 Windows 路徑，已去除 `\\?\` verbatim 前綴（同一 `display_path` 處理執行檔與 host config 路徑）。
- 只建議使用者層級（user scope）的 client 設定位置。專案層級檔案（`.mcp.json`、`opencode.json`、`.vscode/mcp.json`、`.cursor/mcp.json`、`.gemini/settings.json`）位於 repo 內：若 agent 可寫入該 repo，agent 就能把 `--config` 改指向儲存庫檔案，或改寫其他 server 設定。因此 `setup` 的 `default_outputs` 不列出任何專案層級位置，本文件也不建議把 Odin entry 放進專案層級檔案。
- `--project-from-cwd` 依 client 啟動 server 的工作目錄選擇預設專案。工作目錄錯誤只改變公告的預設專案，不改變任何權限。
- 本文只涵蓋 stdio。HTTP 模式不由 `setup` 生成 entry。

## 1. 流程

```powershell
# 1) 複製到固定路徑；setup 會把這個路徑寫入 entry，請從此路徑執行 setup
New-Item -ItemType Directory -Force C:\Odin\bin | Out-Null
Copy-Item .\target\release\odin.exe C:\Odin\bin\odin.exe

# 2) 建立 host config（只限首次；init 拒絕覆寫，預設不授予 write、shell、debug）
C:\Odin\bin\odin.exe --config C:\Odin\host.toml init --root C:\work\app-a --name app-a

# 3) 加入其他 repo：先乾跑，檢查 content 與 expected_sha256
C:\Odin\bin\odin.exe --config C:\Odin\host.toml project add --root C:\work\app-b
C:\Odin\bin\odin.exe --config C:\Odin\host.toml project add --root C:\work\app-b --apply --expected-sha256 <乾跑回傳的 expected_sha256>

# 4) 為各 client 產生 entry（預設乾跑，不寫入）
C:\Odin\bin\odin.exe --config C:\Odin\host.toml setup --client codex --project-from-cwd
```

`project add` 的行為：

- 只追加一個新的 `[projects.<id>]` 表，保留原檔位元組；寫入前把結果以與 `Config::load` 相同的規則完整解析，重疊根目錄等錯誤會在寫入前拒絕。
- 寫入前建立 `<config>.odin-backup-<hash>`；既有檔案需要 `--expected-sha256`，不符即 `CONFLICT`。
- 新專案 `allow_shell=false`、`allow_debug=false`、無命令；`allow_write` 只有加 `--writable` 才為 true。
- 新增 root 會擴大 host 的可讀範圍（`--writable` 另授予寫入）。成功輸出（乾跑與套用）含 `"host_permissions_changed": true`、`"allow_write"`、`"project"`，並移除 `setup` 的 `client_connection` 欄位；`note` 說明只追加一個表、既有文字逐位元組保留，且需重啟 Odin server 才會載入。
- `--name` 預設為 root 目錄名（小寫，無效字元改為 `-`），並經 `validate_id` 檢查；已存在的 ID 拒絕。目錄名稱不含 ASCII 英數字時（例如 `專案` 得到 `--`），直接回錯誤，請以 `--name` 指定。
- root 不得等於或位於 `state_dir` 之內（`Project root cannot be inside the state directory`）。此檢查在 config 解析時執行，`project add` 寫入前同樣適用。
- `--detect-languages` 只依 root 直接下的標記檔加入語言 guidance，輸出 `languages_installed: "NOT_PROBED"`，不代表已安裝或可用。

乾跑輸出中的 `expected_sha256` 為目前檔案 hash（新檔為 `null`），`content` 為要寫入的內容。

### host config 側檔保護

host config 若位於某個 project root 內，該 root 的 policy 會加入 deny glob，涵蓋 host config 本身與 `<config 檔名>.odin-*`（`.odin-backup-<hash>`、`.odin-setup.lock`、`.odin-pending-*` 等）。agent 經 Odin 讀寫這些檔案會被拒絕，這是預期行為。路徑中的字面部分會以與 `literal_glob` 相同方式跳脫。

### 寫入 client 設定檔

```powershell
# 乾跑：若目標檔已存在，expected_sha256 為其目前 hash
C:\Odin\bin\odin.exe --config C:\Odin\host.toml setup --client codex --project-from-cwd --output $HOME\.codex\config.toml

# 套用：既有檔案需提供 hash；新檔不要帶 --expected-sha256
C:\Odin\bin\odin.exe --config C:\Odin\host.toml setup --client codex --project-from-cwd --output $HOME\.codex\config.toml --apply --expected-sha256 <hash>
```

`setup` 的輸出為 `emit_or_apply` 的結果，另加 `client`、`default_outputs`（僅建議位置）、`registration_command`（argv 陣列，無則 `null`）與 `auto_project`（`project_from_cwd`、`default_project`）。

套用行為：

- `--output` 不可位於任何已配置 project root 內（`--output must be outside configured project roots`）。工作區內的檔案只能經 native Transactions 修改；Odin 不把專案層級 client 檔案當作建議位置，請勿把 Odin entry 放入 repo 內。
- 目標檔已存在時，乾跑的 `content` 只是 Odin 區段；需手動合併進既有設定。
- 既有檔案的原始位元組保存於 `<file>.odin-backup-<hash>`。寫入使用同目錄暫存檔再 rename。JSON／TOML 會重新排版，格式與註解可能改變。
- 另有 `<file>.odin-setup.lock` 防止併發；程序中斷後若殘留，請先確認沒有其他 setup 執行，再由操作者刪除。

## 2. 各 client 格式與路徑

使用者層級建議位置見下表；`setup` 的 `default_outputs` 只列這些位置，不列專案層級檔案。

| client | entry 格式（key 為 `odin`） | 使用者層級建議位置（`default_outputs`） | 註冊指令（Odin 不執行） |
|---|---|---|---|
| codex | TOML `[mcp_servers.odin]` 的 `command`、`args` | `$CODEX_HOME/config.toml`，未設定則 `~/.codex/config.toml` | `codex mcp add odin -- <bin> <args...>` |
| claude | JSON `mcpServers.odin = {type:"stdio", command, args}` | `~/.claude.json`（單一 user-scope 建議；note 說明 Claude Code 會持續改寫此檔，應優先使用註冊指令） | `claude mcp add --scope user odin -- <bin> <args...>` |
| opencode | JSON `mcp.odin = {type:"local", command:[bin,...args], enabled:true}` | `~/.config/opencode/opencode.json` | 無 |
| agy | JSON `mcpServers.odin = {command, args}` | `~/.gemini/antigravity/mcp_config.json`（本機已確認） | `agy mcp add odin -- <bin> <args...>` |
| gemini | JSON `mcpServers.odin = {command, args}` | `~/.gemini/settings.json` | 無 |
| cursor | JSON `mcpServers.odin = {command, args}` | `~/.cursor/mcp.json` | 無 |
| vscode | JSON `servers.odin = {type:"stdio", command, args}` | Windows `%APPDATA%\Code\User\mcp.json`；macOS `~/Library/Application Support/Code/User/mcp.json`；Linux `~/.config/Code/User/mcp.json` | 無 |

注意：

- 註冊指令的 argv 由單元測試 `registration_commands_match_native_cli_argv` 檢查。本文實測中只有 `agy mcp add` 實際執行（測後移除）；`claude mcp add` 與 `codex mcp add` 未實際執行。
- 路徑為建議值，`setup` 不會自動寫入。若 `USERPROFILE`／`HOME` 不可用，路徑以字面 `~` 開頭。
- Claude Code 的使用者層級設定實際寫在 `~/.claude.json`，該檔內容由 Claude Code 持續改寫。`default_outputs` 保留此路徑只為說明位置；建議以 `registration_command` 註冊，而非手動合併。

## 3. 實測狀態

環境：Windows 10 Pro 10.0.19045，2026-10-21。隔離設定，未永久修改使用者設定。真實應用測試的 server 啟動目錄為專案內子目錄（`<scratch>/ws/src`），驗證 `--project-from-cwd`。來源 `0cc7d50`，binary SHA-256 `e1820f38…`（詳細紀錄：`verification/client-apps-local.json`）。

**本表紀錄綁定 `0cc7d50` 的 binary。** 之後的變更（entry 與註冊指令移除 `\\?\` 前綴、`default_outputs` 移除專案層級建議、`project add` 輸出欄位與 side-file 保護、`state_dir` 檢查、serve cwd 錯誤訊息）尚未以真實 client 重新執行。實測中 Claude Code、OpenCode、AGY 使用帶 `\\?\` 前綴的路徑；Codex CLI 以 `-c` 覆寫，使用去除前綴的路徑。

| 對象 | 測試方式 | 結果 |
|---|---|---|
| 七種 entry（自動化 smoke） | `scripts/client_setup_smoke.py`：生成 → 套用到含無關 server 的既有檔 → 啟動寫入的 entry → MCP initialize、session default 行、tools/list、status 呼叫 | 127/127 PASS（同一 binary）。不啟動任何 client 應用程式 |
| Claude Code 2.1.306 | `claude -p --mcp-config <檔> --strict-mcp-config` | PASS：載入 Odin，依 instructions 得知專案 `demo`，呼叫 `status` 成功 |
| Codex CLI 0.163.0 | `codex exec -c mcp_servers.odin.command=... -c mcp_servers.odin.args=[...]`（以 `-c` 覆寫，未讀取寫入的 config.toml） | PASS：結果同上 |
| OpenCode 1.18.35 | 專案 `opencode.json`；`opencode mcp list` | 連線 PASS（`odin connected`）。工具呼叫 NOT_RUN：`opencode run` 即使在無關目錄、不用工具時，也因模型供應商 UnknownError 失敗 |
| AGY 1.4.0 | `agy mcp add` 註冊後測試；測後移除，設定檔位元組還原相同 | 伺服器載入，模型要求呼叫 Odin 工具；headless print 模式自動拒絕 `mcp` 權限，工具執行 NOT_RUN。需 AGY permissions.allow 規則或互動核准 |
| Gemini CLI | 本機未安裝，未執行 | NOT_RUN。格式依其文件化 MCP 設定 |
| Cursor | 本機未安裝，未執行 | NOT_RUN。格式依其文件化 MCP 設定 |
| VS Code | 本機已安裝，但未測試其 agent 的 MCP 載入 | NOT_RUN。格式依其文件化 MCP 設定 |

以上「PASS」只代表該次測試所列的動作成功，不代表其他版本、其他 OS 或其他 client 設定路徑已驗收。

## 4. 自動選擇專案（`--project-from-cwd`）

server 啟動時：

1. 取得 server 的目前工作目錄，並 canonicalize。若無法取得目前目錄或 canonicalize 失敗，stderr 寫 `cannot determine current directory: <err>; no session default project`，並不設預設專案。
2. 在 host config 中選出 root 等於或包含此目錄的專案；多個符合時取最長 root。
3. 若有 `--default-project <id>`，則優先使用該專案；該 ID 必須已存在於 host config，否則 `setup` 直接失敗。`--default-project` 不依賴工作目錄。
4. 無符合時，server 照常啟動，stderr 寫一行 warning，不設預設專案。instructions 只列出 `Configured project IDs`，agent 須自行選擇 `project`。

選定結果會以下列文字加入 MCP instructions：

```text
Session default project: <id> (selected by cwd). Pass "project": "<id>" in every tool call unless the user names another configured project.
```

`status` 與 `get_current_config` 的輸出不因此改變。

工作目錄由 client 啟動 stdio server 時決定，不一定等於你開啟 session 的目錄。驗證範圍：

- 已實測 `--project-from-cwd` 的公告與選擇：Claude Code、Codex CLI（server 啟動於專案子目錄時選出正確專案）。
- 未驗證：Cursor、VS Code、AGY、Gemini CLI、OpenCode 的工具呼叫（AGY、OpenCode 工具呼叫 NOT_RUN；Cursor、VS Code、Gemini CLI 未實測）。在這些 client 上，請確認公告是否出現並指向正確專案。
- cwd 錯誤只改變公告的預設專案，不改變權限；每次工具呼叫仍由 agent 指定 `project`，`project` 不符時仍依 host config 拒絕或接受。

不確定 client 的工作目錄時，請改用 `--default-project <id>`。

## 5. 疑難排解

- `Unknown configured project: <id>`：ID 不存在於 host config。先執行 `odin --config ... validate` 檢查。
- `--output must be outside configured project roots`：目標位於已配置 repo 內。請改用使用者層級位置；不要把 Odin entry 放入 repo。
- `CONFLICT: --expected-sha256 must match the existing file`：檔案已變更，重新乾跑取得目前 hash。
- `Another setup operation is in progress...`：確認沒有其他 setup，再刪除殘留的 `<file>.odin-setup.lock`。
- `Project root cannot be inside the state directory`：root 與 state 目錄重疊。改用不同的 root 或 state 目錄。
- `project add` 回錯誤要求 `--name`：目錄名稱不含 ASCII 英數字，請以 `--name <id>` 指定。
- `cannot determine current directory: ...`：server 無法取得工作目錄，沒有 session 預設專案。改用 `--default-project`。
- agent 讀寫 `<config>.odin-*` 被拒：host config 位於 project root 內時的預期保護，不是錯誤。
- 更換 Odin 執行檔或 host config 路徑後，舊 entry 仍指向舊路徑，需重新 `setup`。
- `project add` 只能新增。修改或移除既有專案，需由操作者以文字編輯 host config，並以 `validate` 驗證；Odin 不提供此命令。

## 相關文件

- [使用流程與參數](USAGE.md)
- [已知限制](KNOWN_LIMITATIONS.md)
- [架構](ARCHITECTURE.md)
