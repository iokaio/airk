# SPDX-License-Identifier: Apache-2.0
# Create-only tracked recovery metadata from a bounded authenticated capture.
param(
 [Parameter(Mandatory=$true)][string]$Bundle,
 [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$BundleSha256,
 [Parameter(Mandatory=$true)][string]$ExpectedSid
)
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
Assert-AirkHumanExecution -ExpectedSid $ExpectedSid | Out-Null
$inputPath=[IO.Path]::GetFullPath($Bundle)
if(-not $inputPath.StartsWith(($repo+'\artifacts\'),[StringComparison]::OrdinalIgnoreCase)){throw 'Capture must be repository artifact'}
$inputStream=[IO.File]::Open($inputPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
try{
 if($inputStream.Length -gt 4MB){throw 'Capture exceeds bound'}
 $raw=[byte[]]::new([int]$inputStream.Length)
 $inputStream.ReadExactly($raw)
 if($inputStream.ReadByte() -ne -1){throw 'Capture grew'}
}finally{$inputStream.Dispose()}
$sha=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($raw)).ToLowerInvariant()
if($sha -ne $BundleSha256){throw 'Capture SHA differs'}
$capture=[Text.UTF8Encoding]::new($false,$true).GetString($raw)|ConvertFrom-Json -AsHashtable
$relative='scripts/arbiter/research/ort_dependency_recovery_r1'
if($capture.schema -ne 'airk.recovery-capture/1' -or $capture.destination -ne $relative -or $capture.roundtrip_verified -ne $true){throw 'Wrong capture contract'}
$destination=Join-Path $repo $relative
if(Test-Path -LiteralPath $destination){throw 'Recovery destination exists'}
foreach($leaf in $capture.files.Keys){
 if($leaf -notmatch '^(README\.md|source-recovery\.json|records/[0-9a-f]{64}\.json|notices/[0-9a-f]{64}\.txt)$'){throw 'Unsupported recovery output'}
 if([Text.Encoding]::UTF8.GetByteCount($capture.files[$leaf]) -gt 1MB){throw 'Output exceeds bound'}
}
foreach($dir in @($destination,(Join-Path $destination 'records'),(Join-Path $destination 'notices'))){
 Assert-AirkOutputParent -Path $dir -Root $repo -ExpectedSid $ExpectedSid | Out-Null
 New-Item -ItemType Directory -Path $dir -ErrorAction Stop | Out-Null
 Assert-AirkCreatedOutput -Path $dir -Root $repo -ExpectedSid $ExpectedSid | Out-Null
}
foreach($leaf in ($capture.files.Keys|Sort-Object)){
 $path=Join-Path $destination $leaf
 Assert-AirkOutputParent -Path $path -Root $repo -ExpectedSid $ExpectedSid | Out-Null
 $bytes=[Text.UTF8Encoding]::new($false,$true).GetBytes($capture.files[$leaf])
 $stream=[IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$stream.Write($bytes)}finally{$stream.Dispose()}
 Assert-AirkCreatedOutput -Path $path -Root $repo -ExpectedSid $ExpectedSid | Out-Null
 if((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()){throw 'Created output changed'}
}
if((Get-FileHash -LiteralPath $inputPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $BundleSha256){throw 'Capture changed after materialization'}
@{destination=$relative;files=$capture.files.Count;sources=$capture.sources;records=$capture.records;bundle_sha256=$BundleSha256}|ConvertTo-Json
