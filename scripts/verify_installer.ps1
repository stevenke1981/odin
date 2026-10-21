# Isolated per-user installer smoke, with app files directed to a new dist/installer-qa-* path.
param([Parameter(Mandatory)][string]$Installer, [Parameter(Mandatory)][string]$ExpectedBinarySha256, [string]$Portable = '')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Installer = (Resolve-Path -LiteralPath $Installer).Path
if ($ExpectedBinarySha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'Provide the packaged binary SHA-256.' }
$Dist = [IO.Path]::GetFullPath((Join-Path $Root 'dist'))
New-Item -ItemType Directory -Path $Dist -Force | Out-Null
$QaRoot = [IO.Path]::GetFullPath((Join-Path $Dist ('installer-qa-' + [guid]::NewGuid().ToString('N'))))
if (-not $QaRoot.StartsWith($Dist + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'QA directory escapes dist.' }
New-Item -ItemType Directory -Path $QaRoot | Out-Null
$App = Join-Path $QaRoot 'app'
$Checks = [Collections.Generic.List[object]]::new()
$Commands = [Collections.Generic.List[object]]::new()
$Failure = $null
$Inventory = @()
$UserPathBefore = [Environment]::GetEnvironmentVariable('Path', 'User')
$Metadata = $null
function Assert-Check([string]$Name, [bool]$Passed) {
    $Checks.Add([ordered]@{name=$Name; status=$(if($Passed){'PASS'}else{'FAIL'})})
    if (-not $Passed) { throw "Installer check failed: $Name" }
}
function ConvertTo-OdinCanonical($Value) {
    if ($null -eq $Value) { return $null }
    if ($Value -is [pscustomobject]) {
        $Map = [ordered]@{}
        foreach ($Property in @($Value.PSObject.Properties | Sort-Object Name)) { $Map[$Property.Name] = ConvertTo-OdinCanonical $Property.Value }
        return $Map
    }
    if ($Value -is [array]) {
        $Items = @($Value | ForEach-Object { ConvertTo-OdinCanonical $_ })
        return ,$Items
    }
    return $Value
}
function Verify-Payload([string]$Directory) {
    $Manifest = Join-Path $Directory 'SHA256SUMS.json'
    $Rows = @(Get-Content -LiteralPath $Manifest -Raw -Encoding UTF8 | ConvertFrom-Json)
    foreach ($Row in $Rows) {
        $Resolved = [IO.Path]::GetFullPath((Join-Path $Directory $Row.path))
        if ($Row.sha256 -notmatch '^[a-fA-F0-9]{64}$' -or -not $Resolved.StartsWith($Directory + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid packaged inventory path or digest.' }
        if ((Get-FileHash -LiteralPath $Resolved -Algorithm SHA256).Hash -ine $Row.sha256) { throw "Packaged payload digest mismatch: $($Row.path)" }
    }
    return $Rows
}
try {
    if ($Portable) {
        $Portable = (Resolve-Path -LiteralPath $Portable).Path
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $Zip = [IO.Compression.ZipFile]::OpenRead($Portable)
        try {
            Assert-Check 'portable_safe_member_paths' (@($Zip.Entries | Where-Object { [IO.Path]::IsPathRooted($_.FullName) -or $_.FullName.Split([char[]]@('/','\')) -contains '..' }).Count -eq 0)
        } finally { $Zip.Dispose() }
        $Extracted = Join-Path $QaRoot 'portable'
        Expand-Archive -LiteralPath $Portable -DestinationPath $Extracted
        $PortableInventory = @(Verify-Payload $Extracted)
        Assert-Check 'portable_inventory_sha256' ($PortableInventory.Count -gt 0)
        Assert-Check 'portable_binary_sha256' ((Get-FileHash -LiteralPath (Join-Path $Extracted 'odin.exe') -Algorithm SHA256).Hash -ieq $ExpectedBinarySha256)
        $PortableMetadata = (& (Join-Path $Extracted 'odin.exe') metadata | ConvertFrom-Json)
        Assert-Check 'portable_metadata' ($LASTEXITCODE -eq 0 -and $PortableMetadata.target -eq 'x86_64-pc-windows-msvc')
    }
    # Disable uninstall registration, icons and interactive prompts for this isolated QA installation.
    $InstallArgs = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-','/NOICONS','/ODIN_QA=1',"/DIR=$App",('/LOG=' + (Join-Path $QaRoot 'install.log')))
    $Process = Start-Process -FilePath $Installer -ArgumentList ($InstallArgs | ForEach-Object { '"' + $_ + '"' }) -PassThru -Wait -WindowStyle Hidden
    $Commands.Add([ordered]@{name='install'; exit_code=$Process.ExitCode; log='install.log'})
    Assert-Check 'installer_exit' ($Process.ExitCode -eq 0)
    $Binary = Join-Path $App 'odin.exe'
    Assert-Check 'installed_binary_hash' ((Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash -ieq $ExpectedBinarySha256)
    $Metadata = (& $Binary metadata | ConvertFrom-Json)
    Assert-Check 'installed_metadata' ($LASTEXITCODE -eq 0 -and $Metadata.target -eq 'x86_64-pc-windows-msvc')
    if ($Portable) { Assert-Check 'installed_metadata_matches_portable' ((ConvertTo-OdinCanonical $Metadata | ConvertTo-Json -Depth 100 -Compress) -ceq (ConvertTo-OdinCanonical $PortableMetadata | ConvertTo-Json -Depth 100 -Compress)) }
    $Inventory = @(Verify-Payload $App)
    Assert-Check 'installed_inventory_sha256' ($Inventory.Count -gt 0)
    $InstalledTools = @(& $Binary tools | ConvertFrom-Json)
    Assert-Check 'installed_tools_exit' ($LASTEXITCODE -eq 0)
    $Catalog = @(Get-Content -LiteralPath (Join-Path $App 'schemas/tools.json') -Raw -Encoding UTF8 | ConvertFrom-Json)
    Assert-Check 'installed_tool_contracts' ((ConvertTo-OdinCanonical $InstalledTools | ConvertTo-Json -Depth 100 -Compress) -ceq (ConvertTo-OdinCanonical $Catalog | ConvertTo-Json -Depth 100 -Compress))
    $Runtime = Get-Content -LiteralPath (Join-Path $App 'windows-runtime-dependencies.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($Entry in $Runtime.app_local) {
        $RuntimePath = Join-Path $App $Entry.path
        Assert-Check ('installed_runtime_signature:'+$Entry.path) ((Get-AuthenticodeSignature -LiteralPath $RuntimePath).Status -eq 'Valid')
    }
    # Hold a real stdio server open to prove app-local DLL loading, without granting backends or writes.
    $RuntimeWorkspace = Join-Path $QaRoot 'runtime-workspace'
    New-Item -ItemType Directory -Path $RuntimeWorkspace | Out-Null
    $RuntimeConfig = Join-Path $QaRoot 'runtime-host.toml'
    [IO.File]::WriteAllText($RuntimeConfig, ("state_dir = '"+(Join-Path $QaRoot 'runtime-state')+"'`n[projects.probe]`nroot = '"+$RuntimeWorkspace+"'`n"), [Text.UTF8Encoding]::new($false))
    $RuntimeProcess = [Diagnostics.Process]::new()
    $RuntimeStarted = $false
    try {
        $RuntimeProcess.StartInfo = [Diagnostics.ProcessStartInfo]::new($Binary, ('--config "'+$RuntimeConfig+'" serve --transport stdio'))
        $RuntimeProcess.StartInfo.UseShellExecute = $false
        $RuntimeProcess.StartInfo.CreateNoWindow = $true
        $RuntimeProcess.StartInfo.RedirectStandardInput = $true
        $RuntimeProcess.StartInfo.RedirectStandardOutput = $true
        $RuntimeProcess.StartInfo.RedirectStandardError = $true
        $null = $RuntimeProcess.Start()
        $RuntimeStarted = $true
        $Timer = [Diagnostics.Stopwatch]::StartNew()
        $Loaded = @()
        do {
            $RuntimeProcess.Refresh()
            if ($RuntimeProcess.HasExited) { throw ('Installed stdio server exited before runtime inspection: '+$RuntimeProcess.StandardError.ReadToEnd()) }
            $Loaded = @($RuntimeProcess.Modules | ForEach-Object { $_.FileName })
            if (@($Runtime.app_local | Where-Object { $Loaded -notcontains (Join-Path $App $_.path) }).Count -eq 0) { break }
            Start-Sleep -Milliseconds 100
        } while ($Timer.Elapsed.TotalSeconds -lt 10)
        Assert-Check 'installed_runtime_loaded_app_local' (@($Runtime.app_local | Where-Object { $Loaded -notcontains (Join-Path $App $_.path) }).Count -eq 0)
    } finally {
        if ($RuntimeStarted) {
            $RuntimeProcess.StandardInput.Close()
            if (-not $RuntimeProcess.WaitForExit(5000)) { $RuntimeProcess.Kill(); $RuntimeProcess.WaitForExit() }
        }
        $RuntimeProcess.Dispose()
    }
    Assert-Check 'installed_safe_default_configuration' (-not (Test-Path -LiteralPath (Join-Path $App 'odin.local.toml')) -and -not (Test-Path -LiteralPath (Join-Path $App 'state')))
    $UserData = Join-Path $App 'operator-notes.txt'
    [IO.File]::WriteAllText($UserData, 'retain user data')
    # A second installation tests the same-version update path without touching personal configuration.
    $ReinstallArgs = @($InstallArgs | ForEach-Object { if ($_.StartsWith('/LOG=')) { '/LOG=' + (Join-Path $QaRoot 'reinstall.log') } else { $_ } })
    $Process = Start-Process -FilePath $Installer -ArgumentList ($ReinstallArgs | ForEach-Object { '"' + $_ + '"' }) -PassThru -Wait -WindowStyle Hidden
    $Commands.Add([ordered]@{name='reinstall'; exit_code=$Process.ExitCode; log='reinstall.log'})
    Assert-Check 'reinstall_exit' ($Process.ExitCode -eq 0)
    Assert-Check 'reinstall_preserves_user_file' ((Get-Content -LiteralPath $UserData -Raw) -eq 'retain user data')
    $ReinstalledInventory = @(Verify-Payload $App)
    Assert-Check 'reinstall_inventory_sha256' ($ReinstalledInventory.Count -eq $Inventory.Count)
    $Uninstaller = Join-Path $App 'unins000.exe'
    $Process = Start-Process -FilePath $Uninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="' + (Join-Path $QaRoot 'uninstall.log') + '"')) -PassThru -Wait -WindowStyle Hidden
    $Commands.Add([ordered]@{name='uninstall'; exit_code=$Process.ExitCode; log='uninstall.log'})
    Assert-Check 'uninstaller_exit' ($Process.ExitCode -eq 0)
    Assert-Check 'owned_binary_removed' (-not (Test-Path -LiteralPath $Binary))
    Assert-Check 'owned_inventory_payload_removed' (@($Inventory | Where-Object { Test-Path -LiteralPath (Join-Path $App $_.path) }).Count -eq 0)
    Assert-Check 'uninstall_preserves_user_file' ((Get-Content -LiteralPath $UserData -Raw) -eq 'retain user data')
    Assert-Check 'operator_path_unchanged' ([Environment]::GetEnvironmentVariable('Path', 'User') -ceq $UserPathBefore)
} catch { $Failure = $_.Exception.Message }
finally {
    if ($Failure) {
        $CleanupUninstaller = Join-Path $App 'unins000.exe'
        if (Test-Path -LiteralPath $CleanupUninstaller -PathType Leaf) {
            try {
                $Cleanup = Start-Process -FilePath $CleanupUninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -PassThru -Wait -WindowStyle Hidden
                $Commands.Add([ordered]@{name='failed_run_cleanup'; exit_code=$Cleanup.ExitCode})
            } catch { $Commands.Add([ordered]@{name='failed_run_cleanup'; error=$_.Exception.Message}) }
        }
    }
}
$Report = [ordered]@{scope='ISOLATED_WINDOWS_INSTALL_REINSTALL_UNINSTALL'; status=$(if($Failure){'FAIL'}else{'PASS'}); checks=$Checks; commands=$Commands; error=$Failure; installer_sha256=(Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant(); portable_sha256=$(if($Portable){(Get-FileHash -LiteralPath $Portable -Algorithm SHA256).Hash.ToLowerInvariant()}else{$null}); build=$Metadata; inventory_files=$Inventory.Count; qa_directory=$QaRoot; interruption_rollback='NOT_RUN'; cross_version_upgrade='NOT_RUN'; native_binary_expected_sha256=$ExpectedBinarySha256.ToLowerInvariant()}
[IO.File]::WriteAllText((Join-Path $QaRoot 'installer-report.json'), ($Report | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
$Report | ConvertTo-Json -Depth 12
if ($Failure) { throw $Failure }
