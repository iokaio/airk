# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$airkWorkspace = [IO.Path]::GetFullPath('D:/code/Github/airk')
$airkFreshBase = [IO.Path]::GetFullPath('D:/code/Github/airk/artifacts/r17')
if ($airkFreshBase -ne (Join-Path $airkWorkspace 'artifacts/r17')) { throw 'Unexpected fixture target' }
if (Test-Path -LiteralPath $airkFreshBase) { throw 'Fresh upgrade base already exists; preserve it' }
$airkCurrentOwner = [Security.Principal.WindowsIdentity]::GetCurrent().User
$airkReviewedAcl = Get-Acl -LiteralPath 'D:/code/Github/airk/artifacts/readiness-1817/host-fixtures'
if ($airkReviewedAcl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $airkCurrentOwner.Value) { throw 'Host fixture owner differs' }
New-Item -ItemType Directory -Path $airkFreshBase | Out-Null
Set-Acl -LiteralPath $airkFreshBase -AclObject $airkReviewedAcl
$airkObservedAcl = Get-Acl -LiteralPath $airkFreshBase
$airkReceipt = [ordered]@{
    path = $airkFreshBase
    owner = $airkObservedAcl.GetOwner([Security.Principal.SecurityIdentifier]).Value
    sddl = $airkObservedAcl.Sddl
    acl_matches_reviewed_host_base = $airkObservedAcl.Sddl -eq $airkReviewedAcl.Sddl
    existing_directories_changed = $false
    previous_q31_untouched = $true
}
if (-not $airkReceipt.acl_matches_reviewed_host_base) { throw 'Fresh ACL differs from reviewed host fixture base' }
$airkReceipt | ConvertTo-Json | Set-Content -LiteralPath 'D:/code/Github/airk/artifacts/readiness-1817/upgrade-base.json' -Encoding utf8
$airkReceipt | ConvertTo-Json
