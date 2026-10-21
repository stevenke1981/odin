# Packaging-only PE import inspection. No registry, PATH or global runtime installation.
function Get-OdinPeImports([string]$Path) {
    $Bytes = [IO.File]::ReadAllBytes($Path)
    if ($Bytes.Length -lt 256 -or [Text.Encoding]::ASCII.GetString($Bytes,0,2) -ne 'MZ') { throw 'Expected a PE binary.' }
    $Pe = [BitConverter]::ToUInt32($Bytes,60)
    if ($Pe + 24 -ge $Bytes.Length -or [BitConverter]::ToUInt32($Bytes,$Pe) -ne 0x4550 -or [BitConverter]::ToUInt16($Bytes,$Pe+4) -ne 0x8664) { throw 'Expected an x86-64 PE binary.' }
    $Count = [BitConverter]::ToUInt16($Bytes,$Pe+6)
    $OptionalSize = [BitConverter]::ToUInt16($Bytes,$Pe+20)
    $Optional = $Pe + 24
    if ([BitConverter]::ToUInt16($Bytes,$Optional) -ne 0x20b -or $OptionalSize -lt 128 -or $Optional + $OptionalSize + 40*$Count -gt $Bytes.Length) { throw 'Invalid PE64 headers.' }
    $Sections = @()
    for ($Index = 0; $Index -lt $Count; $Index++) {
        $Offset = $Optional + $OptionalSize + 40*$Index
        $Sections += [ordered]@{virtual_size=[BitConverter]::ToUInt32($Bytes,$Offset+8); rva=[BitConverter]::ToUInt32($Bytes,$Offset+12); raw_size=[BitConverter]::ToUInt32($Bytes,$Offset+16); raw=[BitConverter]::ToUInt32($Bytes,$Offset+20)}
    }
    function Convert-OdinRva([uint32]$Rva) {
        foreach ($Section in $Sections) {
            if ($Rva -ge $Section.rva -and $Rva -lt ([uint64]$Section.rva + [Math]::Max($Section.virtual_size,$Section.raw_size))) {
                $Position = [uint64]$Section.raw + $Rva - $Section.rva
                if ($Position -ge $Bytes.Length) { throw 'PE RVA is outside the file.' }
                return [int]$Position
            }
        }
        throw 'PE RVA has no matching section.'
    }
    $ImportRva = [BitConverter]::ToUInt32($Bytes,$Optional+120)
    if ($OptionalSize -ge 224 -and [BitConverter]::ToUInt32($Bytes,$Optional+216) -ne 0) { throw 'Delay-load imports require an explicit dependency review.' }
    if ($ImportRva -eq 0) { return @() }
    $Descriptor = Convert-OdinRva $ImportRva
    for ($Index = 0; $Index -lt 4096; $Index++) {
        if ($Descriptor + 20 -gt $Bytes.Length) { throw 'Truncated PE import descriptor.' }
        $NameRva = [BitConverter]::ToUInt32($Bytes,$Descriptor+12)
        if ($NameRva -eq 0) { return }
        $NameOffset = Convert-OdinRva $NameRva
        $Length = 0
        while ($NameOffset + $Length -lt $Bytes.Length -and $Bytes[$NameOffset+$Length] -ne 0 -and $Length -le 255) { $Length++ }
        if ($Length -gt 255 -or $NameOffset+$Length -ge $Bytes.Length) { throw 'Invalid PE DLL name.' }
        $Name = [Text.Encoding]::ASCII.GetString($Bytes,$NameOffset,$Length).ToLowerInvariant()
        if ($Name -notmatch '^[a-z0-9_.-]+\.dll$') { throw 'Invalid PE DLL name.' }
        $Name
        $Descriptor += 20
    }
    throw 'PE import descriptor budget exceeded.'
}

function Add-OdinWindowsRuntime([string]$Binary, [string]$Stage, [string]$Root, [string]$RuntimeDirectory = '') {
    $Queue = [Collections.Generic.Queue[string]]::new()
    $Queue.Enqueue($Binary)
    $Seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $Runtime = [Collections.Generic.List[object]]::new()
    $System = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $Builtin = @('kernel32.dll','ntdll.dll','ws2_32.dll','userenv.dll','bcryptprimitives.dll','bcrypt.dll','advapi32.dll','crypt32.dll','rpcrt4.dll')
    $RedistSource = 'https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution'
    while ($Queue.Count) {
        foreach ($Name in @(Get-OdinPeImports $Queue.Dequeue())) {
            if (-not $Seen.Add($Name)) { continue }
            if ($Name -match '^(api-ms-win-|ext-ms-win-)' -or $Builtin -contains $Name) { $null = $System.Add($Name); continue }
            if ($Name -notmatch '^(vcruntime140(?:_1)?|msvcp140(?:_\d)?|concrt140)\.dll$') { throw "Unclassified native runtime dependency: $Name" }
            if (-not $RuntimeDirectory) {
                $Vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
                if (-not (Test-Path -LiteralPath $Vswhere -PathType Leaf)) { throw 'Supply -RuntimeDirectory from the official Visual Studio VC/Redist x64 CRT directory.' }
                $Candidate = @(& $Vswhere -latest -products '*' -find 'VC\Redist\MSVC\*\x64\Microsoft.VC143.CRT\vcruntime140.dll') | Select-Object -Last 1
                if (-not $Candidate) { throw 'Visual Studio x64 redistributable CRT was not found.' }
                $RuntimeDirectory = Split-Path $Candidate -Parent
            }
            $Source = (Resolve-Path -LiteralPath (Join-Path $RuntimeDirectory $Name)).Path
            if ($Source -notmatch '[\\/]VC[\\/]Redist[\\/]MSVC[\\/][0-9.]+[\\/]x64[\\/]Microsoft\.VC143\.CRT[\\/]') { throw 'Runtime must come from the official Visual Studio redistributable directory.' }
            $Signature = Get-AuthenticodeSignature -LiteralPath $Source
            if ($Signature.Status -ne 'Valid' -or $Signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw "Microsoft runtime signature is not valid: $Name" }
            $null = @(Get-OdinPeImports $Source)
            Copy-Item -LiteralPath $Source -Destination (Join-Path $Stage $Name)
            $Runtime.Add([ordered]@{path=$Name; source=$Source; version=(Get-Item -LiteralPath $Source).VersionInfo.FileVersion; sha256=(Get-FileHash -LiteralPath $Source -Algorithm SHA256).Hash.ToLowerInvariant(); authenticode='Valid'; signer=$Signature.SignerCertificate.Subject; redistribution=$RedistSource})
            $Queue.Enqueue($Source)
        }
    }
    Copy-Item -LiteralPath (Join-Path $Root 'packaging/MICROSOFT_RUNTIME_NOTICES.txt') -Destination $Stage
    return [ordered]@{minimum_os='Windows 10 x64 or Windows 11 x64 with OS-provided UCRT'; app_local=@($Runtime); os_provided=@($System | Sort-Object); global_runtime_installation=$false}
}
