# Odin 0.2.0 — Windows 原生預覽交付說明

本次為 **Windows x64 MSVC OS 沙箱與語言／IDE 擴充預覽**。四個 Rust crates，共 85 個原生工具契約。新增 completion／resolve、hover、signature、呼叫階層、inlay hints、semantic tokens 與格式化交易，以及 Clippy／Ruff／ESLint findings 和安全修正預覽。Cargo.lock 由真實 Cargo 產生並保留；產品沒有 Serena bridge、自動 fallback 或任意 eval。

Serena 僅用於開發代理導航；Odin 直接操作自己的 Rust core、LSP/DAP client 與 engine。預設 write/shell/debug 仍停用，只有可信任 host 設定可以開啟。所有原生專案修改經 SHA-256/CAS Transactions，不覆寫衝突。

## 已執行的證據範圍

- Windows engine/protocol/platform 的 package tests、嚴格 package Clippy 與 formatting 有命令及 exit code；見 `verification/*-verification.json`。
- 真 binary CLI、stdio、loopback HTTP、授權負例、generated client config handshake、取消與結果/交易流程已有 smoke。
- Windows 真 rust-analyzer、basedpyright、TypeScript language-server 完成真 symbols、cross-file references、definition/consumer rename 與 rollback（真後端報告共 29 個 PASS 子案例）；debugpy 完成真 breakpoint、variables、evaluate、step。
- 六語各兩個受控專案另完成 completion、hover、signature、階層、語意 tokens、適用格式化交易與診斷流程。`verification/language-ide-final-baseline-03.json` 及實際 command receipt 記錄 260 PASS、14 UNAVAILABLE、12 NOT_VERIFIED、0 FAIL，整體 PARTIAL／exit 2；TS／Java 無版本診斷不算 current clean。
- core 日誌記錄 NTFS aliases/reparse/hardlink、交易 crash checkpoint 與雙 process contention。供應鏈另有 SBOM、license inventory、advisory/pattern scan 報告。
- 真正 Windows MXC ProcessContainer 沙箱以 executor SHA-256 綁定，明確設定檔案權限、封鎖網路、使用私有暫存，失敗不回退。三項原生測試執行了檔案／網路拒絕、後代程序、取消／逾時及清理。
- 真 TypeScript LSP 在明確固定的 host bootstrap profile 下，兩專案完成沙箱內的語意查詢、跨檔引用、重新命名／格式化及 CAS apply／rollback；profile04 實跑 42 PASS／2 UNAVAILABLE／6 NOT_VERIFIED／0 FAIL，整體 PARTIAL。其他五語的實際 sandbox LSP 仍 NOT_RUN。
- `verification/native-inspections-release-final.json` 與其 command receipt 保存同一最終 binary 的三種分析器共 30 項 PASS：唯讀分析、finding／來源 hash、安全 CAS 修正／回復及私有暫存清理。
- 先前 72 工具版本的 portable ZIP／Inno installer 證據保留於 `verification/windows-final-packaging.json`；這次 85 工具擴充沒有重新封裝。

以上只適用各自 source/binary snapshot。`verification/feature-evidence.json` 逐項引用真實命令與剩餘驗收條件。`verification/sandbox-ide-release-validation.json` 記錄 **17 個命令全部 exit 0、200 Rust tests 通過、6 ignored、source_changed=[]**，包括 workspace fmt/test/clippy、另行執行的 3 項原生沙箱測試、release build、CLI/stdio/HTTP/cancellation、真後端、advisory/licenses/supply-chain、source/requirements/publish-policy。其餘 ignored 真 LSP probes 與獨立語言矩陣各有自身證據範圍。

被測 clean revision 為 `c5084f4947fc4a07c9ab381fa9998ba1a1e2246d`；release binary SHA-256 為 `f7e57a5a25229426b62be622770ac2d6b8f7cfd3fcbf47dbf25254aa739eb875`。後續語言驗收 driver、文件與 evidence commit 可以不同；核心來源、實際 driver、binary 及交付 manifest 分別據實綁定。

## 尚未完成

40 項需求維持 38 PARTIAL、2 NOT_RUN。Windows 原生沙箱已有上述限定實測；Linux/macOS native sandbox、完整 OS 隔離矩陣、乾淨 Windows／跨版 installer lifecycle、Defender/CFA、全語言與完整 IDE inspections/refactor parity、性能／品質 benchmark 仍未完成。28 個 language profiles 是安裝指引，不是 28 語驗收。

Move/Inline/QuickFix 是後端 edit 的安全入口；成功 fixture 不代表真實語言重構編譯成功。Safe Delete 仍限靜態 references。記憶仍是 host central storage，沒有自動 repo-local migration；索引是查詢前增量 FTS5，沒有背景 watcher；flow 不是完整 DAG scheduler/Python REPL。未配置 MXC 的 LSP／DAP／固定命令只有程序清理，沒有 OS 隔離；inspections 則拒絕執行。

詳細限制見 [KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md)，後端分級見 [LANGUAGE_SUPPORT.md](docs/LANGUAGE_SUPPORT.md)，40 條原規格見 [FEATURE_SPEC.md](docs/FEATURE_SPEC.md)。

## 本機使用與發布

建置／執行入口見 [README.md](README.md)，平台與設定見 [PLATFORM_IMPLEMENTATION.md](docs/PLATFORM_IMPLEMENTATION.md)。一般 Cargo 命令使用 `--locked`；不要自行放寬 host 權限或把 repository 檔案當可執行 host 設定。

公開儲存庫為 [stevenke1981/odin](https://github.com/stevenke1981/odin)；本輪實際推送、remote HEAD、public visibility 與匿名 HTTP 結果以 [delivery-status.json](verification/delivery-status.json) 的 publication 欄位為準。`publication.json` 保留先前發布範圍。沒有啟動雲端 CI，也沒有建立本輪 GitHub binary release。發布流程見 [PUBLISH_GITHUB.md](docs/PUBLISH_GITHUB.md)。
