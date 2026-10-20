# 本版驗證證據

此目錄僅放 0.2.0 本次實際檢查結果，不沿用 0.1.0 的通過數。

- source-check.json：UTF-8、TOML、JSON、Python AST、契約對應、無 bridge 生產程式碼／依賴及 Dashboard JavaScript；不是 Rust 編譯。
- fixture-selftest.json：6 項 Python LSP／DAP 模擬服務自測；沒有執行 Odin。
- rust-syntax-check.json：Rust parser 不可用，記錄 NOT_RUN；lexical-check.json 只有 Pygments 詞法與括號檢查，不是語法／型別／link 驗證。
- standalone-audit.json：生產原始碼、Cargo manifests、工具契約與移除檔案的相依掃描。
- generator-regression.json：重新執行生成器不恢復移除的工具／設定。
- delivery-status.json：整體交付狀態；未實際執行的項目保持 NOT_RUN。
- source-diff.patch：相對於先前 0.1.0 ZIP 的程式碼／設定／文件變更，不含驗證報告與 manifest。

Rust 測試宣告不是執行結果。PowerShell、CI、真實語言伺服器、debug adapter、Windows/Linux/macOS 原生執行皆未驗收。local-* 報告僅在日後執行腳本時產生，不能用模擬服務替代真實產品證據。

## 2026-10-21 規格追蹤補充

`spec-traceability-20261021.json` 只記錄 40 項需求／53 項參考功能對照與規格完整性檢查；沒有實際 Rust 編譯、LSP/DAP/MCP 端到端結果，也沒有 GitHub 發布成功的證據。
