# Isolated per-user installer smoke, with app files directed to a new dist/installer-qa-* path.
param([Parameter(Mandatory)][string]$Installer, [Parameter(Mandatory)][string]$ExpectedBinarySha256)
$ErrorActionPreference = 'Stop'
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
$Failure = $null
function Assert-Check([string]$Name, [bool]$Passed) {
    $Checks.Add([ordered]@{name=$Name; status=$(if($Passed){'PASS'}else{'FAIL'})})
    if (-not $Passed) { throw "Installer check failed: $Name" }
}
try {
    # Disable uninstall registration, icons and interactive prompts for this isolated QA installation.
    $InstallArgs = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-','/NOICONS','/ODIN_QA=1',"/DIR=$App",('/LOG=' + (Join-Path $QaRoot 'install.log')))
    $Process = Start-Process -FilePath $Installer -ArgumentList ($InstallArgs | ForEach-Object { '"' + $_ + '"' }) -PassThru -Wait -WindowStyle Hidden
    Assert-Check 'installer_exit' ($Process.ExitCode -eq 0)
    $Binary = Join-Path $App 'odin.exe'
    Assert-Check 'installed_binary_hash' ((Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash -ieq $ExpectedBinarySha256)
    $Metadata = (& $Binary metadata | ConvertFrom-Json)
    Assert-Check 'installed_metadata' ($LASTEXITCODE -eq 0 -and $Metadata.target -eq 'x86_64-pc-windows-msvc')
    $UserData = Join-Path $App 'operator-notes.txt'
    [IO.File]::WriteAllText($UserData, 'retain user data')
    # A second installation tests the same-version update path without touching personal configuration.
    $Process = Start-Process -FilePath $Installer -ArgumentList ($InstallArgs | ForEach-Object { '"' + $_ + '"' }) -PassThru -Wait -WindowStyle Hidden
    Assert-Check 'reinstall_exit' ($Process.ExitCode -eq 0)
    Assert-Check 'reinstall_preserves_user_file' ((Get-Content -LiteralPath $UserData -Raw) -eq 'retain user data')
    $Uninstaller = Join-Path $App 'unins000.exe'
    $Process = Start-Process -FilePath $Uninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="' + (Join-Path $QaRoot 'uninstall.log') + '"')) -PassThru -Wait -WindowStyle Hidden
    Assert-Check 'uninstaller_exit' ($Process.ExitCode -eq 0)
    Assert-Check 'owned_binary_removed' (-not (Test-Path -LiteralPath $Binary))
    Assert-Check 'uninstall_preserves_user_file' ((Get-Content -LiteralPath $UserData -Raw) -eq 'retain user data')
} catch { $Failure = $_.Exception.Message }
$Report = [ordered]@{scope='ISOLATED_WINDOWS_INSTALL_REINSTALL_UNINSTALL'; status=$(if($Failure){'FAIL'}else{'PASS'}); checks=$Checks; error=$Failure; installer_sha256=(Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant(); qa_directory=$QaRoot; interruption_rollback='NOT_RUN'; cross_version_upgrade='NOT_RUN'; native_binary_expected_sha256=$ExpectedBinarySha256.ToLowerInvariant()}
[IO.File]::WriteAllText((Join-Path $QaRoot 'installer-report.json'), ($Report | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
$Report | ConvertTo-Json -Depth 12
if ($Failure) { throw $Failure }
