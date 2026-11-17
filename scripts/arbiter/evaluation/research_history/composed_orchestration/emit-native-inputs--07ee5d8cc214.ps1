# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$true
$prep=(Resolve-Path -LiteralPath 'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z').Path
$run=Join-Path $prep 'nn0-native-preparation-r1'
$env:TEMP='C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\temp'
$env:TMP=$env:TEMP;$env:TMPDIR=$env:TEMP
$env:AIRK_PRIVATE_CENSUS_BASE='C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\private-census'
$report=Get-Content -LiteralPath (Join-Path $prep 'metadata/metadata-regeneration-report.private.json') -Raw|ConvertFrom-Json
$images=Get-Content -LiteralPath 'artifacts/v190/final-plan/exec-20261116T052942Z/native-test-images-r1.json' -Raw|ConvertFrom-Json
$image=$images.images|Where-Object binary_id -eq 'airk-inference'
if((Get-FileHash -LiteralPath $image.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $image.sha256 -or (Get-Acl -LiteralPath $image.path).Owner -ne 'TYAI\tdotn'){throw 'original native inference image drift/owner'}
$inventory=Get-Content -LiteralPath 'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/source-inventory.json' -Raw|ConvertFrom-Json -AsHashtable
$invocations=@();$failures=@()
foreach($row in $report.profiles){
 $upstream=switch($row.model){'arbiter-focus-qwen3-reranker-06b'{'Qwen/Qwen3-Reranker-0.6B'} 'arbiter-context-qwen3-17b'{'Qwen/Qwen3-1.7B'} 'arbiter-context-qwen3-4b'{'Qwen/Qwen3-4B'} default{throw 'unexpected emitted model'}}
 $dir=Join-Path $run $row.model;New-Item -ItemType Directory -Path $dir|Out-Null
 $source=$inventory.models[$upstream]
 $sourceDir=Join-Path $source.asset_root ('candidate-assets/'+$upstream.Replace('/','--'))
 $files=@();foreach($entry in $source.input_seals.GetEnumerator()){
  if((Split-Path $entry.Key) -eq $sourceDir){$name=[IO.Path]::GetFileName($entry.Key);if($name.EndsWith('.safetensors') -or $name -in @('config.json','model.safetensors.index.json','tokenizer.json')){if((Get-Acl -LiteralPath $entry.Key).Owner -ne 'TYAI\tdotn'){throw 'publisher source owner mismatch'};$files+=[ordered]@{path=$entry.Key;sha256=$entry.Value}}}
 }
 $originalPath=Join-Path $dir 'original-source-input.json'
 [IO.File]::WriteAllText($originalPath,(@{schema='airk.private.original-numeric-source-input/1';model=$row.model;files=$files}|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
 $env:AIRK_SOFTWARE_V2_NATIVE_INPUT=$row.resource_run_input.path
 $env:AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256=$row.resource_run_input.sha256
 $env:AIRK_SOFTWARE_V2_ORIGINAL_INPUT=$originalPath
 $env:AIRK_SOFTWARE_V2_ORIGINAL_INPUT_SHA256=(Get-FileHash -LiteralPath $originalPath -Algorithm SHA256).Hash.ToLowerInvariant()
 foreach($kind in @('template','original-roster')){
  $selector=if($kind -eq 'template'){'inference_engine::pool::arbiter::qualification::software_v2::emit_software_v2_native_numeric_template'}else{'inference_engine::pool::arbiter::qualification::software_v2::original_sources::emit_software_v2_original_reference_assets'}
  $env:AIRK_SOFTWARE_V2_NATIVE_OUTPUT=Join-Path $dir ($kind+'.json')
  $watch=[Diagnostics.Stopwatch]::StartNew();$failure=$null;$code=$null
  try{& $image.path --ignored --exact $selector --nocapture --test-threads=1 2>&1|Tee-Object -FilePath (Join-Path $dir ($kind+'.log'));$code=$LASTEXITCODE;if($code -ne 0){throw ('actual native NN0 selector failed '+$code)}}catch{$failure=$_.ToString();$failures+=$failure}finally{
   $watch.Stop();$result=[ordered]@{model=$row.model;upstream=$upstream;command=@($image.path,'--ignored','--exact',$selector,'--nocapture','--test-threads=1');image_sha256=$image.sha256;input=@{path=$env:AIRK_SOFTWARE_V2_NATIVE_INPUT;sha256=$env:AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256};original_source_input=@{path=$originalPath;sha256=$env:AIRK_SOFTWARE_V2_ORIGINAL_INPUT_SHA256};output=$env:AIRK_SOFTWARE_V2_NATIVE_OUTPUT;exit_code=$code;failure=$failure;elapsed_seconds=$watch.Elapsed.TotalSeconds;native_model_forward_invocations=0;execution_sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value};if(Test-Path -LiteralPath $result.output){$result.output_sha256=(Get-FileHash -LiteralPath $result.output -Algorithm SHA256).Hash.ToLowerInvariant()};[IO.File]::WriteAllText((Join-Path $dir ($kind+'-execution.json')),($result|ConvertTo-Json -Depth 7),[Text.UTF8Encoding]::new($false));$invocations+=$result
  }
  if($failure){break}
 }
}
[IO.File]::WriteAllText((Join-Path $run 'native-input-emissions.json'),(@{invocations=$invocations;failures=$failures;complete=($failures.Count -eq 0)}|ConvertTo-Json -Depth 10),[Text.UTF8Encoding]::new($false))
$invocations|Select-Object model,exit_code,elapsed_seconds,output_sha256,failure|ConvertTo-Json
if($failures.Count -gt 0){exit 1}