# SPDX-License-Identifier: Apache-2.0
[CmdletBinding()]
param(
 [Parameter(Mandatory)][string]$Manifest,
 [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$ManifestSha256,
 [Parameter(Mandatory)][long]$ManifestBytes,
 [Parameter(Mandatory)][string]$ReceiptDirectory
)
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
$identity=Assert-AirkHumanExecution $sid
Set-Location -LiteralPath $root
# Python receives fixed program/argument slots; no manifest text becomes shell code.
$verified=& python -B -S (Join-Path $PSScriptRoot 'cleanup.py') verify --input $Manifest --sha256 $ManifestSha256 --bytes $ManifestBytes
if($LASTEXITCODE -ne 0){throw 'Exact committed cleanup preflight refused'}
$plan=($verified -join "`n")|ConvertFrom-Json -AsHashtable
if($plan.deferred_count -ne 0 -or $plan.ready_count -lt 1){throw 'Nonempty all-ready plan required'}
$receiptRoot=Get-AirkCheckedPath -Path ([IO.Path]::GetFullPath($ReceiptDirectory)) -Root (Join-Path $root 'artifacts')
Assert-AirkOutputPathObservation -Path $receiptRoot -Root $root -Identity $identity|Out-Null
if(Test-Path -LiteralPath $receiptRoot){throw 'Fresh receipt directory required'}
New-Item -ItemType Directory -Path $receiptRoot|Out-Null
Assert-AirkOutputPathObservation -Path $receiptRoot -Root $root -Identity $identity|Out-Null
if(!(Test-Path -LiteralPath $receiptRoot -PathType Container)){throw 'Created receipt directory absent'}
function Write-ImmutableReceipt([string]$Name,$Value){
 $path=Join-Path $receiptRoot $Name
 Assert-AirkOutputPathObservation -Path $path -Root $root -Identity $identity|Out-Null
 $raw=[Text.UTF8Encoding]::new($false).GetBytes(($Value|ConvertTo-Json -Depth 70)+"`n")
 $stream=[IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$stream.Write($raw);$stream.Flush($true)}finally{$stream.Dispose()}
 Assert-AirkOutputPathObservation -Path $path -Root $root -Identity $identity|Out-Null
 if(!(Test-Path -LiteralPath $path -PathType Leaf)){throw 'Created receipt absent'}
 return @{path=[IO.Path]::GetRelativePath($root,$path).Replace('\','/');sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($raw)).ToLowerInvariant();bytes=$raw.Length}
}
$preflight=Write-ImmutableReceipt 'pre-removal.json' @{schema='airk.source-cleanup-pre-removal/1';manifest=@{path=$Manifest;sha256=$ManifestSha256;bytes=$ManifestBytes};plan=$plan;observed_utc=[DateTime]::UtcNow.ToString('o');human_sid=$sid}
$results=@();$ordinal=0
foreach($row in $plan.files){
 $ordinal++;$intent=$null;$removed=$false;$failure=$null
 try{
  Assert-AirkCurrentToken $identity
  $currentRaw=& (Join-Path $PSScriptRoot 'observe.ps1') -Source $row.source -VerifiedIdentity $identity
  $current=$currentRaw|ConvertFrom-Json -AsHashtable
  foreach($key in @('source','resolved_path','sha256','bytes','owner_sid','effective_sid','default_owner_sid','policy_passed','file_sddl')){if($current[$key] -cne $row.observation[$key]){throw "Source observation changed: $key"}}
  if($current.parents.Count -ne $row.observation.parents.Count){throw 'Parent policy roster changed'}
  for($i=0;$i -lt $current.parents.Count;$i++){foreach($key in @('path','sddl','owner_sid')){if($current.parents[$i][$key] -cne $row.observation.parents[$i][$key]){throw 'Parent policy changed'}}}
  $target=Get-AirkCheckedPath -Path $current.resolved_path -Root (Join-Path $root 'artifacts')
  $item=Get-Item -LiteralPath $target -Force
  if($item.PSIsContainer){throw 'Directory deletion refused'}
  $intent=Write-ImmutableReceipt ('intent-{0:D6}.json' -f $ordinal) @{schema='airk.source-cleanup-intent/1';pre_removal=$preflight;row=$row;current=$current;observed_utc=[DateTime]::UtcNow.ToString('o')}
  # Recheck after writing intent; no recursive operation or command interpolation.
  Assert-AirkOutputPathObservation -Path $target -Root $root -Identity $identity|Out-Null
  if((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -cne $row.source_sha256 -or (Get-Item -LiteralPath $target).Length -ne $row.source_bytes){throw 'Source drift immediately before removal'}
  $finalAcl=Get-Acl -LiteralPath $target -ErrorAction Stop
  if($finalAcl.GetOwner([Security.Principal.SecurityIdentifier]).Value -cne $sid -or $finalAcl.Sddl -cne $row.observation.file_sddl){throw 'Source owner or file access policy changed immediately before removal'}
  Remove-Item -LiteralPath $target -ErrorAction Stop
  if(Test-Path -LiteralPath $target){throw 'Source still exists after removal'}
  $removed=$true
 }catch{$failure=$_.Exception.Message}
 $result=@{schema='airk.source-cleanup-result/1';source=$row.source;source_sha256=$row.source_sha256;source_bytes=$row.source_bytes;removed=$removed;error=$failure;intent=$intent;pre_removal=$preflight;observed_utc=[DateTime]::UtcNow.ToString('o')}
 $resultPin=Write-ImmutableReceipt ('result-{0:D6}.json' -f $ordinal) $result
 $results+=@{source=$row.source;removed=$removed;receipt=$resultPin}
 if($null -ne $failure){break}
}
$summary=Write-ImmutableReceipt 'summary.json' @{schema='airk.source-cleanup-summary/1';pre_removal=$preflight;results=$results;selected=$plan.ready_count;removed=@($results|Where-Object removed).Count;complete=($results.Count -eq $plan.ready_count -and @($results|Where-Object{-not $_.removed}).Count -eq 0)}
$summary|ConvertTo-Json -Compress
if(@($results|Where-Object{-not $_.removed}).Count -or $results.Count -ne $plan.ready_count){throw 'Cleanup stopped; inspect immutable per-file results'}
