# Fixed read-only verifier. Parent supplies PATH from exact persisted Machine+User values.
param([Parameter(Mandatory)][string]$ExpectedBinaryPath, [Parameter(Mandatory)][string]$ExpectedBinarySha256)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
function Identity([string]$Value) {
    if ($Value.StartsWith('\\?\')) {
        if ($Value.Substring(4) -notmatch '^[A-Za-z]:\\') { throw 'Unsupported extended path namespace.' }
        $Value = $Value.Substring(4)
    }
    if ($Value.StartsWith('\\.\')) { throw 'Device path is unsupported.' }
    return [IO.Path]::GetFullPath($Value).TrimEnd('\').ToLowerInvariant()
}
$Selected = @(Get-Command -Name odin -CommandType Application -ErrorAction Stop)[0]
if ((Identity $Selected.Source) -cne (Identity $ExpectedBinaryPath)) { throw 'Get-Command odin selected a different application.' }
$Hash = (Get-FileHash -LiteralPath $Selected.Source -Algorithm SHA256).Hash.ToLowerInvariant()
if ($Hash -cne $ExpectedBinarySha256) { throw 'Resolved Odin executable SHA-256 mismatch.' }
$Raw = & $Selected.Source metadata
$Code = $LASTEXITCODE
if ($Code -ne 0) { throw 'Pinned Odin metadata failed.' }
$Metadata = $Raw | ConvertFrom-Json
if ($Metadata.name -cne 'Odin' -or $Metadata.target -cne 'x86_64-pc-windows-msvc') { throw 'Unexpected Odin build identity.' }
if ((Get-FileHash -LiteralPath $Selected.Source -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Hash) { throw 'Odin executable changed during metadata observation.' }
$Hasher = [Security.Cryptography.SHA256]::Create()
try { $PathDigest = ([BitConverter]::ToString($Hasher.ComputeHash([Text.Encoding]::Unicode.GetBytes($env:Path)))).Replace('-','').ToLowerInvariant() }
finally { $Hasher.Dispose() }
[ordered]@{status='PASS'; selected_application=$Selected.Source; binary_sha256=$Hash; metadata_exit_code=$Code; metadata=$Metadata; effective_path_sha256=$PathDigest; scope='FRESH_FIXED_READONLY_CLI_PROCESS'; existing_process_path_updated=$false; client_restart='NOT_RUN'} | ConvertTo-Json -Depth 8 -Compress
