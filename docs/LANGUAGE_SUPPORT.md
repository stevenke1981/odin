# 語言與除錯後端支援矩陣

Odin 直接連接 host 設定的 LSP/DAP，不包含 Serena bridge。**configured、已握手與特定功能已驗證是不同狀態**；generic LSP transport 不等於 40+ 語言全部支援。以下限於 Windows x64 MSVC 與報告所記 binary digest，沒有跨平台或完整 IDE 等價主張。

最新六語驗收各使用兩個獨立受控專案與真實後端，經 Odin MCP 執行；不是 mock
server。下表計數包含初始化、來源不變、跨專案隔離與交易檢查，不等於功能數。

| 後端 | 實際版本 | 兩專案結果 | 具體缺口 |
|---|---|---|---|
| Rust / rust-analyzer | 1.90.0 (1159e78c) | 48 PASS | 本 fixture 外的語法／巨型 workspace、語言專用重構與其他 OS 未完整驗收 |
| Python / basedpyright | 1.40.2 | 40 PASS、4 UNAVAILABLE | 未提供 inlay-hint resolve 與格式化；versioned diagnostics 正反例／恢復已通過 |
| TypeScript / typescript-language-server | 6.0.2；tsserver 6.0.3 | 40 PASS、2 UNAVAILABLE、6 NOT_VERIFIED | 未提供 inlay-hint resolve；三階段診斷未附 document version |
| Go / gopls | 0.24.0；Go 1.27.3 | 44 PASS、4 UNAVAILABLE | 未提供 completion resolve 與 inlay-hint resolve；semantic tokens 需明確啟用 |
| Java / JDT LS | 1.62.0；Microsoft JDK 25.0.4.1 | 40 PASS、2 UNAVAILABLE、6 NOT_VERIFIED | 未提供 inlay-hint resolve；診斷未附 version；signature help 需明確啟用 |
| C# / csharp-ls | 0.28.0；.NET SDK 10.0.402 | 48 PASS、2 UNAVAILABLE | 未提供 inlay-hint resolve；完整 framework／dependency／solution matrix 尚未驗收 |
| Python / debugpy DAP | 1.8.22 | 獨立 29-check 報告中的真除錯子集 PASS | host-process debuggee，非本輪 OS-sandbox DAP 證據；其他 adapter／OS 與完整生命週期待驗收 |

完整結果見 [language-ide-final-baseline-03.json](../verification/language-ide-final-baseline-03.json)
與其 command receipt／request transcript：實際 exit 2、12 個 LSP 初始化、
**260 PASS／14 UNAVAILABLE／12 NOT_VERIFIED／0 FAIL，整體 PARTIAL**。
實測 symbols、cross-file references、definition/consumer rename CAS apply／rollback、
completion／resolve、hover、signature、workspace／call hierarchy、inlay hints、
semantic tokens、適用後端的格式化交易及 diagnostics 正反例／恢復。
未提供能力與無 version 診斷保留原狀態；空 diagnostics 不當成 clean build。
baseline profiles 明確在 host 執行，OS 沙箱結果另見交付狀態。

[sandbox-ide-release-real-backends.json](../verification/sandbox-ide-release-real-backends.json)
另有同 binary 的三語 references／rename 與 debugpy 共 29 項 PASS；其命令 exit 0
記於 `sandbox-ide-release-validation.json`。舊 binary、indexing、TS7、URI mismatch
與 driver 順序失敗各保留歷史報告，不能取代本輪結果。

[protocol-verification.json](../verification/protocol-verification.json) 另記 Rust 協定套件的真 rust-analyzer smoke，與 Python protocol fixture tests 分開。protocol peers 驗證 framing、capability、版本、交易與錯誤路徑，不是真實語言、JetBrains inspections 或 debugger 功能等價證明。

## 能力與限制

- `get_capabilities` / `doctor --probe-lsp` 執行 capability handshake；索引是否完整仍取決於 backend evidence。未知 readiness 保持 unknown。
- Move/Inline/QuickFix 要求後端提供相應 kind 的可用 edit；只有 command、disabled、unsupported 或模糊 action 會拒絕，沒有文字替換 fallback。
- WorkspaceEdit 只處理 regular UTF-8 files 與既有父目錄；directory、recursive、binary、overwrite 與需要確認的 annotations 明確拒絕。
- diagnostics 必須區分 pending、stale、unversioned、available；有 diagnostics 不證明 clean build。獨立 Clippy/Ruff/ESLint providers 已有同 binary 的 30-check 原生沙箱驗收，仍不代表全部規則與私有 IDE inspection parity。
- host 明確決定 executable、argv、write/shell/debug 權限。外部後端可能需要 Node/Python/Java 等 runtime；Odin 核心檔案/記憶功能不需要 Python。

## 新增支援的驗收

新增語言需記錄 server/runtime/OS/version、workspace config、至少兩個專案及反例；逐項測 symbols、references、rename、diagnostics、refactor、清理與失敗處理。目前六語受控專案只證明報告所列子集；第三方依賴、完整 build／launch、真實大型專案、完整重構與所有 OS 仍需新增驗收。未實測功能保留 NOT_RUN，後端未提供的能力才標 UNAVAILABLE，不用工具數量替代 support matrix。28 個 profiles 只是指引，JavaScript 等其他語言不因 registry 存在而取得 PASS。

本輪 release revision：`c5084f4947fc4a07c9ab381fa9998ba1a1e2246d`；binary SHA-256：`f7e57a5a25229426b62be622770ac2d6b8f7cfd3fcbf47dbf25254aa739eb875`。逐後端設定、driver source／hash 與結果語意見 [LANGUAGE_BACKENDS.md](LANGUAGE_BACKENDS.md)。
