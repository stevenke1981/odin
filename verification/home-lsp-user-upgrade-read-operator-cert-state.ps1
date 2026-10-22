# Read-only counts and identity digests. OpenExistingOnly avoids creating stores.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$Result = [ordered]@{}
foreach ($Location in @('CurrentUser','LocalMachine')) {
    foreach ($Name in [Enum]::GetNames([Security.Cryptography.X509Certificates.StoreName])) {
        $Store = [Security.Cryptography.X509Certificates.X509Store]::new($Name, $Location)
        try {
            $Store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly -bor [Security.Cryptography.X509Certificates.OpenFlags]::OpenExistingOnly)
            $Thumbprints = @($Store.Certificates | ForEach-Object { $_.Thumbprint } | Sort-Object -CaseSensitive)
            $Bytes = [Text.Encoding]::UTF8.GetBytes(($Thumbprints -join "`n"))
            $Hasher = [Security.Cryptography.SHA256]::Create()
            try { $Digest = ([BitConverter]::ToString($Hasher.ComputeHash($Bytes))).Replace('-','').ToLowerInvariant() }
            finally { $Hasher.Dispose() }
            $Result[$Location + '/' + $Name] = [ordered]@{status='OBSERVED'; count=$Thumbprints.Count; identities_sha256=$Digest}
        } catch [Security.Cryptography.CryptographicException] {
            if ($_.Exception.HResult -in @(-2146885628, -2147024894)) {
                $Result[$Location + '/' + $Name] = [ordered]@{status='ABSENT'; count=0; hresult=$_.Exception.HResult}
            } else { throw }
        } finally { $Store.Close() }
    }
}
$Result | ConvertTo-Json -Depth 5 -Compress
