# Odin 0.2.0 — Windows 原生預覽交付說明

本次已由未編譯的原始碼預覽，推進到 **Windows x64 MSVC 已實際建置與測試的預覽**。四個 Rust crates，原有 58 個工具保留並新增 14 個，共 72 個原生工具契約。Cargo.lock 已由真實 Cargo 產生並納入版本控制；產品仍沒有 Serena bridge、自動 fallback 或任意 eval。

Serena 僅用於開發代理導航；Odin 直接操作自己的 Rust core、LSP/DAP client 與 engine。預設 write/shell/debug 仍停用，只有可信任 host 設定可以開啟。所有原生專案修改經 SHA-256/CAS Transactions，不覆寫衝突。

## 已執行的證據範圍

- Windows engine/protocol/platform 的 package tests、嚴格 package Clippy 與 formatting 有命令及 exit code；見 `verification/*-verification.json`。
- 真 binary CLI、stdio、loopback HTTP、授權負例、generated client config handshake、取消與結果/交易流程已有 smoke。
- Windows 真 rust-analyzer、basedpyright、TypeScript language-server 完成真 symbols、cross-file references、definition/consumer rename 與 rollback（真後端報告共 29 個 PASS 子案例）；debugpy 完成真 breakpoint、variables、evaluate、step。
- core 日誌記錄 NTFS aliases/reparse/hardlink、交易 crash checkpoint 與雙 process contention。供應鏈另有 SBOM、license inventory、advisory/pattern scan 報告。
- 最終 release binary 已產生 portable ZIP 與 Inno installer；39 檔／87 項 ZIP 完整性檢查及安裝、重裝、卸載 8 項檢查全部通過。見 `verification/windows-final-packaging.json`；乾淨機、跨版及中斷情境未納入。

以上只適用各自 source/binary snapshot。`verification/feature-evidence.json` 逐項引用真實零退出 command，並保留原驗收條件的缺口；本次文件工作沒有執行產品測試。最終整合 runner 已完成：`verification/local-validation.json` 記錄 **16 個命令全部 exit 0、144 Rust tests 通過、1 ignored、source_changed=[]**，包括 workspace fmt/test/clippy、release build、CLI/stdio/HTTP/cancellation、真後端、advisory/licenses/supply-chain、source/requirements/publish-policy。

被測 clean revision 為 `82901e5cd45845c75c7475d2697db42d472b67ac`；release binary SHA-256 為 `5041951e452278e798568fa0c8c0f260ebe73f1a404659b3b3ac70c8141ce8be`。此後文件/evidence commit 可以不同；Rust/test source digest 及最終封裝 manifest 須據實對照。

## 尚未完成

40 項需求目前為 38 PARTIAL、2 NOT_RUN，沒有宣稱全部完成。真正 OS sandbox、Linux/macOS native matrix、乾淨 Windows/跨版 installer lifecycle、Defender/CFA、六語言與 40+ 語言、完整 IDE inspections/refactor parity、性能/品質 benchmark 均未完成。

Move/Inline/QuickFix 是後端 edit 的安全入口；成功 fixture 不代表真實語言重構編譯成功。Safe Delete 仍限靜態 references。記憶仍是 host central storage，沒有自動 repo-local migration；索引是查詢前增量 FTS5，沒有背景 watcher；flow 不是完整 DAG scheduler/Python REPL。Job Objects/process groups 不是 OS 沙箱。

詳細限制見 [KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md)，後端分級見 [LANGUAGE_SUPPORT.md](docs/LANGUAGE_SUPPORT.md)，40 條原規格見 [FEATURE_SPEC.md](docs/FEATURE_SPEC.md)。

## 本機使用與發布

建置／執行入口見 [README.md](README.md)，平台與設定見 [PLATFORM_IMPLEMENTATION.md](docs/PLATFORM_IMPLEMENTATION.md)。一般 Cargo 命令使用 `--locked`；不要自行放寬 host 權限或把 repository 檔案當可執行 host 設定。

原始碼已實際推送公開 [stevenke1981/odin](https://github.com/stevenke1981/odin)，已核對 remote HEAD、public visibility 與匿名 HTTP 200；實際證據見 [publication.json](verification/publication.json)。沒有啟動雲端 CI，也沒有建立 GitHub binary release；Windows 產物保留在本機 dist。發布流程見 [PUBLISH_GITHUB.md](docs/PUBLISH_GITHUB.md)。
