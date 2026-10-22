# Isolated per-user installer smoke, with app files directed to a new dist/installer-qa-* path.
param([Parameter(Mandatory)][string]$Installer, [Parameter(Mandatory)][string]$ExpectedBinarySha256, [string]$Portable = '')
$ErrorActionPreference = 'Stop'
foreach ($ModuleName in @('Microsoft.PowerShell.Utility','Microsoft.PowerShell.Security','Microsoft.PowerShell.Archive')) {
    Import-Module (Join-Path $PSHOME "Modules/$ModuleName/$ModuleName.psd1") -ErrorAction Stop
}
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
$PathObservations = [Collections.Generic.List[object]]::new()
function Record-OperatorPath([string]$Phase) {
    $Key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    try { $Raw = $(if ($Key) { $Key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } else { $null }) }
    finally { if ($Key) { $Key.Dispose() } }
    $PathObservations.Add([ordered]@{phase=$Phase; observed_at_utc=[DateTime]::UtcNow.ToString('o'); user_path=[Environment]::GetEnvironmentVariable('Path','User'); user_path_raw=$Raw; process_path=$env:Path})
    [IO.File]::WriteAllText((Join-Path $QaRoot 'path-observations.json'), (ConvertTo-Json -InputObject @($PathObservations.ToArray()) -Depth 4), [Text.UTF8Encoding]::new($false))
}
Record-OperatorPath 'before'
$UserPathBefore = $PathObservations[0].user_path
$UserPathRawBefore = $PathObservations[0].user_path_raw
$Metadata = $null
$LspWorkspace = Join-Path $QaRoot 'lsp-workspace'
$LspHost = Join-Path $QaRoot 'host/host.toml'
$LspReceipt = Join-Path $QaRoot 'lsp-install-receipt.json'
$LspReinstallReceipt = Join-Path $QaRoot 'lsp-reinstall-receipt.json'
New-Item -ItemType Directory -Path $LspWorkspace -Force | Out-Null
function Assert-Check([string]$Name, [bool]$Passed) {
    $Checks.Add([ordered]@{name=$Name; status=$(if($Passed){'PASS'}else{'FAIL'})})
    if (-not $Passed) { throw "Installer check failed: $Name" }
}
function Verify-LspReceipt([string]$ReceiptPath) {
    $Receipt = Get-Content -LiteralPath $ReceiptPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-Check ('lsp_setup_result:' + [IO.Path]::GetFileName($ReceiptPath)) ($Receipt.result.status -eq 'PASS')
    Assert-Check ('lsp_health:' + [IO.Path]::GetFileName($ReceiptPath)) ($Receipt.health.status -eq 'PASS' -and $Receipt.health.post_health_integrity.status -eq 'PASS' -and $Receipt.health.checks.Count -eq 10 -and @($Receipt.health.checks | Where-Object { $_.status -ne 'PASS' -or $_.exit_code -ne 0 }).Count -eq 0)
    Assert-Check ('lsp_languages:' + [IO.Path]::GetFileName($ReceiptPath)) (@('python','go','csharp','lua','javascript','typescript','bash' | Where-Object { $Receipt.result.languages -notcontains $_ }).Count -eq 0)
    return $Receipt
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
    $Rows = Get-Content -LiteralPath $Manifest -Raw -Encoding UTF8 | ConvertFrom-Json
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
        $PortableHost = Join-Path $QaRoot 'portable-host/host.toml'
        $PortableReceiptPath = Join-Path $QaRoot 'portable-lsp-receipt.json'
        $PortableSetupRaw = & (Join-Path $Extracted 'backends/node/node.exe') (Join-Path $Extracted 'setup-lsps.mjs') --install --host-config $PortableHost --workspace-root $LspWorkspace --receipt $PortableReceiptPath
        $Commands.Add([ordered]@{name='portable_lsp_setup'; exit_code=$LASTEXITCODE; receipt=$PortableReceiptPath})
        Assert-Check 'portable_lsp_setup_exit' ($LASTEXITCODE -eq 0)
        $PortableLspReceipt = Verify-LspReceipt $PortableReceiptPath
        $PortableValidate = & (Join-Path $Extracted 'odin.exe') --config $PortableHost --workspace $LspWorkspace validate
        $Commands.Add([ordered]@{name='portable_host_validate'; exit_code=$LASTEXITCODE})
        Assert-Check 'portable_generated_host_validates' ($LASTEXITCODE -eq 0)
    }
    Record-OperatorPath 'after_portable'
    # Disable uninstall registration, icons and interactive prompts for this isolated QA installation.
    $InstallArgs = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-','/NOICONS','/ODIN_QA=1',"/DIR=$App","/ODIN_HOST_CONFIG=$LspHost","/ODIN_WORKSPACE_ROOT=$LspWorkspace","/ODIN_SETUP_RECEIPT=$LspReceipt",('/LOG=' + (Join-Path $QaRoot 'install.log')))
    $Process = Start-Process -FilePath $Installer -ArgumentList ($InstallArgs | ForEach-Object { '"' + $_ + '"' }) -PassThru -Wait -WindowStyle Hidden
    $Commands.Add([ordered]@{name='install'; exit_code=$Process.ExitCode; log='install.log'})
    Record-OperatorPath 'after_install'
    Assert-Check 'installer_exit' ($Process.ExitCode -eq 0)
    $Binary = Join-Path $App 'odin.exe'
    Assert-Check 'installed_binary_hash' ((Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash -ieq $ExpectedBinarySha256)
    $Metadata = (& $Binary metadata | ConvertFrom-Json)
    Assert-Check 'installed_metadata' ($LASTEXITCODE -eq 0 -and $Metadata.target -eq 'x86_64-pc-windows-msvc')
    if ($Portable) { Assert-Check 'installed_metadata_matches_portable' ((ConvertTo-OdinCanonical $Metadata | ConvertTo-Json -Depth 100 -Compress) -ceq (ConvertTo-OdinCanonical $PortableMetadata | ConvertTo-Json -Depth 100 -Compress)) }
    $Inventory = @(Verify-Payload $App)
    Assert-Check 'installed_inventory_sha256' ($Inventory.Count -gt 0)
    $InstalledLspReceipt = Verify-LspReceipt $LspReceipt
    $InstalledValidate = & $Binary --config $LspHost --workspace $LspWorkspace validate
    $Commands.Add([ordered]@{name='installed_host_validate'; exit_code=$LASTEXITCODE})
    Assert-Check 'installed_generated_host_validates' ($LASTEXITCODE -eq 0)
    $GeneratedHost = Get-Content -LiteralPath $LspHost -Raw -Encoding UTF8
    Assert-Check 'installed_fresh_host_has_no_elevated_grants' ($GeneratedHost -notmatch '(?m)^allow_(write|shell|debug|inspections|formatters)\s*=\s*true\s*$')
    $HostHashBeforeReinstall = (Get-FileHash -LiteralPath $LspHost -Algorithm SHA256).Hash
    if ($Portable) { Assert-Check 'portable_installed_bundle_manifest_matches' ($PortableLspReceipt.result.bundle_manifest_sha256 -eq $InstalledLspReceipt.result.bundle_manifest_sha256) }
    $Example = Get-Content -LiteralPath (Join-Path $App 'odin.example.toml') -Raw -Encoding UTF8
    Assert-Check 'shipped_template_permissions_disabled' (@('write','shell','debug','inspections','formatters' | Where-Object { $Example -notmatch ("(?m)^allow_"+$_+"\s*=\s*false\s*$") }).Count -eq 0)
    $InstalledTools = & $Binary tools | ConvertFrom-Json
    Assert-Check 'installed_tools_exit' ($LASTEXITCODE -eq 0)
    $Catalog = Get-Content -LiteralPath (Join-Path $App 'schemas/tools.json') -Raw -Encoding UTF8 | ConvertFrom-Json
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
    $ReinstallArgs = @($InstallArgs | ForEach-Object { if ($_.StartsWith('/LOG=')) { '/LOG=' + (Join-Path $QaRoot 'reinstall.log') } elseif ($_.StartsWith('/ODIN_SETUP_RECEIPT=')) { '/ODIN_SETUP_RECEIPT=' + $LspReinstallReceipt } else { $_ } })
    $Process = Start-Process -FilePath $Installer -ArgumentList ($ReinstallArgs | ForEach-Object { '"' + $_ + '"' }) -PassThru -Wait -WindowStyle Hidden
    $Commands.Add([ordered]@{name='reinstall'; exit_code=$Process.ExitCode; log='reinstall.log'})
    Record-OperatorPath 'after_reinstall'
    Assert-Check 'reinstall_exit' ($Process.ExitCode -eq 0)
    $ReinstallLspReceipt = Verify-LspReceipt $LspReinstallReceipt
    Assert-Check 'reinstall_preserves_host_content' ((Get-FileHash -LiteralPath $LspHost -Algorithm SHA256).Hash -ceq $HostHashBeforeReinstall)
    Assert-Check 'reinstall_preserves_user_file' ((Get-Content -LiteralPath $UserData -Raw) -eq 'retain user data')
    $ReinstalledInventory = @(Verify-Payload $App)
    Assert-Check 'reinstall_inventory_sha256' ($ReinstalledInventory.Count -eq $Inventory.Count)
    $Uninstaller = Join-Path $App 'unins000.exe'
    $Process = Start-Process -FilePath $Uninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="' + (Join-Path $QaRoot 'uninstall.log') + '"')) -PassThru -Wait -WindowStyle Hidden
    $Commands.Add([ordered]@{name='uninstall'; exit_code=$Process.ExitCode; log='uninstall.log'})
    Record-OperatorPath 'after_uninstall'
    Assert-Check 'uninstaller_exit' ($Process.ExitCode -eq 0)
    Assert-Check 'owned_binary_removed' (-not (Test-Path -LiteralPath $Binary))
    Assert-Check 'owned_inventory_payload_removed' (@($Inventory | Where-Object { Test-Path -LiteralPath (Join-Path $App $_.path) }).Count -eq 0)
    Assert-Check 'uninstall_preserves_user_file' ((Get-Content -LiteralPath $UserData -Raw) -eq 'retain user data')
    Assert-Check 'uninstall_preserves_external_host' ((Get-FileHash -LiteralPath $LspHost -Algorithm SHA256).Hash -ceq $HostHashBeforeReinstall)
    Assert-Check 'operator_path_unchanged' ($PathObservations[$PathObservations.Count-1].user_path -ceq $UserPathBefore -and $PathObservations[$PathObservations.Count-1].user_path_raw -ceq $UserPathRawBefore)
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
$Report = [ordered]@{scope='ISOLATED_WINDOWS_INSTALL_REINSTALL_UNINSTALL'; status=$(if($Failure){'FAIL'}else{'PASS'}); checks=$Checks; commands=$Commands; operator_path_observations=$PathObservations; error=$Failure; installer_sha256=(Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant(); portable_sha256=$(if($Portable){(Get-FileHash -LiteralPath $Portable -Algorithm SHA256).Hash.ToLowerInvariant()}else{$null}); build=$Metadata; inventory_files=$Inventory.Count; qa_directory=$QaRoot; interruption_rollback='NOT_RUN'; cross_version_upgrade='NOT_RUN'; lsp_semantic_acceptance='NOT_RUN'; lsp_version_health='See source-bound install receipts'; native_binary_expected_sha256=$ExpectedBinarySha256.ToLowerInvariant()}
[IO.File]::WriteAllText((Join-Path $QaRoot 'installer-report.json'), ($Report | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
$Report | ConvertTo-Json -Depth 12
if ($Failure) { throw $Failure }
