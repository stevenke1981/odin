$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
New-Item -ItemType Directory -Force verification | Out-Null
if (-not (Test-Path Cargo.lock)) { throw 'NOT_RUN: generate and review a real Cargo.lock with bootstrap.ps1 first.' }
$env:ODIN_PYTHON = (Get-Command python -ErrorAction Stop).Source
function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    Write-Host ('>>> ' + $Program + ' ' + ($Arguments -join ' '))
    $old = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Program @Arguments; $code = $LASTEXITCODE }
    finally { $ErrorActionPreference = $old }
    if ($code -ne 0) { throw "$Program failed with exit code $code" }
}
Start-Transcript -Path verification/local-verify.log -Force | Out-Null
try {
    Invoke-Checked rustc @('--version')
    Invoke-Checked cargo @('--version')
    Invoke-Checked python @('scripts/check_source.py','--output','verification/local-source-check.json')
    Invoke-Checked python @('scripts/fixture_selftest.py','--output','verification/local-fixture-selftest.json')
    Invoke-Checked cargo @('fmt','--all','--','--check')
    Invoke-Checked cargo @('test','--workspace','--locked')
    Invoke-Checked cargo @('clippy','--workspace','--all-targets','--locked','--','-D','warnings')
    Invoke-Checked cargo @('build','--workspace','--release','--locked')
    Invoke-Checked python @('scripts/mcp_smoke.py','--binary','target/release/odin.exe')
    Invoke-Checked python @('scripts/http_smoke.py','--binary','target/release/odin.exe')
    Write-Host 'Local gates passed. Real LSP, DAP and additional language gates remain separate.'
} finally { Stop-Transcript | Out-Null }
