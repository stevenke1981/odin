# 操作、安全與發行規格 (RFC-OPS-001)

**地位：原驗收目標及本輪安全實作邊界，非完成公告。** 對應 ODIN-001/002/011/012/023/024/025/027/028/029/030/031/032/033。85個工具契約不代表40需求全部完成；本輪final gates與前版72工具的144-test baseline分開。

## 信任與權限

Workspace、symlinks/junctions、build scripts、LSP/DAP資料、analyzer output、HTTP請求與競爭writer都可能不可信。Host config、executor、固定argv/profiles與明確檔案grants由操作者信任並授權；不自動從repository取得執行政策、shell/debug/write/inspection權限或付費額度。

Core的no-follow directory capabilities、hash/CAS、body/output配額與Transactions保護原生存取。它們不是外部程序OS隔離；Job Objects/process groups只管生命週期，也不是防止同user惡意host的邊界。多檔交易沒有瞬間原子可見性，最後CAS檢查與rename之間的外部競態仍須據實揭露。

## 可選Windows native MXC

Windows host可顯式配置native MXC ProcessContainer、固定base-container tier與executor SHA-256。準備時檢查pin、支援狀態、normalized paths、protected state與最小readonly/readwrite/deny grants。使用native argv，不拼shell，network固定deny；驗證或launch失敗就拒絕，**不退回直接host執行**。Pinned hash降低配置漂移，不等於code signing、可信host或無競態保證。

Workload在開始前進入MXC邊界，policy與private scratch guard保留至child/descendants終止及parent reap；outer Job補取消／後代清理。Runtime/工具鏈/依賴位置必須由host明確授read，cache/scratch另授write；環境allowlist不會自動授filesystem權限。Native timeout、nonzero exit與launch failure要保留真實結果。

| 執行路徑 | Host配置sandbox | 未配置 |
|---|---|---|
| LSP / DAP / 固定command profile | 經native MXC launch；原有tool policy仍生效 | 明確privileged host process，只有環境／lifecycle guards |
| Native inspections | allow_inspections +固定profiles +native sandbox，來源readonly | 拒絕，沒有unsandboxed fallback |
| 原生source edit | Transactions +expected hashes，與外部程序政策各自檢查 | 仍需allow_write，不因其他profile而授權 |

Inspection writable/temp不得與任何source root或Odin state重疊。Repository ESLint executable config不得自動探索；host選用analyzer、Clippy build scripts/plugins仍是受信任配置下的可執行內容。Debugger immutable payload與OS grants是兩項不同host決策，未設payload profile仍明列privileged mode。

Linux/macOS sandbox、BFS/AppContainer fallback、VM backend、完整UI/registry/device/ingress/external-internet與compromised-host矩陣未完成。選用但unsupported/invalid的沙箱必須fail closed；不得把沒有配置說成受限模式。

## 證據分層與本輪狀態

| 層級 | 已觀察 | 不能推論 |
|---|---|---|
| 歷史72工具snapshot | 16 gates、144 Rust tests/1 ignored、三語29 checks；保留原revision/digest | 本輪85工具或新依賴已通過 |
| Sandbox author module | 真native Windows MXC兩測試、具名denyfs/network/descendant/timeout/scratch案例 | 所有Odin dispatch或所有OS資源已隔離 |
| 歷史Protocol/engine slices | 29+22 fixtures、author74/62+12blocked與symbol targeted1保留原source | 不等於最新c5084f4全套或六語逐功能驗收 |
| Native inspections currentc5084f4 | 與17gates相同binary的真MCP/MXC三provider30checks，完整command exit0/log/hash；readonly/safeCAS/cleanup | 不證明全部規則、所有OS或JetBrains-private等價 |
| 六語兩專案首輪 | 4551阻擋於Odin啟動；0 MCP/LSP checks | 版本存在、harness生成即是六語支援 |
| 六語兩專案baseline03 | 同f7 binary；host mode，12/12 initialized；260 PASS／14 UNAVAILABLE／12 NOT_VERIFIED／0 FAIL；完整command actual exit 2、PARTIAL | 不證明TS/Java versioned diagnostics、全部IDE能力、六語MXC或28/40+語言完成 |
| TS native MXC profile01 | actual exit 1／FAIL；2 MCP initialize PASS，private TEMP目錄缺失；4 FAIL、0 LSP initialized | 固定trusted bootstrap的profile02仍pending，不能預寫成功 |
| 當輪release candidate | `sandbox-ide-release-validation.json`的17gates、200tests/6ignored與另native3已PASS；標準三LS/debugpy29PASS | 六語baseline另為PARTIAL／exit 2，TSsandbox另待結果；不可覆寫舊baseline |

Root的guard-until-reap已包含在c5084f4當輪native三測試實跑，command/exit0及log/hash記在sandbox-ide-release-validation.json的/commands/4。Analyzer attempt1、attempt2與AppControl拒絕應保留；第三次成功是修正實質harness/runtime flags，不是降低grants或重試相同假設洗綠。

六語baseline03的report、command、requests與normalization分別綁定真實hash與exit 2。Normalization PASS只代表LF處理不改logical checks；不代表語言功能全PASS。新版driver d47fa6將initial-valid觀察提前至mutation前，AST條件經root/reviewer確認未變；17gate report保留舊QA快照，53個production crates檔案hash仍相同。真rename/apply/rollback包含兩個JDTLS專案；TS/Java診斷無document version，保留NOT_VERIFIED及clean_build_proven:false。

TSsandbox01的private TEMP startup失敗保留；profile02以外部trusted固定bootstrap建立私有TEMP後import真server，仍待實測，不加eval或擴大權限。

每個PASS要有command、exit code、OS/toolchain、source SHA/hash、backend/version、binary digest與原始log。報告source binding不同就分開記；後續文件commit不必等於binary revision，但受測source hashes必須吻合。Public日誌可遮蔽本機路徑並記processing方式，不能改結果或重寫歷史digest。詳見 [feature-evidence.json](../../verification/feature-evidence.json)。

## 完整發行門檻仍保留

- 同一當輪候選執行cargo fmt、workspace test --locked、clippy --all-targets --locked -- -D warnings、release build、真CLI/MCP/HTTP/cancellation及相關native/backend/analyzer驗收。未執行的entry保持NOT_RUN，失敗gate阻止發布。
- Windows x64 MSVC、Linux x64、macOS arm64均需native build/test；cross compile不代替。乾淨核心主機without Python/Serena需另驗，開發harness有Python不能冒充該條件。
- 依賴lock、SBOM、advisory、license、secret及artifact allowlist要涵蓋當輪新增來源；沒有正式security/legal audit的主張。
- 獨立review針對新immutable head；未解阻擋finding不得發布。作者不能核准自己；root整合後的source差異需重新核對證據。
- Installer完整clean-machine、跨版upgrade、取消／中斷rollback、Defender/SmartScreen/CFA、資料保護與無孤兒程序矩陣仍未驗收；簽章若無就明列未簽。舊package PASS不自動適用新85-tool binary。
- 三規模／20個coding任務、cold/warm、P50/P95/CPU/RSS/output/tokens/成功率與失敗樣本benchmark未完成，不宣稱Rust天然更快或省token。

## CI、客戶端與外部發布

只保留workflow_dispatch、trusted self-hosted。公眾PR不得自動在無防護runner執行，不能為跑檢查建立/推remote或啟用hosted額度。本輪文件任務沒有cloud CI或remote副作用。

Client setup/hook先dry-run、diff與備份，既有host檔案需expected hash；只動授權entry，不讀repository設定當執行權限。Generated MCP argv handshake與真Codex/Claude/OpenCode app整合分開；uninstall只回復自有設定。

對外發布前完成同候選review、gates與artifact manifest。已有公開舊source repo不代表本輪已push或binary release；只有實際操作後的public visibility、remote HEAD、URL及artifact digest才可宣稱相應完成。

目前產品revision為`c5084f4947fc4a07c9ab381fa9998ba1a1e2246d`，binary SHA-256為`f7e57a5a25229426b62be622770ac2d6b8f7cfd3fcbf47dbf25254aa739eb875`；metadata為clean。6個workspace ignored中有3個native已另行opt-in跑過，其餘3個真LS probes仍ignored。六語host baseline03另有actual exit 2／PARTIAL證據，TS native MXC profile02、打包與發布不由17gate報告推定完成。
