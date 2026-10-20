# Produce a portable archive and, if detected or supplied, an Inno Setup installer.
param([switch]$SkipBuild, [switch]$SkipInstaller, [string]$Binary = '', [string]$Iscc = '', [string]$OutputDirectory = '')
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $Root
try {
    if (-not (Test-Path -LiteralPath 'Cargo.lock')) { throw 'A reviewed Cargo.lock is required.' }
    if (-not $SkipBuild) {
        & cargo build -p odin-agent --release --locked
        if ($LASTEXITCODE -ne 0) { throw "Locked release build failed: $LASTEXITCODE" }
    }
    if (-not $Binary) { $Binary = Join-Path $Root 'target/release/odin.exe' }
    $Binary = (Resolve-Path -LiteralPath $Binary).Path
    $RawMetadata = & $Binary metadata
    if ($LASTEXITCODE -ne 0) { throw 'Binary metadata command failed.' }
    $Metadata = $RawMetadata | ConvertFrom-Json
    if ($Metadata.target -ne 'x86_64-pc-windows-msvc' -or $Metadata.version -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') { throw 'Unexpected native binary target or version.' }
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $Root 'dist' }
    $OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $PackageName = "odin-$($Metadata.version)-$($Metadata.target)"
    $Stage = Join-Path $OutputDirectory ('.stage-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $Stage | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $Stage 'odin.exe')
    foreach ($Name in @('README.md','LICENSE','THIRD_PARTY_NOTICES.md')) { Copy-Item -LiteralPath (Join-Path $Root $Name) -Destination $Stage }
    Copy-Item -LiteralPath (Join-Path $Root 'docs') -Destination $Stage -Recurse
    Copy-Item -LiteralPath (Join-Path $Root 'schemas') -Destination $Stage -Recurse
    Copy-Item -LiteralPath (Join-Path $Root 'examples/odin.native.toml') -Destination (Join-Path $Stage 'odin.example.toml')
    $Metadata | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $Stage 'build-metadata.json') -Encoding utf8NoBOM
    $Inventory = @(Get-ChildItem -LiteralPath $Stage -File -Recurse | Sort-Object FullName | ForEach-Object {
        [ordered]@{path = [IO.Path]::GetRelativePath($Stage, $_.FullName).Replace('\','/'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
    })
    ConvertTo-Json -InputObject $Inventory -Depth 5 | Set-Content -LiteralPath (Join-Path $Stage 'SHA256SUMS.json') -Encoding utf8NoBOM
    $Archive = Join-Path $OutputDirectory "$PackageName.zip"
    if (Test-Path -LiteralPath $Archive) { throw "Refusing to replace existing package: $Archive. Select a fresh output directory." }
    Compress-Archive -LiteralPath (Get-ChildItem -LiteralPath $Stage | ForEach-Object FullName) -DestinationPath $Archive -CompressionLevel Optimal
    $InstallerStatus = 'NOT_RUN'
    $Installer = $null
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
            & $Iscc /Qp "/DAppVersion=$($Metadata.version)" "/DStageDir=$Stage" "/DOutputPath=$OutputDirectory" (Join-Path $Root 'packaging/odin.iss')
            if ($LASTEXITCODE -ne 0) { throw "Inno Setup compilation failed: $LASTEXITCODE" }
            $Installer = Join-Path $OutputDirectory "odin-$($Metadata.version)-windows-x64-setup.exe"
            if (-not (Test-Path -LiteralPath $Installer)) { throw 'Compiler reported success without the expected installer artifact.' }
            $InstallerStatus = 'PASS'
        } else { Write-Warning 'Inno Setup not detected. Portable archive exists; installer NOT_RUN.' }
    }
    $Artifacts = @($Archive)
    if ($Installer) { $Artifacts += $Installer }
    $Checksums = @($Artifacts | ForEach-Object { [ordered]@{path = $_; sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()} })
    $Report = [ordered]@{scope = 'WINDOWS_NATIVE_PACKAGE'; portable = 'PASS'; installer_compile = $InstallerStatus; installer_lifecycle = 'NOT_RUN'; binary_sha256 = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash.ToLowerInvariant(); build = $Metadata; staging = $Stage; artifacts = $Checksums; signed = $false; linux_macos_native = 'NOT_RUN'}
    $Report | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'package-report.json') -Encoding utf8NoBOM
    $Report | ConvertTo-Json -Depth 20
} finally { Pop-Location }
