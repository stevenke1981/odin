# Odin 原生產品路線圖

目前是 **0.2.0 Windows x64 MSVC 原生預覽**：四個 Rust crates、72 個工具契約，原有 58 個工具保留並新增 14 個。Cargo.lock 已實際產生並納入 Git；已執行 Windows package tests、CLI/MCP/HTTP smoke，另有三個真實 LSP 與 debugpy 的定向報告。工具存在與局部測試通過，不能代表全部 40 項需求完成。

[FEATURE_SPEC.md](FEATURE_SPEC.md)、[IMPLEMENTATION_BACKLOG.md](IMPLEMENTATION_BACKLOG.md) 與 [feature-evidence.json](../verification/feature-evidence.json) 定義需求、相依、實際證據及未驗收條件。Windows 最終整合 runner 已完成 16 個零退出命令及 144 Rust tests（1 ignored）；最終 Windows portable／installer 已打包並通過隔離安裝驗收；公開 GitHub 發布結果見 delivery-status.json 的實際紀錄。

## 當前可用範圍

- SHA-256/CAS 預覽、交易提交、journal 恢復與 rollback；no-follow directory capabilities、Windows aliases/reparse/hardlink 負例。
- 直接 LSP 的同步、查詢、diagnostics、symbols filters/paging；有序 WorkspaceEdit regular UTF-8 file operations。
- Move/Inline/QuickFix 僅接受後端 edit，經同一交易流程；不支援便明確拒絕。protocol fixtures 不等於真實語言重構驗收。
- central Markdown 記憶及版本化 bundle、引用與 scope ownership；查詢前增量 FTS5；per-session bounded JSON flow。
- DAP session/state/tickets、環境 allowlist、程序樹生命週期；收緊 host policy 的 contexts/modes。
- client setup/hook 的 dry-run、備份與 CAS；唯讀 Dashboard、trace、result expiry/quota；portable/Inno recipe。

## 下一個整合門檻

同一候選來源的 workspace fmt/test/clippy、release build、真 binary CLI/stdio/HTTP/cancellation、真實後端與供應鏈已執行，16 個 command 皆 exit 0，結果與 source/binary digest 記在 `verification/local-validation.json`。套件切片和舊 binary 的 PASS 不自動適用後續變更；下一步是包含最新文件的最終打包與 artifact manifest 核對。

公開前保留獨立 review、機密掃描、license/SBOM/advisory、manifest 和 manual self-hosted workflow policy。推送後才核對 public visibility 與 remote HEAD；不因 push 啟動雲端 CI。

## 完整功能與平台門檻

真正 OS sandbox、Linux/macOS 原生執行、乾淨 Windows installer/升級/rollback、Defender/CFA、真 client 應用整合、六語言及 40+ 語言適配仍待完成。Job Objects/process groups 是生命週期管理，沒有 workspace 外存取限制。

語意 Move/Inline、native inspection/linter providers、Safe Delete reachability/cascade 必須逐語言驗證編譯與行為；不以 regex 搬移、空 diagnostics 或 command 已送出冒充成功。repo-local memory migration、背景 watcher、完整 DAG、state retention/backup 與全部敵意 I/O 矩陣仍有缺口。

## 1.0 條件

40 項原驗收條件全部有適用的可重現證據，Windows/Linux/macOS 原生矩陣完整，語言與 debugger support matrix 據實分級，資料遷移/故障恢復/安裝卸載可驗收。三規模、20 個日常 coding 任務的性能與品質實驗完成前，不宣稱更快、RSS 更低或 token 節省。沒有預設完成日期或背景排程。
