# 使用當前專案與即時分析

Odin 的核心支援多專案；固定 sigma 來自舊 host/client entry。新模式在每次
server 啟動時，依 `--workspace PATH` 選定唯一專案；`--workspace .` 的 `.`
相對於 **啟動程序的 cwd**，與 host.toml 所在目錄無關。MCP 沒有任意 root
參數，也不從 caller 的 repository files 建立 executable host 配置。

## Host 設定

將 [設定範例](../examples/odin.workspace.toml) 放在 workspace 外的可信 host
目錄，指定 `workspace.allowed_roots` 與實際安裝的 language program。相對的
allowed_roots/state_dir 仍以 host 設定檔目錄為基準。新 workspace 一律唯讀；
沒有 write、shell、debug、inspections、formatters grants，不自動安裝 backend。
要寫入或執行，必須另由操作者登記明確 root 與 host permissions。

```powershell
cd C:\Projects\alpha
odin --config C:\Odin\host.toml --workspace . analyze
odin --config C:\Odin\host.toml --workspace . serve --transport stdio
# 輸出含 --workspace . 的 client entry，套用需既有 setup 的 CAS/backup 流程。
odin --config C:\Odin\host.toml --workspace . setup --client codex
```

workspace 模式的 `--config` 必須是絕對路徑；省略時使用固定
`home/.config/odin/host.toml`，不讀取 cwd 的 odin.toml 作為 host policy。
一般 host 仍須位於選定專案外；固定 home host 透過強制私有樹保護提供窄例外。
`projects` 可省略，這時需明確 workspace 或 host 宣告的 `workspace.default_root`。
新安裝的 default_root 是 home，但不登記成共同 ancestor project。沒有匹配／允許的
root 時明確拒絕；未宣告 default_root 的舊設定保留原有多專案行為。詳見
[Home 預設與私有路徑](HOME_AND_INSTALLER_PROGRESS.md)。

啟動目錄位於已登記 root 裡時，選該登記專案，保留其明確 host policy。
其餘目錄精確綁定指定 PATH，不向上搜尋 .git 或 manifest；開在 src 子目錄
時請傳完整專案根目錄。canonical root 的 hash 形成穩定 ID，使不同專案的
state、memories、索引、journals、LSP keys 分開。Windows 同根大小寫別名相同。
原有 host/config/state 保護與重疊 root 拒絕仍生效。

## 使用時分析

MCP 初始 instructions 會列這次 server 的 project ID，亦可呼叫 list_projects。
每次開始工作先呼叫 `project_analysis`，再用 `get_capabilities` 與真正 LSP
工具取得語意資訊。`onboarding` 也包含同一份結構分析。workspace hook 不寫死
本次 hash；setup 保留原始 `.`，避免再次固化產生設定時的專案。

`project_analysis` 是 read policy，參數為 project、max_evidence (1–64，預設20)、
sample_files (0–100，預設20)。每次重掃描允許的 live files，列檔案數、目錄、
副檔名、語言/marker hints、host backend 缺口、實讀 manifest/instruction 的
SHA-256 與讀取錯誤。只回 hash/大小，不回執行指令；不啟動/probe backend。
tests/inspections 維持 NOT_RUN，不能把結構 hints 當成語意分析或測試通過。

`scan_truncated`、`evidence_truncated`、`directories_truncated` 會揭露配額限制。
structure_sha256 只涵蓋列出的 paths 與本次有界 evidence，沒有涵蓋全部來源
內容；LSP 查詢會另同步文件 hash。忽略規則與 path policy 會限制可見資料，
缺少 evidence 不能證明檔案不存在。讀取不是跨檔案原子快照，沒有背景 watcher。

## 使用範圍

同一 entry 可服務每次 launch 的不同專案。已開啟的 MCP process 不會隨 client
UI 切換 cwd；切專案需要 client 以新 cwd 重新啟動 server。若 client 固定其
MCP cwd，請在該 client 指定正確 cwd 或傳絕對 --workspace root。
本次 Windows CLI/MCP 驗收不代表所有真實 client 的生命週期已驗收。

私人 state 及其任何子目錄不能成為 project root。setup/hook 的 host-output
防護保留全部已登記 roots 與 workspace.allowed_roots；即使 runtime 只選 A，
也不能用 host 輸出功能寫入 B。Home policy 僅對固定標準 client 設定檔的
明確 setup/CAS 提供窄 host-output 例外；Hook 與任意 home 檔案不適用。
backend gaps 按實際觀察到的副檔名檢查；例如只配置 ts 不能覆蓋 tsx。
