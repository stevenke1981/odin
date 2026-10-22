# Windows 擴充 LSP

安裝與 portable setup 配置 14 種主要語言 profile，預設 12 套實作；另附 3 套明確替代 provider，共 15 套 LSP。Registry 的 32 筆語言指引仍與實際隨附清單分開。每個副檔名只配置一個 provider，沒有隱藏 fallback。

| 語言 | 預設固定後端 | 隨附版本 |
|---|---|---|
| Rust | rust-analyzer | 0.3.3089（2026-10-19 release） |
| Python | basedpyright | 1.40.2 |
| Go | gopls | 0.24.0 |
| C# | csharp-ls | 0.28.0 |
| Lua | LuaLS | 3.19.2 |
| JavaScript、TypeScript | typescript-language-server、TypeScript | 6.0.2、6.0.3 |
| Bash | bash-language-server | 5.8.1 |
| C、C++ | clangd | 23.1.0 |
| Slint | slint-lsp | 1.18.1 |
| TOML | Taplo | 0.10.0 |
| YAML | yaml-language-server | 2.0.0 |
| Markdown | Marksman | 2026-02-08 |

替代 provider：Ruff 0.17.1、ty 0.0.90、vtsls 0.3.0。隨附 Node 24.21.0、Go 1.27.3、.NET SDK 10.0.402／runtime 10.0.13；Marksman 另有其上游內嵌 .NET runtime。`smol-toml` 是 host 設定的資料 parser，不是 LSP。19 個版本 probes 包括 runtime／編譯器，所以不等於 19 套 LSP。

## 選擇 provider

Python 預設 basedpyright，JS／TS 預設 TSLS。可在 portable setup 的 plan 和 apply 使用同一組選擇：

```powershell
$app = 'C:\Tools\Odin'
& "$app\backends\node\node.exe" "$app\setup-lsps.mjs" `
  --host-config 'C:\Users\me\.config\odin\host.toml' `
  --python-backend ty --typescript-backend vtsls --plan
# 審查確切 before_sha256、plan_sha256 與 collisions。
& "$app\backends\node\node.exe" "$app\setup-lsps.mjs" `
  --host-config 'C:\Users\me\.config\odin\host.toml' `
  --python-backend ty --typescript-backend vtsls --apply `
  --expected-sha256 '<before_sha256>' --expected-plan-sha256 '<plan_sha256>'
```

`--python-backend` 接受 `basedpyright|ty|ruff`；`--typescript-backend` 接受 `typescript-language-server|vtsls`。受管 marker 保留所選 provider，重裝省略 flags 不會重置。更改自訂非受管 profile 另需明確替換決定。操作者 Rust／`.rs` profile 始終保留；若它同時佔用其他隨附副檔名而造成 shadow，setup 拒絕並要求明確修正，不能只靠 replace flag 取得成功。

Ruff 提供 lint diagnostics、格式化與 code actions；它不提供 Python 語意導航／型別分析。ty 為 0.0.x 的早期替代方案，只有實際廣告能力與執行證據能支持功能主張。這裡選 LSP 不會授予 native Ruff inspection／stdin formatter 的額外 shell、write 或 sandbox grants。後者仍走獨立 host 政策。

## 設定與能力邊界

Rust toolchain 沒有隨附；rust-analyzer 停用 checkOnSave、build scripts、proc macros。既有自訂 Rust 後端可以保留自己的主機政策。Python interpreter／venv、專案套件、外部 C/C++ SDK 與 build metadata 需由主機提供。

Clangd 停用 repository `.clangd` config，不開啟 query-driver wildcard；`compile_commands.json` 是後端讀取的 build metadata。C profile 擁有 `.h`，C++ 與它共用 clangd；fixture 必須實測 C++ header，不能由副檔名推測語意。Taplo 停用自動 config/schema discovery；YAML 停用 schemaStore，不宣稱 modeline schema 網路已受 OS 封鎖。vtsls 使用固定 bundled TypeScript 且不自動選 workspace tsdk。

Slint／TOML／YAML／Markdown 的驗收依實際 capabilities 分項；不保證全部 rename、references、refactors。Bash 未附 ShellCheck、shfmt、man/help 工具。沒有 Tree-sitter parser、Custom RulesServer 或程式碼關係圖的新實作；分享中的架構建議只作為後續參考。

新 workspace 仍唯讀，shell/debug/inspection grants 關閉。LSP 是可信 host 程序；native 路徑隔離不等於 OS sandbox。不要把任意 repository 程式當 executable host 配置。Home 私有樹與提交／取消邊界見 [HOME_AND_INSTALLER_PROGRESS.md](HOME_AND_INSTALLER_PROGRESS.md)。

## 來源、授權與驗收

固定 assets 與 license URL/hash 在 `packaging/lsp-native-sources.json`；完整 payload 每檔在 `backends/manifest.json`。Slint LSP 是獨立 GPL-3.0-only 執行檔，附壓縮對應來源側錄（Slint、Skia、993 crate、14 native dependency archives）。1,009 個來源摘要可檢查，實際重建官方執行檔為 `NOT_RUN`。Odin 沒有連結 Slint API。

Notice inventory 保留 `WITH_DECLARED_NOTICE_GAPS`：Taplo 6、Ruff 9、ty 9、rust-analyzer 7、Marksman 3 筆上游補充授權文字缺口。Cargo lock supersets 含 dev／非 Windows packages，彼此可重複；它們不是精確 linked SBOM。版本、hash、側錄不等於法律認證或簽章。

最新 source-bound 的 Rust gates、真實 LSP MCP、Home、完整安裝／重裝／移除結果在 [本輪驗收](../verification/home-root-installer-validation.json)。每一項保留實際命令、exit code 與 binary／manifest hash；尚未執行的項目保留 `NOT_RUN`。版本健康檢查與 protocol fixtures 不替代語意證據。Linux／macOS、乾淨 Windows、完整 client 應用矩陣與上游重建仍需個別驗收。

Slint 的三個 MSVC DLL 以固定 SHA／版本與 Microsoft 有效簽章一併封裝，位於 Slint 程式旁；沒有安裝全域 runtime 或要求終端使用者安裝 Visual Studio。建置端需明確提供官方 VS x64 CRT 目錄，指令見後端建置文件。實際 module path 只能證明測試主機從 app-local 載入；乾淨主機矩陣仍為 `NOT_RUN`。上游 Slint 額外 direct-protocol shutdown probe 的 duplicate response／exit 3 另列為未解限制，不以版本或符號測試替代正常 shutdown 證據。
