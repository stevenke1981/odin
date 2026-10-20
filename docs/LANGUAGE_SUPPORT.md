# 語言與除錯後端支援矩陣

Odin 直接連接 host 設定的 LSP/DAP，不包含 Serena bridge。**configured、已握手與特定功能已驗證是不同狀態**；generic LSP transport 不等於 40+ 語言全部支援。以下限於 Windows x64 MSVC 與報告所記 binary digest，沒有跨平台或完整 IDE 等價主張。

| 後端 | 已記錄版本 | 已實測範圍 | 尚未驗收 |
|---|---|---|---|
| Rust / rust-analyzer | 1.90.0 (1159e78c) | 真 symbols、cross-file references、rename definition/consumer uses、rollback；另有直接協定 initialize/documentSymbol smoke | 20 查詢逐項對照、diagnostics 正反例、Move/Inline/QuickFix 真實重構、Linux/macOS |
| Python / basedpyright | 1.40.2 | 真 symbols、cross-file references、rename definition/consumer uses、rollback | diagnostics 正反例、語言專用 refactor、至少兩個實際專案與 Linux/macOS |
| TypeScript / typescript-language-server | 6.0.2 | 真 symbols、cross-file references、rename definition/consumer uses、rollback；測試明確指定相容 tsserver 路徑 | diagnostics 正反例、真實 refactor、多 workspace 及 Linux/macOS |
| Go、C#、Java、JavaScript 等其他語言 | NOT_VERIFIED | 可由 host 設定直接後端 | 六語言矩陣未完成；未執行不可宣稱支援 |
| Python / debugpy DAP | 1.8.22 | Windows 真 debuggee launch、breakpoint、threads、variables、evaluate、step、disconnect | Linux 真 adapter、跨 adapter/OS 子孫程序清理、完整 debug_batch 生命週期 |

實際觀察見 [real-backends.json](../verification/real-backends.json)，其 binary SHA-256 與 backend versions 必須保留。最終報告共有 **29 個 PASS 子案例**，三語各有獨立 consumer 檔案，實際測到跨檔 references、definition/consumer rename 與 rollback；另含 debugpy 真實除錯。`local-validation.json` 的 `/commands/9` 記錄該命令 **exit 0**。這不代表 diagnostics、真實 Move/Inline/QuickFix 或全部語言驗收。較早 indexing/content-modified/TS7 嘗試另存歷史報告。

[protocol-verification.json](../verification/protocol-verification.json) 另記 Rust 協定套件的真 rust-analyzer smoke，與 Python protocol fixture tests 分開。protocol peers 驗證 framing、capability、版本、交易與錯誤路徑，不是真實語言、JetBrains inspections 或 debugger 功能等價證明。

## 能力與限制

- `get_capabilities` / `doctor --probe-lsp` 執行 capability handshake；索引是否完整仍取決於 backend evidence。未知 readiness 保持 unknown。
- Move/Inline/QuickFix 要求後端提供相應 kind 的可用 edit；只有 command、disabled、unsupported 或模糊 action 會拒絕，沒有文字替換 fallback。
- WorkspaceEdit 只處理 regular UTF-8 files 與既有父目錄；directory、recursive、binary、overwrite 與需要確認的 annotations 明確拒絕。
- diagnostics 必須區分 pending、stale、unversioned、available；有 diagnostics 不證明 clean build。獨立 Clippy/Ruff/ESLint rule providers 尚未完成。
- host 明確決定 executable、argv、write/shell/debug 權限。外部後端可能需要 Node/Python/Java 等 runtime；Odin 核心檔案/記憶功能不需要 Python。

## 新增支援的驗收

新增語言需記錄 server/runtime/OS/version、workspace config、至少兩個實際專案及反例；逐項測 symbols、references、rename、diagnostics、refactor、清理與失敗處理。先完成 Rust/Python/TypeScript 全部原驗收，再擴 Go/C#/Java 等六語言。未實測功能一律保留 NOT_RUN/UNAVAILABLE，不用工具數量替代 support matrix。

本次驗證 revision：`82901e5cd45845c75c7475d2697db42d472b67ac`；release binary SHA-256：`5041951e452278e798568fa0c8c0f260ebe73f1a404659b3b3ac70c8141ce8be`。
