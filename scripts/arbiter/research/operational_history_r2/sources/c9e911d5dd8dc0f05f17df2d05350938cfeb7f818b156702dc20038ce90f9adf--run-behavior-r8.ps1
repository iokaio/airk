# SPDX-License-Identifier: Apache-2.0
param([switch]$Run)
$ErrorActionPreference='Stop'
$base=$PSScriptRoot
$source=Join-Path $base source
if(-not $Run){throw 'Explicit -Run required after final native fixture sync'}
foreach($directory in @($base,$source,(Join-Path $base receipts))){if((Get-Acl -LiteralPath $directory).Owner -ne 'TYAI\tdotn'){throw 'Foreign input/output parent'}}
$temp=Join-Path $base test-temp-r8
if(Test-Path $temp){throw 'Fresh test temp required'}
New-Item -ItemType Directory -Path $temp | Out-Null
$env:TEMP=$temp; $env:TMP=$temp
$env:PYTHONDONTWRITEBYTECODE='1'
$original=Get-Content (Join-Path $base receipts/source-manifest.json) -Raw | ConvertFrom-Json
$paths=@($original.files.path)+@('packages/vscode/src/nativeMetadataCodec.js','packages/vscode/src/nativeMetadataCodec.d.ts') | Sort-Object -Unique
function SourceCensus {
 $rows=@(); foreach($name in $paths){$path=Join-Path $source $name; if(-not(Test-Path -LiteralPath $path -PathType Leaf)){throw "Missing source $name"}; $rows+=@{path=$name;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $path).Length}}
 return $rows
}
function OutputCensus {
 $out=Join-Path $source packages/vscode/out
 return @(Get-ChildItem -LiteralPath $out -File -Recurse | Sort-Object FullName | ForEach-Object {if((Get-Acl -LiteralPath $_.FullName).Owner -ne 'TYAI\tdotn'){throw 'Foreign compiler output'};@{path=[IO.Path]::GetRelativePath($out,$_.FullName).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant();bytes=$_.Length}})
}
$inputs=SourceCensus; $outputs=OutputCensus
$manifest=Join-Path $base receipts/behavior-inputs-r8.json
if(Test-Path $manifest){throw 'Fresh receipt required'}
$metadata=@{schema='airk.ui-behavior-inputs/1';base_head=$original.head;account=[Security.Principal.WindowsIdentity]::GetCurrent().Name;sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;node=(& node --version);files=$inputs;compiled_outputs=$outputs;note='Fresh public source snapshot with exact retained generation/version deltas; no Git identity claimed for snapshot'}
[IO.File]::WriteAllText($manifest,($metadata|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
Set-Location $source
$results=@()
$batches=@(
 @{name='admin-shared';args=@('--test','--test-concurrency=4','packages/admin/test/*.test.mjs','packages/ui/test/*.test.mjs')},
 @{name='editor-compiled';args=@('--test','--test-concurrency=4','packages/vscode/out/test/*.test.js')}
)
foreach($batch in $batches){
 $log=Join-Path $base ('receipts/behavior-'+$batch.name+'-r8.log'); if(Test-Path $log){throw 'Fresh log required'}
 $timer=[Diagnostics.Stopwatch]::StartNew(); & node @($batch.args) *> $log; $status=$LASTEXITCODE; $timer.Stop()
 $results+=@{suite=$batch.name;argv=@('node')+$batch.args;cwd=$source;exit_code=$status;elapsed_seconds=$timer.Elapsed.TotalSeconds;input_manifest_sha256=(Get-FileHash $manifest -Algorithm SHA256).Hash.ToLowerInvariant()}
 $after=SourceCensus
 if(($inputs|ConvertTo-Json -Depth 5 -Compress) -cne ($after|ConvertTo-Json -Depth 5 -Compress)){throw 'Consumed source changed during tests'}
 $afterOutputs=OutputCensus
 if(($outputs|ConvertTo-Json -Depth 5 -Compress) -cne ($afterOutputs|ConvertTo-Json -Depth 5 -Compress)){throw 'Consumed compiled outputs changed during tests'}
 [IO.File]::WriteAllText((Join-Path $base receipts/behavior-results-r8.json),($results|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
 Get-Content $log | Select-Object -Last 12
 if($status -ne 0){exit $status}
}