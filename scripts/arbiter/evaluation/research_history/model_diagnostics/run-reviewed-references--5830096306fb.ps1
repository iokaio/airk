# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
$reference=(Resolve-Path -LiteralPath 'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1').Path
$runs=Join-Path $reference 'runs-native-r1'
$review=Join-Path $reference 'primary-concrete-review-r1.json'
$reviewSHA='2d5a542e243b60a02cf3f84a078daa93d3a3b90b3cb850071503b772e9e4f0ce'
if((Get-FileHash -LiteralPath $review -Algorithm SHA256).Hash.ToLowerInvariant() -ne $reviewSHA){throw 'primary concrete review drift'}
$approved=Get-Content -LiteralPath $review -Raw|ConvertFrom-Json
$python=(Resolve-Path -LiteralPath 'artifacts/advanced-split/models/training-env/venv/Scripts/python.exe').Path
$env:TEMP='C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\temp';$env:TMP=$env:TEMP;$env:TMPDIR=$env:TEMP
$env:AIRK_PRIVATE_CENSUS_BASE='C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\private-census'
$results=@();$failed=$false
foreach($tier in @('focus','compact','large')){
 foreach($kind in @('original_fp32','same_artifact')){
  if($failed){break}
  $name=$tier+'-'+$kind
  $output=Join-Path $runs $name
  if(Test-Path -LiteralPath $output){throw 'fresh reference output required; never retry existing phase'}
  $pin=@($approved.plans|Where-Object {[IO.Path]::GetFileName($_.path) -eq $name+'.json'})
  if($pin.Count -ne 1){throw 'exact approved phase not found'};$pin=$pin[0]
  if((Get-FileHash -LiteralPath $pin.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pin.sha256){throw 'reviewed plan drift'}
  $plan=Get-Content -LiteralPath $pin.path -Raw|ConvertFrom-Json
  $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  if($sid -ne $plan.human_sid){throw 'batch human identity changed'}
  $gpuRaw=@(& 'C:\WINDOWS\system32\nvidia-smi.exe' --query-gpu=index,name,uuid,memory.total,memory.free,driver_version --format=csv,noheader,nounits)
  if($LASTEXITCODE -ne 0 -or $gpuRaw.Count -ne 1){throw 'actual single local GPU observation required'}
  $gpu=$gpuRaw[0].Split(',')|ForEach-Object {$_.Trim()}
  if($gpu[0] -ne '0' -or $gpu[1] -ne $plan.expected_gpu_name -or $gpu[2] -ne 'GPU-08e2b3f6-6a39-3456-77cb-a57287a702ec' -or ([long]$gpu[4]*1MB) -lt $plan.limits.minimum_cuda_free_bytes){throw 'actual GPU identity/free reserve does not match reviewed card'}
  $command=@('-B',(Join-Path $reference 'reference_parent.py'),'--plan',$pin.path,'--plan-sha256',$pin.sha256,'--output',$output,'--execute')
  $watch=[Diagnostics.Stopwatch]::StartNew();$failure=$null;$code=$null
  try{& $python @command 2>&1|Tee-Object -FilePath (Join-Path $runs ($name+'-parent.log'));$code=$LASTEXITCODE;if($code -ne 0){throw ('actual owned reference phase failed '+$code)};$actual=Get-Content -LiteralPath (Join-Path $output 'execution.json') -Raw|ConvertFrom-Json;if(-not $actual.complete -or $null -ne $actual.failure){throw 'actual original parent did not report complete'}}catch{$failure=$_.ToString();$failed=$true}finally{
   $watch.Stop();$result=[ordered]@{phase=$name;kind=$kind;tier=$tier;primary_review=@{path=$review;sha256=$reviewSHA};plan=@{path=$pin.path;sha256=$pin.sha256};command=@($python)+$command;execution_sid=$sid;gpu_observation_before=$gpuRaw;elapsed_seconds=$watch.Elapsed.TotalSeconds;exit_code=$code;failure=$failure;output=$output;automatic_retry=$false;qualification_granted=$false}
   $execution=Join-Path $output 'execution.json';if(Test-Path -LiteralPath $execution){$result.actual_execution=@{path=$execution;bytes=(Get-Item -LiteralPath $execution).Length;sha256=(Get-FileHash -LiteralPath $execution -Algorithm SHA256).Hash.ToLowerInvariant()}}
   if(Test-Path -LiteralPath $output){$acl=Get-Acl -LiteralPath $output;$result.output_owner=$acl.Owner;$result.output_sddl=$acl.Sddl;if($acl.Owner -ne 'TYAI\tdotn'){throw 'actual output owner changed'}}
   $results+=$result;[IO.File]::WriteAllText((Join-Path $runs 'batch-observations.json'),(@{complete=($results.Count -eq 6 -and -not $failed);failed=$failed;results=$results;automatic_retry=$false}|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))
   [pscustomobject]$result|ConvertTo-Json -Depth 8
  }
 }
 if($failed){break}
}
if($failed){exit 1}