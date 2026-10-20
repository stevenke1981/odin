# 獨立產品路線圖：不以橋接代替原生完成

## 0.2.0 — 已修改的來源範圍

移除 Serena Bridge、設定、程序管理與兩個工具；保留 58 個原生／標準協議工具。新增拒絕舊設定與舊工具的回歸測試原始碼。此項是源碼變更，不是編譯或執行驗收證據。

## 下一個門檻 — 真實建置與直接後端

在具備 Rust toolchain 的環境解析依賴，產生 Cargo.lock，修復編譯／Clippy／tests。於未安裝 Serena 的乾淨環境測試 MCP、rust-analyzer、DAP 與 Windows 原生執行。完成前維持 SOURCE_PREVIEW。

## 語意層與重構

補齊 WorkspaceEdit resource operations、能力與 readiness registry、file watcher / 增量失效、語言專用設定。Move／Inline 先定義各語言語意正確性與 fixture，再透過後端 Code Actions 或自建語言適配器實作；不可拿字串搬移充當語意重構。

## 批次、IDE、診斷

改良原生 JSON flow 的 per-session locks、有界併行與資料篩選；不重新依賴 Python／Serena REPL。區分 compiler diagnostics、linter 和 IDE inspection；有需要的 IDE integration 採獨立協議與自行實作的 adapter，不使用 Serena 外掛。

## 程序與部署

Windows Job Objects、Unix process groups、實際 OS sandbox runner、路徑 hardening、state retention / backup、檔案與結果的配額。全部列為待驗收，不因採 Rust 就視為安全。

## 1.0 條件

原生 Windows/Linux/macOS 驗收、支援語言矩陣、故障復原、安全審查、安裝／卸載、真實使用報告與對照量測。沒有預設完成日期、背景排程或未執行的 CI 成功宣告。

## 正式執行工作清單

以 [FEATURE_SPEC.md](FEATURE_SPEC.md) 的 ODIN-001..040 為唯一完整的驗收工作 ID。優先 P0 → P1 → P2，跨依賴順序依 [IMPLEMENTATION_BACKLOG.md](IMPLEMENTATION_BACKLOG.md)。未取得真實執行證據不得提高驗收狀態。
