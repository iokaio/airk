# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$prep=(Resolve-Path -LiteralPath 'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z').Path
$run=Join-Path $prep 'nn0-native-preparation-r1'
$reportPath=Join-Path $prep 'metadata/metadata-regeneration-report.private.json'
$report=Get-Content -LiteralPath $reportPath -Raw|ConvertFrom-Json
if(-not $report.complete -or $report.NN_invocations -ne 0 -or $report.profiles.Count -ne 3 -or $report.native_source_group -ne '5f788569885d7b33b476f3c0381b55528f3981a000fe960d884b0128a997075c'){throw 'metadata source/report incomplete'}
$catalog=Get-Content -LiteralPath 'crates/airk-retrieval/src/arbiter/release-catalog.json' -Raw|ConvertFrom-Json
$flat=Join-Path $prep 'flat-current-models'
if(Test-Path -LiteralPath $flat){throw 'fresh flat root required'}
$profiles=@();$total=[long]0;$preserved=0;$changed=@()
foreach($row in $report.profiles){
 if((Get-FileHash -LiteralPath $row.profile.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $row.profile.sha256){throw 'profile pin drift'}
 $p=Get-Content -LiteralPath $row.profile.path -Raw|ConvertFrom-Json
 $old=@($catalog.profiles|Where-Object model -eq $p.model)
 if($old.Count -ne 1){throw 'original model catalog not exact'};$old=$old[0]
 if($p.manifest -ne $row.manifest -or $p.qualification -or ($null -ne $p.qualified_targets -and @($p.qualified_targets).Count -ne 0) -or ($null -ne $p.downloads -and @($p.downloads).Count -ne 0)){throw 'unqualified private profile required'}
 $focus=$p.model -eq 'arbiter-focus-qwen3-reranker-06b'
 if(($focus -and $p.max_tokens -ne 512) -or (-not $focus -and $p.max_tokens -ne 8192)){throw 'native model width mismatch'}
 foreach($asset in $p.assets){
  if([IO.Path]::GetFileName($asset.name) -ne $asset.name){throw 'flat logical name required'}
  $before=@($old.assets|Where-Object name -eq $asset.name)
  if($before.Count -eq 1 -and [System.Text.Json.Nodes.JsonNode]::DeepEquals([System.Text.Json.Nodes.JsonNode]::Parse(($before[0]|ConvertTo-Json -Compress -Depth 8)),[System.Text.Json.Nodes.JsonNode]::Parse(($asset|ConvertTo-Json -Compress -Depth 8)))){$preserved++}else{if($focus -or $asset.role -notin @('compiler','calibration')){throw 'nonmetadata identity changed'};$changed+=[pscustomobject]@{model=$p.model;name=$asset.name;role=$asset.role;sha256=$asset.sha256}}
  $total += $asset.bytes
 }
 if((Get-FileHash -LiteralPath $row.asset_files.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $row.asset_files.sha256){throw 'native asset map drift'}
 $files=Get-Content -LiteralPath $row.asset_files.path -Raw|ConvertFrom-Json -AsHashtable
 if($files.Count -ne $p.assets.Count){throw 'native map count mismatch'}
 $profiles += [pscustomobject]@{row=$row;profile=$p;files=$files}
}
if($changed.Count -ne 4){throw 'exact four new Context metadata assets required'}
$disk=Get-PSDrive D
if($disk.Free -lt $total + (4GB)){throw 'insufficient real destination disk'}
New-Item -ItemType Directory -Path $flat|Out-Null
$watch=[Diagnostics.Stopwatch]::StartNew();$copied=@();$failure=$null
try{
 foreach($item in $profiles){
  $dir=Join-Path $flat $item.profile.manifest
  New-Item -ItemType Directory -Path $dir|Out-Null
  Copy-Item -LiteralPath $item.row.profile.path -Destination (Join-Path $dir 'profile.json') -ErrorAction Stop
  if((Get-FileHash -LiteralPath (Join-Path $dir 'profile.json') -Algorithm SHA256).Hash.ToLowerInvariant() -ne $item.row.profile.sha256){throw 'flat profile bytes changed'}
  foreach($asset in $item.profile.assets){
   $from=[IO.Path]::GetFullPath($item.files[$asset.name]);$to=Join-Path $dir $asset.name
   if(-not $from.StartsWith($prep+'\') -and -not $from.StartsWith((Split-Path $prep)+'\final-plan-acquired-assets-20261116T071657Z\')){throw 'source outside approved asset/metadata roots'}
   $info=Get-Item -LiteralPath $from
   if((Get-Acl -LiteralPath $from).Owner -ne 'TYAI\tdotn' -or ($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $info.Length -ne $asset.bytes){throw 'source owner/type/length mismatch'}
   $src=[IO.File]::Open($from,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
   try{
    $sha=[Security.Cryptography.SHA256]::Create()
    try{$actual=[Convert]::ToHexString($sha.ComputeHash($src)).ToLowerInvariant()}finally{$sha.Dispose()}
    if($actual -ne $asset.sha256){throw 'actual source asset SHA drift'}
    $src.Position=0
    if((Get-PSDrive D).Free -lt $asset.bytes+4GB){throw 'destination disk reserve exhausted'}
    $dst=[IO.File]::Open($to,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try{$src.CopyTo($dst,1MB);$dst.Flush($true)}finally{$dst.Dispose()}
   }finally{$src.Dispose()}
   $out=Get-Item -LiteralPath $to
   if((Get-Acl -LiteralPath $to).Owner -ne 'TYAI\tdotn' -or $out.Length -ne $asset.bytes -or (Get-FileHash -LiteralPath $to -Algorithm SHA256).Hash.ToLowerInvariant() -ne $asset.sha256){throw 'actual copied destination mismatch'}
   $entry=[pscustomobject]@{model=$item.profile.model;manifest=$item.profile.manifest;name=$asset.name;role=$asset.role;bytes=$asset.bytes;sha256=$asset.sha256;source=$from;destination=$to;source_owner='TYAI\tdotn';destination_owner='TYAI\tdotn';copy_method='ordinary CreateNew stream copy; source read-locked while hashed/copied; destination reopened and hashed'}
   $copied+=$entry
   [IO.File]::AppendAllText((Join-Path $run 'flat-copy-census.jsonl'),($entry|ConvertTo-Json -Compress)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
  }
 }
}catch{$failure=$_.ToString();throw}finally{
 $watch.Stop()
 $result=[ordered]@{complete=($failure -eq $null);failure=$failure;metadata_report_sha256=(Get-FileHash -LiteralPath $reportPath -Algorithm SHA256).Hash.ToLowerInvariant();expected_bytes=$total;copied_bytes=($copied|Measure-Object bytes -Sum).Sum;copied_files=$copied.Count;preserved_original_assets=$preserved;new_context_metadata=$changed;elapsed_seconds=$watch.Elapsed.TotalSeconds;flat_root=$flat;native_model_forward_invocations=0;remaining_disk_bytes=(Get-PSDrive D).Free}
 [IO.File]::WriteAllText((Join-Path $run 'flat-copy-summary.json'),($result|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
 $result|ConvertTo-Json -Depth 8
}