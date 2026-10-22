# Windows 與 Linux 安裝包

Odin app 是 CLI、MCP 服務與本機 Dashboard。Windows 包含 14 種語言 profiles、12 套預設 LSP 與 Ruff／ty／vtsls 三套明確替代後端，固定 Node／Go／.NET runtime 一併離線部署，版本表見 [EXPANDED_LSP.md](EXPANDED_LSP.md)。Rust toolchain 與 Python interpreter 仍需主機提供。Linux／macOS 封裝路徑需另行配置後端；Windows payload 不代表其他平台驗收。
安裝不會自動啟動 daemon、修改 MCP client 或 PATH，也不會提高 write/shell/debug。
既有 host grants 與操作者 Rust profiles 保持原值；新 workspace 預設唯讀。
`odin.example.toml` 是未啟用的 inert 範本；實際設定由下述 setup 建立。

[Inno Setup 官方說明](https://jrsoftware.org/isinfo.php) 指定 Windows 應用程式；
Windows 使用已安裝的 Inno Setup 7 產生 `.exe`，Linux 使用原生 `.deb` 與
portable `.tar.gz`。不得把 Inno EXE 或 Wine 執行稱為 Linux 安裝包驗收。

## Windows x64

先執行 workspace 的 fmt、tests 與 strict Clippy，再由乾淨來源編譯 release。
使用已選定的執行檔時，明確提供其 SHA-256；封裝不會重新編譯。

```powershell
$binary = (Resolve-Path .\target\release\odin.exe).Path
$sha = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
.\scripts\package.ps1 -SkipBuild -Binary $binary -ExpectedBinarySha256 $sha `
  -LspPayload C:\OdinBuild\prepared-backends `
  -Iscc 'C:\Program Files\Inno Setup 7\ISCC.exe' -OutputDirectory .\dist\windows-candidate
.\scripts\verify_installer.ps1 `
  -Installer .\dist\windows-candidate\odin-0.2.0-windows-x64-setup.exe `
  -Portable .\dist\windows-candidate\odin-0.2.0-x86_64-pc-windows-msvc.zip `
  -ExpectedBinarySha256 $sha
```

輸出包含 ZIP、Inno EXE、`SHA256SUMS`、`build-manifest.json` 與
`package-report.json`。缺少 Inno 時 installer 維持 `NOT_RUN`；不冒充已打包。
封裝拒絕覆寫同名產物，請使用新輸出目錄。Inno 以 per-user 模式安裝，
安裝目錄由使用者選擇；不改 PATH，使用完整路徑或自行設定 PATH 執行。
`-LspPayload` 為必要輸入，必須是含 `manifest.json`、完整 runtime/LSP closure 的
已準備目錄；不是 repository 中任意可執行設定。封裝拒絕 links/junctions，逐檔
驗證 manifest hash、必要 component 與 entry points，再建立含完整 closure 的 ZIP64。
封裝不下載依賴、不執行 npm hooks。manifest 是內容一致性清單，未簽章產物不具
發行者認證；component 版本、來源與授權必須隨 payload 保留。

最低系統是 Windows 10/11 x64，UCRT 與系統 DLL 由 OS 提供。
封裝會遞迴檢查 PE imports，只從 Visual Studio 官方 VC/Redist/x64 CRT 目錄
帶入需要的 app-local runtime，逐一要求有效 Microsoft Authenticode 簽章。
缺少 runtime、未知 DLL 或 delay-load imports 會停止封裝，要求明確檢視。
套件內 `windows-runtime-dependencies.json` 記錄來源、版本、hash 與系統相依；
`MICROSOFT_RUNTIME_NOTICES.txt` 說明 Microsoft 個別授權與官方再散布清單。

驗證器在新的 `dist/installer-qa-*` 目錄實際安裝、同版重裝與卸載，
檢查 portable／安裝內容 inventory、binary hash、metadata、使用者檔案保留與
PATH 不變，並對 portable 及 Inno 安裝結果實際執行19 個 LSP／runtime 版本命令、
健康檢查前後 closure hash、生成 host config 的 Odin validate、同版重裝設定保留。
這些是版本／安裝驗證；真正 document symbols、definition 等語意結果仍需獨立 MCP
整合測試，不能由版本成功推論。
`/ODIN_QA=1` 不寫一般 uninstall 登錄項，並強制要求獨立 `/ODIN_HOST_CONFIG=`；
驗證腳本使用自己的 workspace／host／receipt，所有程序隱藏執行。
正常安裝保留標準每使用者 uninstall 登錄。測試沒有驗收乾淨 Windows、
跨版本升級、中斷回復、SmartScreen 或 Defender/CFA。

## Windows：安裝時設定 LSP

Inno 安裝頁要求選定專案外的絕對 host config 路徑，預設為
`%USERPROFILE%\.config\odin\host.toml`。新設定的 workspace root 預填使用者 home，
也可明確改成其他既存目錄；既有設定未給新 root 時保留原 roots/default/grants。
Home 下的 host/state/install 由強制私有樹保護，詳見 [Home 與進度](HOME_AND_INSTALLER_PROGRESS.md)。安裝程式先複製固定 payload，再由 bundled Node 執行
`setup-lsps.mjs --install`，依序完成 plan、版本健康檢查及 expected-hash apply。
任何一個必要元件失敗或 payload 在健康檢查時改變，都不套用新 host config。

```powershell
# 標準使用者 silent 安裝；workspace root 是操作者明確選定的範圍。
.\odin-0.2.0-windows-x64-setup.exe /VERYSILENT /NORESTART `
  /ODIN_HOST_CONFIG="C:\Users\me\.config\odin\host.toml" `
  /ODIN_WORKSPACE_ROOT="D:\Source\my-project"
```

既有 workspace 與所有已註冊 projects 會加入隨附 profiles；既有 roots、write/shell/debug
等 grants、commands、其他設定值及操作者 Rust／`.rs` profiles 保留；未修改的受管 Rust profile 可隨安裝路徑更新。TOML 會重新排版，註解不保留。
只有 marker 中 digest 仍吻合的受管 profile 可以自動更新。自訂同名 profile 預設
產生 CONFLICT；操作者審查 plan 後，才可在安裝頁勾選替換，或明確傳
`/ODIN_REPLACE_LANGUAGE_PROFILES=1`。其他自訂 language ID 若已佔用隨附語言副檔名，
須先明確編輯該 host profile，不能靠替換選項刪除不相關 profile。

Portable 解壓後使用同一支腳本：

```powershell
$app = 'C:\Tools\Odin'
& "$app\backends\node\node.exe" "$app\setup-lsps.mjs" `
  --host-config 'C:\Users\me\.config\odin\host.toml' `
  --workspace-root 'D:\Source\my-project' --plan
# 審查 before_sha256、plan_sha256、changes 及 collisions 後套用。
& "$app\backends\node\node.exe" "$app\setup-lsps.mjs" `
  --host-config 'C:\Users\me\.config\odin\host.toml' `
  --workspace-root 'D:\Source\my-project' --apply `
  --expected-sha256 absent --expected-plan-sha256 '<reviewed-plan-sha256>'
& "$app\backends\node\node.exe" "$app\setup-lsps.mjs" --health
```

`absent` 只適用新檔；既有檔必須使用 plan 的確切 SHA-256。需要替換自訂 profile 時，
plan 與 apply 都應明確加入 `--replace-language-profiles`。Host config、state、marker、
Lua JSON 與 receipts 一般必須在 registered／allowed workspace roots 之外。
固定 home host 的例外會保護整個 host 目錄、state、install 與 client 憑證樹；
home 內的 receipts 只准 create-only 寫入受保護的標準 host 目錄。
Apply 以獨占 lock、重驗 hash、備份及同目錄 staged replace 處理衝突；拒絕連結、
hardlink host file 及第三方變更。這不是多檔瞬間原子交易，也沒有宣稱消除外部程序
在最後 CAS 檢查與 rename 間的競態。安裝後 receipt 與備份寫在 host 目錄，卸載保留。
移動 portable 後，應在新位置重新 plan/apply，使絕對 backend 路徑更新。

Python／JS／TS 可用明確 provider flags 切換；重裝未指定時保留受管選擇，見 [擴充設定](EXPANDED_LSP.md)。固定 profiles 的邊界：

- Python 預設 Node basedpyright；Python 執行器、虛擬環境及專案依賴由操作者管理。
- JavaScript／TypeScript 預設共用 TSLS 與固定 bundled tsserver，停用自動 typing acquisition。
- Go 固定 runner 設 GOROOT、GOTOOLCHAIN=local、GOFLAGS=-mod=readonly 與 host cache。
  安裝離線；日常 gopls 可透過官方 Go proxy／sumdb 下載缺少的 modules 到 cache，
  不自動下載新 toolchain。
- C# 固定 runner 選 bundled .NET SDK／CSharpLanguageServer.dll，cache 位於 host state。
  建置、runner／health 與語意測試明確固定 `DOTNET_ADD_GLOBAL_TOOLS_TO_PATH=false`、
  `DOTNET_GENERATE_ASPNET_CERTIFICATE=false`，避免 SDK first-use 修改使用者 PATH 或產生
  開發憑證；這兩項預設行為見 [Microsoft 文件](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-environment-variables#dotnet_add_global_tools_to_path)。
  真實專案的 SDK、套件與 design-time MSBuild 行為仍受其專案特性影響。
- Lua 固定 runner 使用 host-owned 純 JSON `--configpath`，停用 runtime.plugin、
  third-party checking／addon manager，每次程序使用獨立 log/meta cache。版本健康檢查
  也明確指定 private temp 路徑，禁止在 bundled Lua 目錄產生 log。
- Bash 使用固定 Node entry，停用可另行執行的 shellcheck／shfmt 及 explainshell endpoint；
  這不代表已包含 shellcheck、formatter 或每一種 shell 方言。

Runner 只接受固定 backend 名稱，不接受自由 program／argv／env，不使用 shell 或 eval。
Child environment 僅傳必要 Windows runtime 變數並覆寫 SDK 設定，不傳任意 host tokens。
每次 Go／C#／Lua 啟動驗證 manifest 結構與必要 launcher hash；完整 SDK／modules closure
於 setup、`--verify-payload`、`--health` 驗證。日常 runtime 信任 host-owned 安裝目錄，
不能把這個較快檢查宣稱為每次完整防竄改。沒有因此開啟 OS sandbox，backend 仍有
主機程序權限；cache 保留且未實作完整 retention。對 client 使用方式仍需設定同一
外部 host config 及 `--workspace <root>`，安裝不自動修改 client。

版本健康檢查、完整 closure、installer lifecycle、真實逐語 LSP、乾淨主機、跨版本
升級與中斷 rollback 各自記錄；新增腳本或某一層 PASS 不替代其他層驗收。
安裝最後階段有獨立視窗顯示真實檔案 hash／LSP 探測／提交進度，UI 持續處理訊息；
每個 probe 最多 60 秒、worker 預設總期限 900 秒。可停止驗證，提交階段不強殺；
停止不回滾已複製的應用檔案。失敗完成頁與 exit code 不會冒充成功。
安裝器會傳回 post-install setup 的失敗 exit code；不能只靠 Inno 的檔案複製成功
宣稱 LSP 設定成功。QA 模式另外以 recipe Check 禁止建立正常使用者的文件捷徑。

以下針對真正 production Inno recipe 建立小型、無 LSP 的私有 argv fixture，實際
安裝／呼叫 bundled Node recorder／卸載，驗證含空白與尾反斜線的參數、強制 child
失敗 exit code、receipt 路徑拒絕與原有捷徑保留；輸出報告不視為產品安裝驗收。

```powershell
python tests/inno_argv_probe.py `
  --node 'C:\OdinBuild\prepared-backends\node\node.exe' `
  --iscc 'C:\Program Files\Inno Setup 7\ISCC.exe'
```

## Linux x86-64

Debian package 名稱是 `odin-agent`，安裝 binary 到 `/usr/bin/odin`，文件、
schema 與設定範例到 `/usr/share/odin-agent/`。套件不含 maintainer shell scripts，
不建立服務、修改 host config、改 PATH 或刪除使用者 state。
卸載由 dpkg 移除套件擁有的檔案；使用者另加檔案保留。

這個 `.deb` recipe 僅接受 `x86_64-unknown-linux-musl`、無 `PT_INTERP` 的
static ELF。Windows 可以使用 pinned Rust 1.90.0 與
[Zig 官方發行](https://ziglang.org/download/) 的 C compiler 交叉編譯，
但真正封裝前必須在 Linux 執行該 ELF 的 metadata。跨編譯與 target 執行分開記錄。
一般 GNU Linux／macOS 原生 build 可繼續使用 `scripts/package_tar.py` 建立 tar。

`bash scripts/package.sh <fresh-output-dir>` 在 Linux x86-64 明確建置 musl target，
產生 `.deb` 與 tar；需先有 `musl-tools`（或明確設定的同等 C compiler/linker），
以及 Rust 1.90.0 的 `x86_64-unknown-linux-musl` standard library。
預設以 `musl-gcc` 編譯 bundled SQLite C，但 Rust 的 self-contained static CRT
由 `cc` driver 連結；Debian 的 `musl-gcc` driver 會注入 dynamic interpreter，
不能直接取代這個 Rust linker。封裝會拒絕帶 `PT_INTERP` 的 ELF。
使用 rustup 的建置機可先執行 `rustup target add --toolchain 1.90.0 x86_64-unknown-linux-musl`。
入口保留 macOS 原生 tar 路徑；沒有把 macOS 視為 Linux `.deb`。

```bash
# Run on Linux, using the actual selected ELF; no automatic dependency installation.
sha="$(sha256sum /path/to/odin | cut -d ' ' -f 1)"
python3 scripts/package_linux.py --binary /path/to/odin \
  --expected-binary-sha256 "$sha" --output dist/linux-candidate
(cd dist/linux-candidate && sha256sum --check SHA256SUMS)
sudo dpkg --install dist/linux-candidate/odin-agent_0.2.0_amd64.deb
odin metadata
sudo dpkg --remove odin-agent
```

安裝生命週期測試會實際修改 guest 的 dpkg 狀態，只能在明確選定的可丟棄 VM
以 root 執行。它先拒絕既有 Odin 套件／非套件 binary，再驗證 portable inventory、
ELF 執行、安裝、重裝、卸載及使用者檔案保留。

```bash
python3 scripts/verify_linux_installer.py --disposable-vm \
  --package dist/linux-candidate/odin-agent_0.2.0_amd64.deb \
  --portable dist/linux-candidate/odin-0.2.0-x86_64-unknown-linux-musl.tar.gz \
  --expected-binary-sha256 "$sha" --output dist/linux-installer-qa
```

本機 Linux 驗收可使用 [QEMU 官方下載頁](https://www.qemu.org/download/) 所列的
Windows distributor，以 TCG、無 GUI、loopback serial console 執行
[Debian 官方 cloud image](https://cloud.debian.org/images/cloud/)。下載工具與 base image
先比對發行者 checksum，只解壓工具、不開啟 Windows OS features，base image 保持
唯讀並建立 writable overlay。TCG guest 的成功不等於實體 Linux、其他 distribution
或 Linux OS sandbox 支援。此版本 Linux OS sandbox 仍未實作。

Windows 與 Linux 安裝包均未簽章。最終 candidate 的版本、來源、binary／產物 hash、
實際命令與退出碼應以 `verification/package-v3-*` 報告為準；有腳本不代表已執行。
