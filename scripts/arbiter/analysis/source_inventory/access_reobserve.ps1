# SPDX-License-Identifier: Apache-2.0
# Reobserve exact previously denied paths without changing ownership/access.
param(
 [Parameter(Mandatory=$true)][string]$ExpectedSid,
 [Parameter(Mandatory=$true)][string]$Manifest,
 [Parameter(Mandatory=$true)][string]$ManifestSha256
)
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
Assert-AirkHumanExecution -ExpectedSid $ExpectedSid | Out-Null
if((Get-FileHash -LiteralPath $Manifest).Hash -ne $ManifestSha256){throw 'Prior manifest changed'}
$prior=Get-Content -Raw -LiteralPath $Manifest | ConvertFrom-Json
if($prior.schema -ne 'airk.private.shallow-access-inventory/1'){throw 'Unexpected prior manifest'}
$root=[System.IO.Path]::GetFullPath((Join-Path $repo 'artifacts'))+[System.IO.Path]::DirectorySeparatorChar
$rows=@()
foreach($old in $prior.failures){
 $path=[System.IO.Path]::GetFullPath($old.path)
 if(-not $path.StartsWith($root,[System.StringComparison]::OrdinalIgnoreCase)){throw 'Path outside artifacts'}
 $row=[ordered]@{path=$path;prior_reason=$old.reason;readable=$false;owner=$null;sddl=$null;immediate_entries=$null;error=$null}
 try{
  $item=Get-Item -LiteralPath $path -Force
  if(($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0){throw 'Reparse path refused'}
  $acl=Get-Acl -LiteralPath $path
  $row.owner=$acl.Owner;$row.sddl=$acl.Sddl
  $children=@(Get-ChildItem -LiteralPath $path -Force -ErrorAction Stop)
  $row.immediate_entries=$children.Count;$row.readable=$true
 }catch{$row.error=$_.Exception.Message}
 $rows+=[pscustomobject]$row
}
if((Get-FileHash -LiteralPath $Manifest).Hash -ne $ManifestSha256){throw 'Prior manifest changed during reobservation'}
[ordered]@{schema='airk.private.access-reobservation/1';observed_utc=[DateTime]::UtcNow.ToString('o');prior_manifest=$Manifest;prior_sha256=$ManifestSha256;mutations_performed=$false;effective_sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value;default_owner_sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().Owner.Value;rows=$rows}|ConvertTo-Json -Depth 6
