# Windows 與 Linux 安裝包

Odin app 是 CLI、MCP 服務與本機 Dashboard。安裝不會自動啟動 daemon、
修改 MCP client、授予 shell/debug/write，或替使用者安裝語言伺服器。

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

最低系統是 Windows 10/11 x64，UCRT 與系統 DLL 由 OS 提供。
封裝會遞迴檢查 PE imports，只從 Visual Studio 官方 VC/Redist/x64 CRT 目錄
帶入需要的 app-local runtime，逐一要求有效 Microsoft Authenticode 簽章。
缺少 runtime、未知 DLL 或 delay-load imports 會停止封裝，要求明確檢視。
套件內 `windows-runtime-dependencies.json` 記錄來源、版本、hash 與系統相依；
`MICROSOFT_RUNTIME_NOTICES.txt` 說明 Microsoft 個別授權與官方再散布清單。

驗證器在新的 `dist/installer-qa-*` 目錄實際安裝、同版重裝與卸載，
檢查 portable／安裝內容 inventory、binary hash、metadata、使用者檔案保留與
PATH 不變。`/ODIN_QA=1` 不寫一般 uninstall 登錄項，所有程序隱藏執行。
正常安裝保留標準每使用者 uninstall 登錄。測試沒有驗收乾淨 Windows、
跨版本升級、中斷回復、SmartScreen 或 Defender/CFA。

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
