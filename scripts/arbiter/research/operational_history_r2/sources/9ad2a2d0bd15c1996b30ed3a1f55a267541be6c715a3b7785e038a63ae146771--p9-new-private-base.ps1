# SPDX-License-Identifier: Apache-2.0
# Create a new fixture root with its owner-only inheritable DACL at creation.
# Never repair permissions on an existing path or change the repository ACL.
#Requires -Version 7.0
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows fixture adapter only' }
$airkIdentity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$airkOwnerSid = $airkIdentity.User
$airkParent = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$airkBase = [IO.Path]::GetFullPath((Join-Path $airkParent ('p9-private-fixtures-' + [guid]::NewGuid().ToString('N'))))
if ([IO.Directory]::Exists($airkBase) -or [IO.File]::Exists($airkBase)) { throw 'Fresh fixture path unexpectedly exists' }
if ([IO.Path]::GetDirectoryName($airkBase) -ne $airkParent) { throw 'Fixture base escaped its intended parent' }
$airkSecurity = [System.Security.AccessControl.DirectorySecurity]::new()
$airkSecurity.SetOwner($airkOwnerSid)
$airkSecurity.SetAccessRuleProtection($true, $false)
$airkRule = [System.Security.AccessControl.FileSystemAccessRule]::new(
    $airkOwnerSid,
    [System.Security.AccessControl.FileSystemRights]::FullControl,
    [System.Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',
    [System.Security.AccessControl.PropagationFlags]::None,
    [System.Security.AccessControl.AccessControlType]::Allow
)
$airkSecurity.AddAccessRule($airkRule)
[System.IO.FileSystemAclExtensions]::CreateDirectory($airkSecurity, $airkBase) | Out-Null
$airkObserved = Get-Acl -LiteralPath $airkBase
$airkObservedOwner = $airkObserved.GetOwner([System.Security.Principal.SecurityIdentifier]).Value
$airkEntries = @($airkObserved.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))
if ($airkObservedOwner -ne $airkOwnerSid.Value -or -not $airkObserved.AreAccessRulesProtected -or $airkEntries.Count -ne 1) {
    throw 'Fresh fixture native boundary does not match the intended owner/DACL'
}
$airkEntry = $airkEntries[0]
if ($airkEntry.IdentityReference.Value -ne $airkOwnerSid.Value -or
    $airkEntry.FileSystemRights -ne [System.Security.AccessControl.FileSystemRights]::FullControl -or
    $airkEntry.InheritanceFlags -ne [System.Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit' -or
    $airkEntry.PropagationFlags -ne [System.Security.AccessControl.PropagationFlags]::None -or
    $airkEntry.AccessControlType -ne [System.Security.AccessControl.AccessControlType]::Allow -or
    $airkEntry.IsInherited) {
    throw 'Fresh fixture DACL differs from its creation contract'
}
[pscustomobject]@{
    schema = 'airk.p9.private-fixture-base/1'
    path = $airkBase
    owner_sid = $airkOwnerSid.Value
    owner_name = $airkIdentity.Name
    sddl = $airkObserved.Sddl
    created_utc = [DateTimeOffset]::UtcNow.ToString('o')
    existing_acl_changed = $false
} | ConvertTo-Json
