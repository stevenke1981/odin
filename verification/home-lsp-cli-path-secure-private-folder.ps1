param([Parameter(Mandatory)][string]$Directory)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Management/Microsoft.PowerShell.Management.psd1') -ErrorAction Stop
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$TaskRoot = [IO.Path]::GetFullPath($PSScriptRoot).TrimEnd('\')
$Target = [IO.Path]::GetFullPath($Directory)
if (-not $Target.StartsWith($TaskRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'ACL target must remain inside this owned external task directory.' }
$Item = Get-Item -LiteralPath $Directory -Force
if (-not $Item.PSIsContainer -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Private directory must be a physical existing directory.' }
$Sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$System = [Security.Principal.SecurityIdentifier]::new('S-1-5-18')
function Test-StrictPrivateAcl($Value) {
    if (-not $Value.AreAccessRulesProtected -or $Value.GetOwner([Security.Principal.SecurityIdentifier]).Value -cne $Sid.Value) { return $false }
    $Rules = @($Value.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
    if ($Rules.Count -ne 2) { return $false }
    $Seen = @{}
    foreach ($Rule in $Rules) {
        $Identity = $Rule.IdentityReference.Value
        if ($Identity -notin @($Sid.Value, $System.Value) -or $Seen.ContainsKey($Identity) -or $Rule.IsInherited -or
            $Rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
            $Rule.FileSystemRights -ne [Security.AccessControl.FileSystemRights]::FullControl -or
            $Rule.InheritanceFlags -ne ([Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit) -or
            $Rule.PropagationFlags -ne [Security.AccessControl.PropagationFlags]::None) { return $false }
        $Seen[$Identity] = $true
    }
    return $Seen.ContainsKey($Sid.Value) -and $Seen.ContainsKey($System.Value)
}
$Sections = [Security.AccessControl.AccessControlSections]::Access -bor [Security.AccessControl.AccessControlSections]::Owner
$Actual = $Item.GetAccessControl($Sections)
if ($Actual.GetOwner([Security.Principal.SecurityIdentifier]).Value -cne $Sid.Value) { throw 'Private directory owner is not the current user; no ownership override is performed.' }
$Action = 'VALIDATED_EXISTING_NO_WRITE'
if (-not (Test-StrictPrivateAcl $Actual)) {
    # Persist only the modified Access section. No SACL/audit or owner mutation,
    # and no request for SeSecurityPrivilege through the Set-Acl cmdlet.
    $Acl = [Security.AccessControl.DirectorySecurity]::new()
    $Acl.SetAccessRuleProtection($true, $false)
    foreach ($Identity in @($Sid, $System)) {
        $Rule = [Security.AccessControl.FileSystemAccessRule]::new($Identity, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        [void]$Acl.AddAccessRule($Rule)
    }
    $Item.SetAccessControl($Acl)
    $Action = 'ENFORCED_DACL_ONLY_OWNER_UNCHANGED'
    $Actual = $Item.GetAccessControl($Sections)
}
if (-not (Test-StrictPrivateAcl $Actual)) { throw 'Private directory strict owner/ACL validation failed.' }
[ordered]@{status='PASS'; action=$Action; owner_current_user=$true; protected=$true; explicit_full_control_principal_count=2; permitted_principals='CURRENT_USER_AND_SYSTEM_ONLY'; sacl_mutation=$false; owner_mutation=$false} | ConvertTo-Json -Compress
