# Odin 0.2.0 — Windows 原生預覽交付說明

本次已由未編譯的原始碼預覽，推進到 **Windows x64 MSVC 已實際建置與測試的預覽**。四個 Rust crates，原有 58 個工具保留並新增 14 個，共 72 個原生工具契約。Cargo.lock 已由真實 Cargo 產生並納入版本控制；產品仍沒有 Serena bridge、自動 fallback 或任意 eval。

Serena 僅用於開發代理導航；Odin 直接操作自己的 Rust core、LSP/DAP client 與 engine。預設 write/shell/debug 仍停用，只有可信任 host 設定可以開啟。所有原生專案修改經 SHA-256/CAS Transactions，不覆寫衝突。

## 已執行的證據範圍

- Windows engine/protocol/platform 的 package tests、嚴格 package Clippy 與 formatting 有命令及 exit code；見 `verification/*-verification.json`。
- 真 binary CLI、stdio、loopback HTTP、授權負例、generated client config handshake、取消與結果/交易流程已有 smoke。
- Windows 真 rust-analyzer、basedpyright、TypeScript language-server 完成定向 symbols/references/rename/rollback；debugpy 完成真 breakpoint、variables、evaluate、step。
- core 日誌記錄 NTFS aliases/reparse/hardlink、交易 crash checkpoint 與雙 process contention。供應鏈另有 SBOM、license inventory、advisory/pattern scan 報告。
- portable ZIP 與 Inno recipe 已編譯；recipe 報告使用較早 binary，不是最終發佈工件或完整 installer lifecycle 證明。

以上只適用各自 source/binary snapshot。`verification/feature-evidence.json` 逐項引用真實零退出 command，並保留原驗收條件的缺口；本次文件工作沒有執行產品測試。最終整合的 workspace/release/binary/supply-chain runner 在實際執行後由 root 更新 `verification/local-validation.json`、交付狀態與最終數量；準備中的命令不可填 PASS。

## 尚未完成

40 項需求目前為 38 PARTIAL、2 NOT_RUN，沒有宣稱全部完成。真正 OS sandbox、Linux/macOS native matrix、乾淨 Windows/跨版 installer lifecycle、Defender/CFA、六語言與 40+ 語言、完整 IDE inspections/refactor parity、性能/品質 benchmark 均未完成。

Move/Inline/QuickFix 是後端 edit 的安全入口；成功 fixture 不代表真實語言重構編譯成功。Safe Delete 仍限靜態 references。記憶仍是 host central storage，沒有自動 repo-local migration；索引是查詢前增量 FTS5，沒有背景 watcher；flow 不是完整 DAG scheduler/Python REPL。Job Objects/process groups 不是 OS 沙箱。

詳細限制見 [KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md)，後端分級見 [LANGUAGE_SUPPORT.md](docs/LANGUAGE_SUPPORT.md)，40 條原規格見 [FEATURE_SPEC.md](docs/FEATURE_SPEC.md)。

## 本機使用與發布

建置／執行入口見 [README.md](README.md)，平台與設定見 [PLATFORM_IMPLEMENTATION.md](docs/PLATFORM_IMPLEMENTATION.md)。一般 Cargo 命令使用 `--locked`；不要自行放寬 host 權限或把 repository 檔案當可執行 host 設定。

本說明的 GitHub 發布狀態仍是 **待 root 實際操作與驗證**，不沿用先前 connector 不足/404 的結論，也不預寫已發布。目標為公開 `stevenke1981/odin`；只有實際 push、public visibility 與 remote HEAD 核對才算完成。沒有啟動雲端 CI。發布流程見 [PUBLISH_GITHUB.md](docs/PUBLISH_GITHUB.md)。
