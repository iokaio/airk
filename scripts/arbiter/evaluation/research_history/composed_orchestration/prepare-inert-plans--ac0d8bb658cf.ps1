# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$true
$prep=(Resolve-Path -LiteralPath 'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z').Path
$run=Join-Path $prep 'nn0-native-preparation-r1'
$reference=(Resolve-Path -LiteralPath 'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1').Path
$env:TEMP='C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\temp';$env:TMP=$env:TEMP;$env:TMPDIR=$env:TEMP
$python=(Resolve-Path -LiteralPath 'artifacts/advanced-split/models/training-env/venv/Scripts/python.exe').Path
$review=Get-Content -LiteralPath (Join-Path $reference 'primary-review-r3.json') -Raw|ConvertFrom-Json
$report=Get-Content -LiteralPath (Join-Path $prep 'metadata/metadata-regeneration-report.private.json') -Raw|ConvertFrom-Json
$planRoot=Join-Path $reference 'plans-native-r1';if(Test-Path -LiteralPath $planRoot){throw 'fresh inert plan root required'};New-Item -ItemType Directory -Path $planRoot|Out-Null
$results=@();$failures=@()
foreach($row in $report.profiles){
 $tier=switch($row.model){'arbiter-focus-qwen3-reranker-06b'{'focus'} 'arbiter-context-qwen3-17b'{'compact'} 'arbiter-context-qwen3-4b'{'large'} default{throw 'unknown emitted model'}}
 $upstream=switch($tier){'focus'{'Qwen/Qwen3-Reranker-0.6B'} 'compact'{'Qwen/Qwen3-1.7B'} 'large'{'Qwen/Qwen3-4B'}}
 $card=$review.bounded_reference_cards.$tier
 foreach($kind in @('original_fp32','same_artifact')){
  $plan=Join-Path $planRoot ($tier+'-'+$kind+'.json')
  $nativeDir=Join-Path $run $row.model
  $command=@('-B',(Join-Path $reference 'prepare_reference.py'),'--native-input',$row.resource_run_input.path,'--template',(Join-Path $nativeDir 'template.json'),'--original-roster',(Join-Path $nativeDir 'original-roster.json'),'--upstream',$upstream,'--kind',$kind,'--human-sid','S-1-5-21-3678370870-3858642417-826968105-1002','--gpu-name','NVIDIA RTX A6000','--wall-seconds',[string]$card.wall_seconds,'--host-gib',[string]$card.host_gib,'--cuda-gib',[string]$card.cuda_gib,'--minimum-cuda-free-gib',[string]$card.minimum_cuda_free_gib,'--threads',[string]$card.threads,'--output',$plan)
  $watch=[Diagnostics.Stopwatch]::StartNew();$failure=$null;$code=$null
  try{& $python @command 2>&1|Tee-Object -FilePath (Join-Path $run ($tier+'-'+$kind+'-plan.log'));$code=$LASTEXITCODE;if($code -ne 0){throw ('inert input audit failed '+$code)}}catch{$failure=$_.ToString();$failures+=$failure}finally{
   $watch.Stop();$result=[ordered]@{tier=$tier;model=$row.model;kind=$kind;command=@($python)+$command;elapsed_seconds=$watch.Elapsed.TotalSeconds;exit_code=$code;failure=$failure;plan=$plan;selected_model_forward_invocations=0};if(Test-Path -LiteralPath $plan){$result.plan_sha256=(Get-FileHash -LiteralPath $plan -Algorithm SHA256).Hash.ToLowerInvariant();$result.plan_bytes=(Get-Item -LiteralPath $plan).Length};$results+=$result
   [IO.File]::WriteAllText((Join-Path $run 'inert-plan-preparation.json'),(@{complete=($failures.Count -eq 0 -and $results.Count -eq 6);results=$results;failures=$failures}|ConvertTo-Json -Depth 10),[Text.UTF8Encoding]::new($false))
  }
  if($failure){break}
 }
}
$results|Select-Object tier,kind,exit_code,elapsed_seconds,plan_sha256,failure|ConvertTo-Json
if($failures.Count -gt 0){exit 1}