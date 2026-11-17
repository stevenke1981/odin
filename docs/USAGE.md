# 使用流程與參數

以下命令是在成功建置後的操作方式；不是本次已執行的紀錄。

## 原生查詢

```powershell
$odin = '.\target\release\odin.exe'
& $odin --config examples\odin.native.toml call list_dir --project demo
& $odin --config examples\odin.native.toml call read_file --project demo --args '{"relative_path":"src/lib.rs"}'
& $odin --config examples\odin.native.toml call get_symbols_overview --project demo --args '{"relative_path":"src/lib.rs"}'
& $odin --config examples\odin.native.toml call find_symbol --project demo --args '{"query":"greet","include_body":true}'
```

PowerShell 版本對 native argv 引號處理有差異。遇到 JSON 引號被剝除，使用 `--args-file`；所有重要操作都有 JSON 檔案方式，不必手工 escape 多層字串。

## 原生修改

```powershell
$odin = '.\target\release\odin.exe'
$preview = & $odin --config examples\odin.native.toml call transaction_prepare --args-file examples\prepare-create.json | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Prepare failed' }
$preview | ConvertTo-Json -Depth 20
# 審查 diff 後才執行以下 apply；不要無條件自動串接。
$apply = @{project='demo'; transaction_id=$preview.transaction_id} | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $PWD 'apply.local.json'), $apply, (New-Object Text.UTF8Encoding($false)))
& $odin --config examples\odin.native.toml call transaction_apply --args-file apply.local.json
```

同一 request_id 不可更改輸入；重複 apply 已 committed 的 transaction 不重寫檔案。`transaction_rollback` 會產生另一個反向 journal；`transaction_recover` 只處理 interrupted Prepared / RecoveryRequired。

注意 `examples/prepare-create.json` 是 create-only；若 notes.md 已存在則拒絕，不會覆蓋。新檔案的父目錄必須已存在。

## 專案 refactor 規則

host 可在專案下設定 `[projects.<id>.refactor_rules]`，未設定即無規則：

- `max_files`：單次 refactor 最多可修改的檔案數。
- `protected_globs`：refactor 不可修改的路徑 glob，與 `deny_globs` 分開；硬性路徑保護仍是 `deny_globs`。
  `max_files` 與 `protected_globs` 以正規化後的專案相對路徑計算，只計內容真的改變的檔案。
- `naming`：`[{kinds, pattern, description}]` 陣列，只由 `rename_symbol` 檢查。`kinds` 為 LSP SymbolKind 數值（1–26，空陣列表示全部），`pattern` 為整個名稱的 regex（等同 `^(?:pattern)$`），`description` 為違規時提供給 agent 的說明。多條規則是 AND：所有 `kinds` 含該 symbol kind（或 `kinds` 為空）的規則都必須符合，不能用多條規則表達「任一符合」。
- `guidance`：字串陣列，host 撰寫的重構指引。

Agent 不應自行推測規則，先呼叫唯讀的 `refactor_rules`（參數只需 `project`）取得有效規則。違反規則的 refactor 會回傳錯誤碼 `REFACTOR_RULE_VIOLATION`；此時應調整命名或範圍，不可改用 `replace_content`、`replace_in_files` 等通用編輯工具繞過。

建議的 agent 流程：

1. `refactor_rules`：讀取有效規則。
2. `find_symbol`：定位要重構的符號。
3. refactor 工具（`rename_symbol`、`move_symbol`、`inline_symbol`、`safe_delete_symbol`、`prepare_code_action`、`apply_quick_fix`）：產生交易預覽。
4. 審查預覽的 diff。
5. `transaction_apply`：以 expected hash 套用；衝突即拒絕。
6. `get_diagnostics_for_file`：檢查修改後的診斷。
7. 在 host 已授權 `allow_shell` 與測試 profile 的前提下執行測試（見下節「驗證建置」），以 `success`／`exit_code` 判定結果。

規則只約束上述六個 refactor 工具，其餘邊界見 [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md)。

## 驗證建置

host 中設定命令 profile 後，還必須明確打開 allow_shell。Agent 只能傳 profile 名稱：

```json
{"project":"demo","profile":"test"}
```

回傳 `success:false` 或非零 exit_code 就是測試失敗；即使外層 MCP call 沒有 protocol error，也不可當成驗收通過。

## 受限批次

```powershell
.\target\release\odin.exe --config examples\odin.native.toml call flow_run --args-file examples\flow-files.json
```

`flow-symbols.json` 需要真實 LSP。每步的 `as` 存變數，`{"$ref":"name#/pointer"}` 只讀資料，不能執行字串。namespace 以 project + session 分開。使用 flow_vars / flow_clear 查詢與清理。

## 記憶

`write_memory` 需內容與 expected_sha256，新增時為 null。`read_memory` 回 hash，後續修改必須使用新 hash。`global/` 為全域 scope，還需 allow_global_memory_write。同 scope rename 更新 `mem:` 引用；唯讀 referring notes 會使 rename 拒絕。

## LSP 設定

各個語言伺服器以 `[[projects.<id>.languages]]` 設定 id / extensions / program / args，可另填 initialization_options 與 settings tables。Odin 不把在 PATH 找到程式當作驗證成功。

Rust 範例需要安裝 rust-analyzer 與 rust-src/sysroot。其他語言需要自行使用其官方啟動參數；有的需要 build metadata / compile_commands / workspace setup。沒有泛稱「所有 LSP 都已驗證」。

## DAP

在 host 中設定 `[projects.demo.debuggers.<name>]` 的固定 program / args，明確 allow_debug，再依序：debug_start → debug_request(initialize 已由 Odin 執行，通常接 launch/attach) → debug_events → setBreakpoints → configurationDone → threads / stackTrace / scopes / variables / evaluate → continue / step → debug_stop。

launch / attach 回 ticket；等待 initialize event / adapter-specific 規範，不要把 pending 當完成。Odin 不代填未知 adapter 的 launch arguments。真實 adapter 測試未執行。

## 無上游依賴

本版不安裝或啟動 Serena。由 0.1.0 升級時移除 `[projects.<id>.serena]` 設定與兩個 `serena_*` 工具呼叫。舊設定會被拒絕，舊工具不會轉發。

無 language server 時可用檔案／記憶／FTS5；有 language server 才能執行相應語意能力；有 DAP adapter 且明確授權才提供除錯。無後端能力時會回錯誤，不做猜測式替代。詳見 `STANDALONE.md`。

## MCP 工具公開範圍

`serve` 預設 `--tool-profile core`：只公開日常的讀取、搜尋、符號、引用與交易編輯工具，且隱藏 host 政策對所有專案都停用的工具（例如唯讀專案的編輯工具）；單一專案時 `project` 參數可省略。`ide` 加上導航、IDE 查詢與其餘編輯工具，`full` 公開全部 87 個、契約與 catalog 完全相同。`--include-tools a,b` 個別加入。CLI `call` 不受影響。公開範圍不是權限邊界：每次呼叫仍由 engine 檢查 host policy。

## HTTP

只支援 loopback listener；MCP endpoint `/mcp` 及 `/api/*` 都需要 Bearer。Dashboard 是 `/`，輸入 token 後讀取資料；不在網頁自動載入外部 scripts 或字型。stdio MCP 不需要開 HTTP port。

## 自動載入 Coding Agent

`setup --client <c> --project-from-cwd` 產生可讓 client 啟動 `odin serve` 的 entry，預設乾跑、不寫入；加 `--output <檔> --apply --expected-sha256 <hash>` 才會寫入。`--default-project <id>` 可固定預設專案。這兩個選項只影響 MCP instructions 的建議文字，工具呼叫仍需明確 `project`。`project add --root <dir>` 可在既有 host config 追加專案，同樣預設乾跑；新增 root 會擴大權限（輸出 `host_permissions_changed: true`），root 不得位於 state 目錄內，非 ASCII 目錄名需 `--name`。只建議使用者層級的 client 設定位置，不把 Odin entry 放進 repo 內的專案層級檔。各 client 的格式、路徑與實測狀態見 [CLIENT_SETUP.md](CLIENT_SETUP.md)。
