# Produce a portable archive and, if detected or supplied, an Inno Setup installer.
param([switch]$SkipBuild, [switch]$SkipInstaller, [string]$Binary = '', [string]$ExpectedBinarySha256 = '', [string]$Iscc = '', [string]$RuntimeDirectory = '', [string]$OutputDirectory = '')
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
function Write-Utf8([string]$Path, [string]$Text) { [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false)) }
Push-Location $Root
try {
    if (-not (Test-Path -LiteralPath 'Cargo.lock')) { throw 'A reviewed Cargo.lock is required.' }
    if (-not $SkipBuild) {
        & cargo build -p odin-agent --release --locked
        if ($LASTEXITCODE -ne 0) { throw "Locked release build failed: $LASTEXITCODE" }
    }
    if (-not $Binary) { $Binary = Join-Path $Root 'target/release/odin.exe' }
    $Binary = (Resolve-Path -LiteralPath $Binary).Path
    $BinaryHash = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($ExpectedBinarySha256 -and ($ExpectedBinarySha256 -notmatch '^[a-fA-F0-9]{64}$' -or $BinaryHash -ne $ExpectedBinarySha256.ToLowerInvariant())) { throw 'Binary does not match the selected release candidate SHA-256.' }
    $RawMetadata = & $Binary metadata
    if ($LASTEXITCODE -ne 0) { throw 'Binary metadata command failed.' }
    $Metadata = $RawMetadata | ConvertFrom-Json
    if ($Metadata.target -ne 'x86_64-pc-windows-msvc' -or $Metadata.version -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') { throw 'Unexpected native binary target or version.' }
    if ($Metadata.tracked_source_dirty -ne $false -or $Metadata.profile -ne 'release') { throw 'Packaging requires a clean-source release build.' }
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $Root 'dist' }
    $OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $PackageName = "odin-$($Metadata.version)-$($Metadata.target)"
    foreach ($OutputName in @("$PackageName.zip", "odin-$($Metadata.version)-windows-x64-setup.exe", 'package-report.json', 'build-manifest.json', 'SHA256SUMS')) {
        if (Test-Path -LiteralPath (Join-Path $OutputDirectory $OutputName)) { throw "Refusing to replace existing package output: $OutputName. Select a fresh output directory." }
    }
    $Stage = Join-Path $OutputDirectory ('.stage-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $Stage | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $Stage 'odin.exe')
    . (Join-Path $Root 'packaging/windows-runtime.ps1')
    $RuntimeInfo = Add-OdinWindowsRuntime -Binary $Binary -Stage $Stage -Root $Root -RuntimeDirectory $RuntimeDirectory
    Write-Utf8 (Join-Path $Stage 'windows-runtime-dependencies.json') ($RuntimeInfo | ConvertTo-Json -Depth 12)
    foreach ($Name in @('README.md','LICENSE','THIRD_PARTY_NOTICES.md')) { Copy-Item -LiteralPath (Join-Path $Root $Name) -Destination $Stage }
    Copy-Item -LiteralPath (Join-Path $Root 'docs') -Destination $Stage -Recurse
    Copy-Item -LiteralPath (Join-Path $Root 'schemas') -Destination $Stage -Recurse
    Copy-Item -LiteralPath (Join-Path $Root 'examples/odin.native.toml') -Destination (Join-Path $Stage 'odin.example.toml')
    Write-Utf8 (Join-Path $Stage 'build-metadata.json') ($Metadata | ConvertTo-Json -Depth 20)
    foreach ($Optional in @('docs/THIRD_PARTY_LICENSES.txt','verification/sbom.cdx.json')) {
        $Source = Join-Path $Root $Optional
        if (Test-Path -LiteralPath $Source -PathType Leaf) {
            $Target = Join-Path $Stage $Optional
            New-Item -ItemType Directory -Path (Split-Path $Target -Parent) -Force | Out-Null
            Copy-Item -LiteralPath $Source -Destination $Target -Force
        }
    }
    $Inventory = @(Get-ChildItem -LiteralPath $Stage -File -Recurse | Sort-Object FullName | ForEach-Object {
        [ordered]@{path = $_.FullName.Substring($Stage.Length + 1).Replace('\','/'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
    })
    Write-Utf8 (Join-Path $Stage 'SHA256SUMS.json') (ConvertTo-Json -InputObject $Inventory -Depth 5)
    $Archive = Join-Path $OutputDirectory "$PackageName.zip"
    if (Test-Path -LiteralPath $Archive) { throw "Refusing to replace existing package: $Archive. Select a fresh output directory." }
    Compress-Archive -LiteralPath (Get-ChildItem -LiteralPath $Stage | ForEach-Object FullName) -DestinationPath $Archive -CompressionLevel Optimal
    $InstallerStatus = 'NOT_RUN'
    $Installer = $null
    $CompilerInfo = $null
    if (-not $SkipInstaller) {
        if (-not $Iscc) {
            $Candidates = @((Get-Command ISCC.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source))
            $Candidates += @('C:/Program Files/Inno Setup 7/ISCC.exe','C:/Program Files (x86)/Inno Setup 7/ISCC.exe','C:/Program Files (x86)/Inno Setup 6/ISCC.exe','C:/Program Files/Inno Setup 6/ISCC.exe')
            foreach ($RegistryRoot in @('HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall','HKLM:/Software/Microsoft/Windows/CurrentVersion/Uninstall','HKLM:/Software/WOW6432Node/Microsoft/Windows/CurrentVersion/Uninstall')) {
                $Candidates += @(Get-ChildItem -LiteralPath $RegistryRoot -ErrorAction SilentlyContinue | Get-ItemProperty | Where-Object { $_.DisplayName -like 'Inno Setup*' -and $_.InstallLocation } | ForEach-Object { Join-Path $_.InstallLocation 'ISCC.exe' })
            }
            $Iscc = $Candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -First 1
        }
        if ($Iscc) {
            $Iscc = (Resolve-Path -LiteralPath $Iscc).Path
            $CompilerVersion = (& $Iscc --version | Out-String).Trim()
            if ($LASTEXITCODE -ne 0 -or $CompilerVersion -notmatch '^7\.') { throw 'This release recipe requires the installed Inno Setup 7 compiler.' }
            & $Iscc --quiet-progress --no-ide-signtools --no-signing "--define=AppVersion=$($Metadata.version)" "--define=StageDir=$Stage" "--define=OutputPath=$OutputDirectory" (Join-Path $Root 'packaging/odin.iss')
            if ($LASTEXITCODE -ne 0) { throw "Inno Setup compilation failed: $LASTEXITCODE" }
            $Installer = Join-Path $OutputDirectory "odin-$($Metadata.version)-windows-x64-setup.exe"
            if (-not (Test-Path -LiteralPath $Installer)) { throw 'Compiler reported success without the expected installer artifact.' }
            $InstallerStatus = 'PASS'
            $CompilerInfo = [ordered]@{path=$Iscc; version=$CompilerVersion; sha256=(Get-FileHash -LiteralPath $Iscc -Algorithm SHA256).Hash.ToLowerInvariant()}
        } else { Write-Warning 'Inno Setup not detected. Portable archive exists; installer NOT_RUN.' }
    }
    $Artifacts = @($Archive)
    if ($Installer) { $Artifacts += $Installer }
    $Checksums = @($Artifacts | ForEach-Object { [ordered]@{path = $_; sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()} })
    $Report = [ordered]@{scope = 'WINDOWS_NATIVE_PACKAGE'; portable = 'PASS'; installer_compile = $InstallerStatus; installer_lifecycle = 'NOT_RUN'; binary_sha256 = $BinaryHash; build = $Metadata; staging = $Stage; artifacts = $Checksums; signed = $false; linux_macos_native = 'NOT_RUN'; compiler = $CompilerInfo; runtime_dependencies=$RuntimeInfo; packaging_source_revision = ((& git rev-parse HEAD) | Out-String).Trim()}
    Write-Utf8 (Join-Path $OutputDirectory 'package-report.json') ($Report | ConvertTo-Json -Depth 20)
    Write-Utf8 (Join-Path $OutputDirectory 'build-manifest.json') ($Report | ConvertTo-Json -Depth 20)
    Write-Utf8 (Join-Path $OutputDirectory 'SHA256SUMS') ((($Checksums | ForEach-Object { $_.sha256 + '  ' + [IO.Path]::GetFileName($_.path) }) -join "`n") + "`n")
    $Report | ConvertTo-Json -Depth 20
} finally { Pop-Location }
