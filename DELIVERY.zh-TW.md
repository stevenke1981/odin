# Odin 0.2.0 Standalone — 交付說明

## 本次完成範圍

這是由先前 0.1.0 原始碼包修改而來的獨立版本。Serena Bridge、BridgeConfig、上游工具分派／程序管理、兩個 serena_* 工具、橋接範例與上游 mock 已移除。四個 Rust crates、58 個原生／直接協議工具保留。完整變更可查 `verification/source-diff.patch`。

`status` 新增 standalone=true / external_tool_bridge=false。舊 bridge 設定會被拒絕，舊工具視為未知；沒有缺少原生能力時自動轉發的路徑。原始碼中的相關回歸測試已更新，Rust 測試尚未執行。

## 不是本次完成項目

沒有補齊原先依靠上游提供的語意 Move／Inline、JetBrains inspections、Python REPL 或所有語言適配。沒有新增獨立 JetBrains 外掛。**移除相依不是功能等價；本版仍是 SOURCE_PREVIEW。**

Odin 核心不需要 Python 或 Serena；語意功能仍需另外配置語言伺服器，除錯仍需另外配置 DAP adapter。外部後端可能有 Java、Node.js、Python 等自己的環境需求。Python 是交付／開發驗證工具之一。

## 實際驗證

本次執行了原始碼／工具契約檢查、Python LSP／DAP fixture 自測、Dashboard JavaScript 語法檢查、Bash 語法檢查、Rust 詞法括號檢查，以及封裝校驗。具體數量與結果以本版 `verification/*.json` 為準，不沿用 0.1.0 數字。

沒有 Rust／Cargo，故依賴解析、Cargo.lock、Rust 編譯、Rust tests、fmt、Clippy、真實 Odin MCP 啟動、真實 LSP／DAP 及平台測試均為 **NOT_RUN**。Rust parser 亦未能安裝；詞法檢查不能替代 Rust 語法／型別／連結驗證。沒有提供執行檔。

未連接或修改 5900X；未建立或推送遠端倉庫；未啟動雲端 CI；未建立背景任務。舊 ZIP 未被覆寫。

## 使用入口

先讀 `README.md`、`docs/STANDALONE.md` 與 `docs/KNOWN_LIMITATIONS.md`。在具備指定 Rust／MSVC 等建置環境的主機執行 `scripts/bootstrap.ps1` 與 `scripts/verify.ps1`，以實際退出碼及 log 判斷結果。由 0.1.0 升級先備份，再移除舊 bridge 設定；原生 state 格式未刻意更動，但跨版本遷移尚未驗證。
