# Run from a native Windows MSVC development environment.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
New-Item -ItemType Directory -Force verification | Out-Null
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'NOT_RUN: Rust/Cargo is not installed.' }
if (-not (Get-Command python -ErrorAction SilentlyContinue)) { throw 'NOT_RUN: Python 3.11+ is required.' }
function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    Write-Host ('>>> ' + $Program + ' ' + ($Arguments -join ' '))
    $old = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Program @Arguments; $code = $LASTEXITCODE }
    finally { $ErrorActionPreference = $old }
    if ($code -ne 0) { throw "$Program failed with exit code $code" }
}
Start-Transcript -Path verification/local-bootstrap.log -Force | Out-Null
try {
    Invoke-Checked rustc @('--version')
    Invoke-Checked cargo @('--version')
    if (-not (Test-Path Cargo.lock)) { Invoke-Checked cargo @('generate-lockfile') }
    Invoke-Checked cargo @('fmt', '--all')
    Invoke-Checked cargo @('build', '--workspace', '--locked')
    Write-Host 'Bootstrap finished. Review Cargo.lock, then run scripts/verify.ps1.'
} finally { Stop-Transcript | Out-Null }
