# 驗收證據與適用範圍

目前交付是 Windows x64 MSVC 原生預覽、72 個工具契約。此目錄同時保留 0.2.0 早期未編譯的歷史快照與本次真實測試；較舊的 `NOT_RUN` 不否定後來已執行的檢查，較舊的 PASS 也不自動適用後續變更。

## 逐需求與整合結果

- `feature-evidence.json`：40 項原驗收條件的逐項整理，38 PARTIAL、2 NOT_RUN。checks 只引用已有 status PASS 且 exit_code 0 的命令；每筆附來源 report/pointer/digest。supporting_reports 是補充觀察，不推造缺少的 exit code。
- `local-validation.json`：由 root 的最終整合 runner 實際執行後產生，記錄 workspace fmt/test/clippy/release build、binary smokes、backend、供應鏈與檢查的 exit code/log/digest。尚未開始、執行中或準備中的 entry 保留 NOT_RUN；不得先把整份報告寫 PASS。
- `delivery-status.json`：由整合 owner 更新的整體交付結果。不能只改需求文件便宣稱 build/release/publish 通過。

報告中的 source SHA 與 source hashes/binary digest 共同界定測試內容。文件/evidence commit 可能晚於被測 binary 的 commit；需要比對實際 source hash，不強行宣稱 SHA 必然相同。公共日誌可使用 `<WORKSPACE>`、`<USERPROFILE>` 代換本機路徑；原始測試結果及失敗不變。

## 已完成切片

| 報告 | 證據層級 |
|---|---|
| `engine-verification.json` | Windows engine package command/exit、來源 hashes；completion 內的語意/DAP 案例使用 protocol fixtures |
| `protocol-verification.json` | 協定 unit/fixture tests、實際 rust-analyzer initialize/documentSymbol、Clippy/rustfmt |
| `platform-verification.json` | CLI/HTTP/setup/hook package command/exit、來源與 binary smoke |
| `platform-mcp-cancellation-verification.json` | 真 binary 取消排隊寫入／pending backend；慢 peer 不是語言能力證明 |
| `platform-*-smoke.json` | 真 binary hash、實際 stdio/HTTP/CLI 子案例；真 client 應用未因此視為已安裝 |
| `real-backends.json` | 真 rust-analyzer/basedpyright/TypeScript LSP 與 debugpy 子案例、版本及 binary digest；完整 command exit 由整合 manifest 補足 |
| `core-*.txt`、`windows-short-aliases.txt` | 原生交易、crash checkpoint、雙程序競爭、NTFS path aliases 的命令輸出；單靠文字結尾不推造 exit code |
| `platform-packaging-recipe.json` | 較早 binary 的 ZIP/Inno recipe 編譯；不代表最終工件或 installer lifecycle |
| `cargo-audit.json`、`license-inventory.json`、`sbom.cdx.json`、`public-candidate-scan.json` | 對當時候選/lock 的 advisory、license、SBOM 與 pattern scan；不是 formal security/legal audit |

`*-first-run`、`*-initial-failure`、`*-before-fix` 與 TS7 failure 等紀錄保留第一次可定位失敗，後續修復與 PASS 另有來源，不能刪掉失敗只保留成功樣本。

## 歷史與靜態檢查

`source-check.json`、`fixture-selftest.json`、`standalone-audit.json`、`generator-regression.json`、`rust-syntax-check.json`、`lexical-check.json` 與 `spec-traceability-20261021.json` 各自只證明報告聲明的範圍。原始 parser 不可用或 58 工具的舊快照不是目前功能清單；Python mock peer 自測與詞法括號檢查永遠不能代替 Rust compiler、真 binary 或真後端驗收。

## 仍未驗收

Linux/macOS native runtime、真正 OS sandbox、乾淨 Windows/跨版 installer/rollback/Defender/CFA、三個真 client 應用、六語言及 40+ 語言、完整 inspections/semantic refactor、性能/品質 benchmark 保持未完成。雲端 CI 未 dispatch；GitHub public URL/remote HEAD 必須在實際發布後另記，不能由本機 PASS 推論。
