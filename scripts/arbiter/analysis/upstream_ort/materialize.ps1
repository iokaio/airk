# SPDX-License-Identifier: Apache-2.0
# Create-only materialization of an authenticated metadata bundle.
param(
 [Parameter(Mandatory=$true)][string]$ExpectedSid,
 [Parameter(Mandatory=$true)][string]$Bundle,
 [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$BundleSha256,
 [Parameter(Mandatory=$true)][string]$Destination
)
function Assert-AirkCapturedBundleBytes {
 param([byte[]]$Bytes,[string]$ExpectedSha256)
 if($null -eq $Bytes -or $Bytes.Length -eq 0 -or $Bytes.Length -gt 512MB){throw 'Captured bundle exceeds size bounds'}
 $actual=[System.Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant()
 if($actual -ne $ExpectedSha256){throw 'Captured bundle SHA differs'}
}
function Read-AirkPinnedBundleBytes {
 param([string]$Path,[string]$ExpectedSha256)
 $inputStream=[System.IO.File]::Open([System.IO.Path]::GetFullPath($Path),[System.IO.FileMode]::Open,[System.IO.FileAccess]::Read,[System.IO.FileShare]::Read)
 try{
  if($inputStream.Length -gt 512MB){throw 'Bundle exceeds bound'}
  $captured=[byte[]]::new([int]$inputStream.Length)
  $offset=0
  while($offset -lt $captured.Length){
   $read=$inputStream.Read($captured,$offset,$captured.Length-$offset)
   if($read -eq 0){throw 'Bundle truncated while captured'}
   $offset+=$read
  }
  if($inputStream.ReadByte() -ne -1){throw 'Bundle grew while captured'}
 }finally{$inputStream.Dispose()}
 Assert-AirkCapturedBundleBytes -Bytes $captured -ExpectedSha256 $ExpectedSha256
 return ,$captured
}
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
Assert-AirkHumanExecution -ExpectedSid $ExpectedSid | Out-Null
Assert-AirkCreatedOutput -Path ([System.IO.Path]::GetFullPath($Bundle)) -Root $repo -ExpectedSid $ExpectedSid | Out-Null
$snapshot=Read-AirkPinnedBundleBytes -Path $Bundle -ExpectedSha256 $BundleSha256
if($Destination -notmatch '^artifacts/[a-zA-Z0-9_./-]+$' -or $Destination.Split('/') -contains '..'){throw 'Invalid destination'}
$output=[System.IO.Path]::GetFullPath((Join-Path $repo $Destination))
$prefix=(Join-Path $repo 'artifacts')+[System.IO.Path]::DirectorySeparatorChar
if(-not $output.StartsWith($prefix,[System.StringComparison]::OrdinalIgnoreCase)){throw 'Output escapes artifacts'}
Assert-AirkOutputParent -Path $output -Root $repo -ExpectedSid $ExpectedSid | Out-Null
if(Test-Path -LiteralPath $output){throw 'Destination must be fresh'}
[void][System.IO.Directory]::CreateDirectory($output)
Assert-AirkCreatedOutput -Path $output -Root $repo -ExpectedSid $ExpectedSid | Out-Null
$groups=[System.Collections.Generic.HashSet[string]]::new()
$files=@();$maps=@()
$memory=[System.IO.MemoryStream]::new($snapshot,$false)
$reader=[System.IO.StreamReader]::new($memory,[System.Text.UTF8Encoding]::new($false,$true))
try{
 while($null -ne ($line=$reader.ReadLine())){
  if($line.Length -gt 128MB){throw 'Bundle line exceeds bound'}
  $item=$line|ConvertFrom-Json -Depth 20
  if($item.schema -ne 'airk.private.projection-bundle/1' -or $item.destination -ne $Destination -or $item.group -notmatch '^[a-z0-9-]+$' -or -not $groups.Add($item.group)){throw 'Invalid/duplicate bundle group'}
  $folder=Join-Path $output $item.group
  Assert-AirkOutputParent -Path $folder -Root $repo -ExpectedSid $ExpectedSid | Out-Null
  if(Test-Path -LiteralPath $folder){throw 'Group already exists'}
  [void][System.IO.Directory]::CreateDirectory($folder)
  Assert-AirkCreatedOutput -Path $folder -Root $repo -ExpectedSid $ExpectedSid | Out-Null
  foreach($property in $item.files.PSObject.Properties){
   if($property.Name -notin @('projection.json','source-disposition.json','upstream-tree.json','summary.json')){throw 'Unexpected output name'}
   $bytes=[System.Text.Encoding]::UTF8.GetBytes([string]$property.Value)
   if($bytes.Length -gt 16MB){throw 'Metadata file exceeds bound'}
   $target=Join-Path $folder $property.Name
   Assert-AirkOutputParent -Path $target -Root $repo -ExpectedSid $ExpectedSid | Out-Null
   $stream=[System.IO.File]::Open($target,[System.IO.FileMode]::CreateNew)
   try{$stream.Write($bytes,0,$bytes.Length)}finally{$stream.Dispose()}
   Assert-AirkCreatedOutput -Path $target -Root $repo -ExpectedSid $ExpectedSid | Out-Null
   $pin=[pscustomobject]@{path=[System.IO.Path]::GetRelativePath($repo,$target).Replace('\','/');sha256=(Get-FileHash -LiteralPath $target).Hash.ToLowerInvariant();bytes=$bytes.Length}
   $files+=$pin
   if($property.Name -eq 'source-disposition.json'){$maps+=$pin}
  }
 }
}finally{$reader.Dispose()}
if((Get-FileHash -LiteralPath $Bundle).Hash -ne $BundleSha256){throw 'Bundle changed during materialization'}
$index=Join-Path $output 'projection-index.json'
Assert-AirkOutputParent -Path $index -Root $repo -ExpectedSid $ExpectedSid | Out-Null
$record=[ordered]@{schema='airk.private.upstream-projection-index/1';bundle_sha256=$BundleSha256;destination=$Destination;files=$files;maps=$maps;qualification=$false}
$bytes=[System.Text.Encoding]::UTF8.GetBytes(($record|ConvertTo-Json -Depth 8)+[char]10)
$stream=[System.IO.File]::Open($index,[System.IO.FileMode]::CreateNew)
try{$stream.Write($bytes,0,$bytes.Length)}finally{$stream.Dispose()}
Assert-AirkCreatedOutput -Path $index -Root $repo -ExpectedSid $ExpectedSid | Out-Null
[pscustomobject]@{index=$index;sha256=(Get-FileHash -LiteralPath $index).Hash.ToLowerInvariant();maps=$maps.Count;files=$files.Count}|ConvertTo-Json
