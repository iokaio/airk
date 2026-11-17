# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory=$true)][string]$Anchor)
$ErrorActionPreference = 'Stop'
# Dedicated proof for this frozen private FINAL output anchor; no filesystem writes.
$item = Get-Item -LiteralPath $Anchor -Force
if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'anchor_shape' }
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$acl = Get-Acl -LiteralPath $item.FullName
$owner = $acl.GetOwner([Security.Principal.SecurityIdentifier])
if ($owner.Value -ne $sid.Value -or -not $acl.AreAccessRulesProtected) { throw 'anchor_owner_or_inheritance' }
$rules = @($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
$full = $false
foreach ($rule in $rules) {
    if ($rule.IdentityReference.Value -ne $sid.Value -or $rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow) { throw 'anchor_foreign_or_deny_rule' }
    if (($rule.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq [Security.AccessControl.FileSystemRights]::FullControl -and
        ($rule.InheritanceFlags -band [Security.AccessControl.InheritanceFlags]::ContainerInherit) -and
        ($rule.InheritanceFlags -band [Security.AccessControl.InheritanceFlags]::ObjectInherit) -and
        $rule.PropagationFlags -eq [Security.AccessControl.PropagationFlags]::None) { $full = $true }
}
if (-not $full) { throw 'anchor_inheritable_full_control_missing' }
[ordered]@{ schema='airk.private-qasper-final-anchor-observation/1'; canonical_path=$item.FullName; current_sid=$sid.Value; owner_sid=$owner.Value; protected=$acl.AreAccessRulesProtected; rules=$rules | ForEach-Object { [ordered]@{sid=$_.IdentityReference.Value;rights=[string]$_.FileSystemRights;inheritance=[string]$_.InheritanceFlags;propagation=[string]$_.PropagationFlags;inherited=$_.IsInherited;type=[string]$_.AccessControlType} }; no_reparse=$true; current_owner_only=$true; sddl=$acl.Sddl } | ConvertTo-Json -Depth 6 -Compress
