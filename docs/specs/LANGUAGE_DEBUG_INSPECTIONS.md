# 語言、IDE 級能力與除錯規格 (RFC-INTEL-001)

**地位：設計目標，非已驗收功能。** 參閱 ODIN-007/008/010/013/017/018/019/034/035/036/039。

## 直接標準協定，不使用 Serena

Odin 核心使用 Rust 自己的 LSP/DAP client。LSP 後端可以是 `rust-analyzer`、`gopls`、`basedpyright`、`typescript-language-server`、`clangd` 等，**後端執行檔仍必須由操作者安裝/允許 Odin 安裝**。不同 server 的 capabilities 不得混為一談。

## Backend adapter trait（目標）

- `detect_language(paths, manifests) -> LanguageCandidates`
- `resolve_executable(profile) -> ExecutableInfo{version, sha256, provenance}`
- `initialize(workspace_folders, capabilities) -> BackendCapabilities`
- `index_state() -> Pending | Ready | Error{reason}`
- `query(SymbolQuery) -> SymbolResponse`；必須有 symbol kind/range/source。
- `apply_rename_symbol(..) -> WorkspaceEdit`；並由 transaction engine 檢查；不是直接寫入。
- `healthcheck()`、`stop()`、`restart()`、`invalidate(paths)`。

## 診斷來源與語言能力

| 來源 | 提供哪些資訊 | 不等於 |
|---|---|---|
| LSP diagnostics | syntax/type 或 server 提供的 rules | 編譯器完整測試 |
| compiler/build | exit code 與 build log | IDE 即時 annotation |
| linter/inspections | 規則 ID、嚴重度、建議修復 | Debugger 的執行時行為 |
| DAP debugger | stack、frame、variable、evaluate | JetBrains IDE 專屬 persistent debug REPL |

每個 finding 必須標記 `provider`、`source`、`rule_id`、`relative_path`、`range`、`severity`、`document_version`、`status`，否則不得把空清單解讀成「已驗收通過」。可執行 quick fix 先變成 WorkspaceEdit 並進入 journal；未經預覽不得直接寫入。

## DAP 狀態機

`CREATED → INITIALIZED → LAUNCHING/ATTACHING → CONFIGURED → RUNNING ↔ STOPPED → TERMINATED`。只有 STOPPED 才能執行 frame/scope/variable/evaluate；需要 target PID、session id、可取消/超時。關閉時先 disconnect，再按 host policy 清理 debugger/debuggee process tree。

## 語言支援矩陣

公開的狀態必須分三層：`INSTALLED`（後端在 PATH）、`CONFIGURED`（完成 initialize）、`VERIFIED`（跑通真人/fixture 測試），每項能力另記 `PASS/FAIL/NOT_RUN/UNAVAILABLE`。先驗收 Rust、Python、TypeScript，再逐步擴充 Go、JavaScript、Go/C#、Java 等；**不因列出 40 個名字就宣稱支援 40 種語言**。

## JetBrains 特定能力替代

- `jet_brains_move` → 原生 semantic Move API，語言 adapter/CodeAction。
- `jet_brains_inline_symbol` → 原生 semantic Inline API。
- `jet_brains_run_inspections` → 自有 linter/inspection plugin providers。
- `jet_brains_debug` → 通用 DAP + bounded debugger session（不是 Python REPL）。

Odin 不依賴 Serena JetBrains Plugin；即便使用者有 JetBrains IDE，也不是與其原生 internal protocol 1:1 等價。
