# SPDX-License-Identifier: Apache-2.0
param([string]$Source,[switch]$BatchStdin,[object]$VerifiedIdentity)
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
if($null -eq $VerifiedIdentity){$identity=Assert-AirkHumanExecution $sid}else{$identity=$VerifiedIdentity;if($identity.UserSid -cne $sid){throw 'Wrong supplied human identity'};Assert-AirkCurrentToken $identity}
function Get-SourceObservation([string]$Entry){
 Assert-AirkCurrentToken $identity
 if($Entry -notmatch '^artifacts/' -or $Entry.Contains('\') -or $Entry.Contains(':') -or @($Entry.Split('/')|Where-Object{$_ -in @('','.','..')}).Count){throw 'Canonical artifact source required'}
 $path=[IO.Path]::GetFullPath((Join-Path $root $Entry))
 $check=Assert-AirkOutputPathObservation -Path $path -Root $root -Identity $identity
 $item=Get-Item -LiteralPath $path -Force
 if($item.PSIsContainer -or $item.Length -gt 16777216){throw 'Bounded ordinary file required'}
 $acl=Get-Acl -LiteralPath $path
 $parents=@();$cursor=$check.Parent
 while($true){$parentAcl=Get-Acl -LiteralPath $cursor;$parents+=@{path=$cursor;sddl=$parentAcl.Sddl;owner_sid=$parentAcl.GetOwner([Security.Principal.SecurityIdentifier]).Value};if($cursor -eq $root){break};$cursor=[IO.Path]::GetDirectoryName($cursor)}
 $raw=[IO.File]::ReadAllBytes($path)
 if($raw.Length -ne $item.Length -or (Get-Item -LiteralPath $path).LastWriteTimeUtc -ne $item.LastWriteTimeUtc){throw 'Source changed while observing'}
 Assert-AirkCurrentToken $identity
 return [ordered]@{source=$Entry;resolved_path=$path;sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($raw)).ToLowerInvariant();bytes=$raw.Length;owner_sid=$acl.GetOwner([Security.Principal.SecurityIdentifier]).Value;effective_sid=$identity.UserSid;default_owner_sid=$identity.UserSid;policy_passed=$true;file_sddl=$acl.Sddl;parents=$parents}
}
if($BatchStdin){
 if($Source){throw 'Choose one observation mode'}
 $inputText=[Console]::In.ReadToEnd();if($inputText.Length -gt 16777216){throw 'Bounded observation roster required'}
 $sources=ConvertFrom-Json -InputObject $inputText -NoEnumerate
 if($sources -isnot [array] -or $sources.Count -lt 1 -or $sources.Count -gt 16380){throw 'Nonempty bounded source array required'}
 $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
 $rows=@(foreach($entry in $sources){
  if($entry -isnot [string] -or !$seen.Add($entry)){throw 'Unique source string required'}
  try{Get-SourceObservation $entry}catch{[ordered]@{source=$entry;error=$_.Exception.Message}}
 })
 ConvertTo-Json -InputObject $rows -Depth 12 -Compress
}else{
 if(!$Source){throw 'Source required'}
 Get-SourceObservation $Source|ConvertTo-Json -Depth 12 -Compress
}
