# SPDX-License-Identifier: Apache-2.0
# Read-only shallow access inventory. JSON is stdout; callers guard retained outputs.
param(
    [Parameter(Mandatory=$true)][string]$ExpectedSid,
    [ValidateRange(0,8)][int]$Depth=2
)
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
Assert-AirkHumanExecution -ExpectedSid $ExpectedSid | Out-Null
$root=Join-Path $repo 'artifacts'
$errors=@()
$directories=@(Get-ChildItem -LiteralPath $root -Directory -Recurse -Depth $Depth -ErrorAction SilentlyContinue -ErrorVariable +errors)
$rows=@()
foreach($failure in $errors){
    $path=[string]$failure.TargetObject
    $row=[ordered]@{path=$path;reason=$failure.Exception.Message;owner=$null;sddl=$null;acl_error=$null;parent_owner=$null;parent_sddl=$null;parent_acl_error=$null;contents_known=$false}
    try{$acl=Get-Acl -LiteralPath $path; $row.owner=$acl.Owner; $row.sddl=$acl.Sddl}catch{$row.acl_error=$_.Exception.Message}
    try{$acl=Get-Acl -LiteralPath ([System.IO.Directory]::GetParent($path).FullName);$row.parent_owner=$acl.Owner;$row.parent_sddl=$acl.Sddl}catch{$row.parent_acl_error=$_.Exception.Message}
    $rows+=[pscustomobject]$row
}
[ordered]@{
    schema='airk.private.shallow-access-inventory/1'
    observed_utc=[DateTime]::UtcNow.ToString('o')
    root=$root;depth=$Depth;directories_observed=$directories.Count
    effective_sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    default_owner_sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().Owner.Value
    failures=$rows
    complete_recursive_coverage=$false
    mutations_performed=$false
}|ConvertTo-Json -Depth 6
