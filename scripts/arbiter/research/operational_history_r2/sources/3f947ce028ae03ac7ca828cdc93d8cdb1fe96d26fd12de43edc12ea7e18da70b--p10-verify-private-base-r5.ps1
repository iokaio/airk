# SPDX-License-Identifier: Apache-2.0
# Read-only environment proof; never creates entries or changes ACLs.
#Requires -Version 7.0
param([Parameter(Mandatory=$true)][string]$ExpectedReceiptSha256)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows qualification adapter only' }
$airkReceiptPath = 'D:\code\Github\airk\artifacts\v190\p10-private-base-r5.json'
if ((Get-FileHash -LiteralPath $airkReceiptPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ExpectedReceiptSha256) { throw 'Private-base receipt drift' }
$airkReceipt = Get-Content -LiteralPath $airkReceiptPath -Raw | ConvertFrom-Json
$airkBase = [IO.Path]::GetFullPath($airkReceipt.path)
if ([IO.Path]::GetDirectoryName($airkBase) -ne 'D:\code\Github\airk\artifacts\v190' -or [IO.Path]::GetFileName($airkBase) -notmatch '^p10-private-fixtures-[0-9a-f]{32}$') { throw 'Private base escaped its exact workspace parent' }
foreach ($airkChecked in @('D:\code\Github\airk', 'D:\code\Github\airk\artifacts', $PSScriptRoot, $airkBase)) {
    $airkItem = Get-Item -LiteralPath $airkChecked -Force
    if (-not $airkItem.PSIsContainer -or ($airkItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Private base must remain a real directory under its original parents' }
}
$airkIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()
$airkOwnerSid = $airkIdentity.User.Value
$airkAcl = Get-Acl -LiteralPath $airkBase
$airkRules = @($airkAcl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
if ($airkReceipt.schema -ne 'airk.p10.private-fixture-base/1' -or $airkReceipt.owner_sid -ne $airkOwnerSid -or $airkAcl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $airkOwnerSid -or -not $airkAcl.AreAccessRulesProtected -or $airkRules.Count -ne 1 -or $airkAcl.Sddl -ne $airkReceipt.sddl) { throw 'Private-root owner or protected DACL differs from creation proof' }
$airkRule = $airkRules[0]
if ($airkRule.IdentityReference.Value -ne $airkOwnerSid -or $airkRule.IsInherited -or $airkRule.FileSystemRights -ne [Security.AccessControl.FileSystemRights]::FullControl -or $airkRule.InheritanceFlags -ne [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit' -or $airkRule.PropagationFlags -ne [Security.AccessControl.PropagationFlags]::None -or $airkRule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow) { throw 'Private-root single ACE contract changed' }
[pscustomobject]@{path=$airkBase; owner_sid=$airkOwnerSid; sddl=$airkAcl.Sddl; receipt_sha256=$ExpectedReceiptSha256; verified_utc=[DateTimeOffset]::UtcNow.ToString('o'); acl_mutated=$false; contents_not_required_empty=$true} | ConvertTo-Json -Compress
