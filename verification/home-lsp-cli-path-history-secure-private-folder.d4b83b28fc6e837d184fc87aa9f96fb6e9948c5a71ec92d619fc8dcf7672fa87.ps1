param([Parameter(Mandatory)][string]$Directory)
$ErrorActionPreference = 'Stop'
foreach ($Module in @('Microsoft.PowerShell.Management','Microsoft.PowerShell.Security')) {
    Import-Module (Join-Path $PSHOME ('Modules/' + $Module + '/' + $Module + '.psd1')) -ErrorAction Stop
}
$Item = Get-Item -LiteralPath $Directory -Force
if (-not $Item.PSIsContainer -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Private directory must be a physical existing directory.' }
$Sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$System = [Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$Acl = [Security.AccessControl.DirectorySecurity]::new()
$Acl.SetAccessRuleProtection($true, $false)
$Acl.SetOwner($Sid)
foreach ($Identity in @($Sid, $System)) {
    $Rule = [Security.AccessControl.FileSystemAccessRule]::new($Identity, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
    [void]$Acl.AddAccessRule($Rule)
}
Set-Acl -LiteralPath $Directory -AclObject $Acl
$Actual = Get-Acl -LiteralPath $Directory
if (-not $Actual.AreAccessRulesProtected) { throw 'Private directory ACL inheritance is not protected.' }
foreach ($Rule in $Actual.Access) {
    $RuleSid = $Rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
    if ($RuleSid -notin @($Sid.Value, $System.Value)) { throw 'Private directory includes an unexpected ACL principal.' }
}
'PASS: private backup directory limited to current user and SYSTEM'
