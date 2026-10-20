# 已知限制 — 不是隱藏的完成項目

1. **整個 Rust 程式尚未在交付環境編譯。** 型別、相依解析、SDK 對接及平台差異可能仍需修正。Rust tests / clippy / fmt / binary smoke 均不可標 PASS。
2. 本版不依賴 Serena，但不是它的逐參數替代品。移除橋接並沒有新增 JetBrains 專用能力，也沒有把未實作的進階功能變成原生實作。
3. native `list_dir` 回檔案清單，不包含獨立空目錄項；`find_symbol` 為 name_path substring，沒有完全重現上游所有 kind/depth/grouping/filter options。
4. native diagnostics 不等同 JetBrains inspections。symbols/reference 結果由語言伺服器決定，動態載入、反射、宏與未納入 workspace 的檔案可能不完整。
5. `safe_delete_symbol` 只檢查 LSP 靜態 references，不保證動態 consumers 不受影響；原生 Move / Inline / cascading delete 未實作。
6. WorkspaceEdit 的檔案建立／移動／刪除 resource operations 未實作；只接受受快照保護的文字 edits。
7. memory 預設 central host storage；沒有 repo-local layout 自動遷移、跨 scope move/import/export、ignored patterns 與完整 YAML/Jinja context/mode 層。
8. FTS5 是全量 snapshot lexical index；沒有 file watcher、增量索引、向量 embeddings。查詢移除過期命中，但新內容須 reindex。
9. flow 是 read-only JSON dataflow，不是 Python REPL；同時流程會被共用 namespace mutex 序列化，沒有最佳化併行流引擎。
10. 不再提供外部 MCP tools/list/call 代理；舊 serena_tools／serena_call 與橋接設定會失效。需要進階 Move／Inline／Inspections 的工作不能把這版當作功能等價替代品。
11. journal 是逐檔原子替換＋恢復，非多檔原子可見交易。外部寫入競態仍需 OS/工作流控制。
12. Windows 原生路徑、installer lifecycle、Unix process group、macOS 沙箱、後代程序清理均未驗證或未實作。此包沒有 Windows 安裝程式。
13. HTTP 是本機單操作者服務，非公網或多租戶產品。Dashboard 只顯示專案和稽核，不是完整 Serena UI / logs / analytics / model token 統計。
14. 不自動安裝 LSP / debug adapter，不把 CLI presence check 當成功整合。真實 LSP／DAP 測試皆 NOT_RUN；Serena／其 JetBrains 外掛已不屬於本版後端。
15. 稽核、index、journal、spool 未提供 retention / encryption / backup automation。大型 repository 的 RSS / latency / disk growth 尚未量測。
16. Cargo.lock 尚未生成；直接依賴已精確指定，遞迴依賴、完整 SBOM、license audit、漏洞掃描仍待實際 Cargo resolve/build。

這些限制讓 0.2.0 適合作為接續實作與驗收的原始碼基礎，而不是無人值守的 production 替代品。
