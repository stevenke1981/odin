# 發行門檻與證據等級

## 本次交付

本版是 SOURCE_PREVIEW。`verification/delivery-status.json` 為交付時狀態；`source-check.json` 是檔案與契約靜態檢查；`fixture-selftest.json` 只驗證 Python mock peers 自己能回覆 fixture protocol。它們都不是 Rust 編譯證據。

禁止以「有 test function」、「有 CI YAML」、「工具名稱已列出」、「fixture 回傳成功」推論產品完成。

## 第一層：可重現建置

在實際主機安裝指定 toolchain、C/C++ linker 與 Python。先 bootstrap 產生 Cargo.lock 並檢視相依，再保留 lock。完成 cargo fmt、cargo test --workspace --locked、cargo clippy --workspace --all-targets --locked -- -D warnings、cargo build --workspace --release --locked。失敗修正後重新執行，保存完整 log、版本與 commit/hash。

此版沒有宣稱 lockfile-ready reproducible build；首次 resolve 前，遞迴相依並未凍結。

## 第二層：可執行整合

對實際 Odin binary 執行 scripts/mcp_smoke.py：initialize、tools/list、scope、read/write policy、prepare/apply/rollback、flow、原生輸出格式。再驗證 HTTP auth / Host / Origin、stdio 不混入 logs、正常與意外退出。

Rust fixture tests 只驗證 Odin 的 transport client 與可控制的 peer；真實協議差異另外驗收。`cargo test -p odin-protocol --test fixture_protocols real_rust_analyzer_smoke -- --ignored --nocapture` 需已安裝 rust-analyzer，不能默認算入普通測試通過數。

## 第三層：真實後端矩陣

至少 Rust/rust-analyzer、Python、TypeScript 各自記錄 OS、server version、workspace config、符號、跨檔 references、rename、diagnostics。DAP 需實際啟動 debuggee、斷點、單步、variables、退出。獨立性回歸需在未安裝 Serena 的環境執行，並檢查舊配置與舊工具明確拒絕；不得靠上游轉發通過功能驗收。

Move／Inline／inspection 等進階功能需另外原生實作及逐語言驗證；沒有完成前保持 NOT_IMPLEMENTED。

## 第四層：平台與失敗情境

Windows x86_64 MSVC、Linux x86_64、macOS arm64 各自 native build/test。測試 CRLF/Unicode/長路徑、權限、symlink/junction、缺檔、外部並行寫入、磁碟寫失敗、journal 中斷、重啟恢復、backend hang / malformed messages、後代程序清理。

本版尚無 installer；沒有 Inno Setup 或 GUI lifecycle 完成證據。

## 第五層：性能與完整性比較

固定對照環境與完成定義。測量實際任務成功率、重構後 build/test、p50/p95 工具延遲、CPU/RSS、token/bytes、冷暖索引、復原成功率。公布所有失敗樣本，沒有完成前不得宣稱比 Serena 更快或百分比節省。

## CI 成本

附 workflow 只有 workflow_dispatch 且使用 self-hosted runner；沒有自動 push/pull_request 雲端 CI。不會因使用此 ZIP 自動建立倉庫、啟動 runner 或消耗使用者額度。CI 本身本次 NOT_RUN。

## 規格驗收的附加門檻

`python scripts/check_requirements.py` 僅證明 40 個需求與 53 項上游參考工具間的文件追蹤完整；**不能當 Rust 測試通過**。需求的實際 DoD、負例、真實後端、跨平台及發行條件在 [FEATURE_SPEC.md](FEATURE_SPEC.md)，每項未驗收前 `requirements/features.json` 保留現況。
