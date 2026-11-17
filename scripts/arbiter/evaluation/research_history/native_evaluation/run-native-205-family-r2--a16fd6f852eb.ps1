# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('trained-v2-s8','original-shipped-u8-ort130-evaluation','original-shipped-s8-ort130-evaluation','original-mixed-u8-ort130-evaluation','original-mixed-s8-ort130-evaluation')]
    [string]$Family
)
$ErrorActionPreference='Stop'
$RepoRoot='D:\code\Github\airk'
Set-Location -LiteralPath $RepoRoot
$RunRoot=Join-Path $RepoRoot ('artifacts/advanced-split/models/native-205-family-r2-'+$Family)
if(Test-Path -LiteralPath $RunRoot){throw 'Fresh private family output required; retain failed run and diagnose.'}
New-Item -ItemType Directory -Path $RunRoot | Out-Null
$PrivateTemporaryRoot=Join-Path $RunRoot 'private-temp'
New-Item -ItemType Directory -Path $PrivateTemporaryRoot | Out-Null
$TaskUserSid=[Security.Principal.WindowsIdentity]::GetCurrent().User
$PrivateAcl=[Security.AccessControl.DirectorySecurity]::new()
$PrivateAcl.SetOwner($TaskUserSid);$PrivateAcl.SetAccessRuleProtection($true,$false)
$PrivateAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($TaskUserSid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow))
Set-Acl -LiteralPath $PrivateTemporaryRoot -AclObject $PrivateAcl
$Harness=Join-Path $RepoRoot 'artifacts/advanced-split/native-harnesses/delivery-06-before-clean/airk_kernel-43b1554a9b9060c4.exe'
$Selector='inference_owner::evaluation_tests::native_205_questions_use_the_same_current_kernel_collection_chunking_and_diskann_retrieval'
$EncoderInput=Join-Path $RepoRoot 'artifacts/advanced-split/models/native-encoder-qualification-input-r1.json'
$QuestionInput=Join-Path $RepoRoot 'artifacts/advanced-split/models/native-encoder-205-input-r1.json'
$Manifest=Join-Path $RepoRoot 'artifacts/advanced-split/models/native-human-fixtures-r2/manifest.private.json'
$Output=Join-Path $RunRoot 'native-family.json'
$StopFile=Join-Path $RunRoot 'native-query-boundary-stop.json'
$Expected=@{
    $Harness='4616c1ac9075ea4c6cd8098ee9f4a062437b8dc65df4ff619ded2e8a451e1e08'
    $EncoderInput='3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81'
    $QuestionInput='2e74a873125834e17455c40879c0ba161626053fbd39742bcbbc73077d3dd046'
    $Manifest='2627e54241d8e961cc03c005c02f1655be364462ada16bbafdc0434cf4192a60'
}
foreach($Item in $Expected.GetEnumerator()) {if((Get-FileHash -LiteralPath $Item.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Item.Value){throw "Frozen family input changed: $($Item.Key)"}}
$List=& $Harness $Selector --list --exact 2>&1
$List | Set-Content -LiteralPath (Join-Path $RunRoot 'selector-list.log') -Encoding utf8
if($LASTEXITCODE -ne 0 -or ($List -join "`n") -notmatch '1 test, 0 benchmarks'){throw 'Exactly one retained R2 selector required'}
$Start=[Diagnostics.ProcessStartInfo]::new();$Start.FileName=$Harness;$Start.WorkingDirectory=$RepoRoot
$Start.UseShellExecute=$false;$Start.CreateNoWindow=$true;$Start.RedirectStandardOutput=$true;$Start.RedirectStandardError=$true
foreach($Argument in @($Selector,'--ignored','--exact','--nocapture','--test-threads=1')){$Start.ArgumentList.Add($Argument)}
$EnvironmentValues=@{
    TEMP=$PrivateTemporaryRoot;TMP=$PrivateTemporaryRoot;AIRK_PRIVATE_CENSUS_BASE=$PrivateTemporaryRoot
    AIRK_NATIVE_ENCODER_INPUT=$EncoderInput;AIRK_NATIVE_RETRIEVAL_INPUT=$QuestionInput
    AIRK_NATIVE_ENCODER_FAMILY=$Family;AIRK_NATIVE_RETRIEVAL_SOURCE_MANIFEST=$Manifest
    AIRK_NATIVE_RETRIEVAL_SOURCE_MANIFEST_SHA256=$Expected[$Manifest]
    AIRK_NATIVE_RETRIEVAL_OUTPUT=$Output;AIRK_NATIVE_RETRIEVAL_STOP_FILE=$StopFile
}
foreach($Item in $EnvironmentValues.GetEnumerator()){$Start.Environment[$Item.Key]=[string]$Item.Value}
$StdoutPath=Join-Path $RunRoot 'native-stdout.log';$StderrPath=Join-Path $RunRoot 'native-stderr.log'
$Stdout=[IO.File]::Create($StdoutPath);$Stderr=[IO.File]::Create($StderrPath)
$Process=[Diagnostics.Process]::new();$Process.StartInfo=$Start
$Started=[DateTime]::UtcNow.ToString('o');$Timer=[Diagnostics.Stopwatch]::StartNew()
if(-not $Process.Start()){throw 'Native family process did not start'}
$CopyOut=$Process.StandardOutput.BaseStream.CopyToAsync($Stdout)
$CopyError=$Process.StandardError.BaseStream.CopyToAsync($Stderr)
$StartReceipt=[ordered]@{family=$Family;pid=$Process.Id;command=@($Harness)+@($Start.ArgumentList);environment=$EnvironmentValues;input_pins=$Expected;started_at_utc=$Started;native_cap_seconds=3600;scope='Actual one-family native vector205. No other baseline query, no human grading; no force kill or boundary pause inside a family.'}
$StartReceipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $RunRoot 'started.json') -Encoding utf8
$BoundaryStopRequested=$false
if(-not $Process.WaitForExit(3600000)){
    $BoundaryStopRequested=$true
    @{reason='one-hour native cap reached';requested_at_utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath $StopFile -Encoding utf8
    Write-Output (@{family=$Family;stage='native-hour-cap-boundary-stop-requested';pid=$Process.Id;force_killed=$false}|ConvertTo-Json -Compress)
    $Process.WaitForExit()
}
$CopyOut.GetAwaiter().GetResult();$CopyError.GetAwaiter().GetResult();$Stdout.Dispose();$Stderr.Dispose();$Timer.Stop()
$Stable=$true
foreach($Item in $Expected.GetEnumerator()){if((Get-FileHash -LiteralPath $Item.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Item.Value){$Stable=$false}}
$Log=(Get-Content -LiteralPath $StdoutPath -Raw)+(Get-Content -LiteralPath $StderrPath -Raw)
$Complete=$false
if(Test-Path -LiteralPath $Output){$Native=Get-Content -LiteralPath $Output -Raw|ConvertFrom-Json;$Complete=$Native.complete -eq $true -and $Native.family -eq $Family -and $Native.records.Count -eq 205}
$Receipt=[ordered]@{schema='airk.private-native-205-family-run/1';family=$Family;command=$StartReceipt.command;environment=$EnvironmentValues;input_pins=$Expected;pid=$Process.Id;started_at_utc=$Started;finished_at_utc=[DateTime]::UtcNow.ToString('o');elapsed_seconds=$Timer.Elapsed.TotalSeconds;returncode=$Process.ExitCode;selected_tests=1;input_pins_unchanged=$Stable;native_complete=$Complete;hour_cap_boundary_stop_requested=$BoundaryStopRequested;output=$Output;scope='Vector-only actual same205 native family; source/index/pipeline before-after proof owned by native R2 harness. No human metrics or release qualification.'}
$Receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $RunRoot 'run-receipt.json') -Encoding utf8
if($Process.ExitCode -ne 0 -or -not $Stable -or -not $Complete -or $BoundaryStopRequested -or $Log -notmatch '1 passed; 0 failed'){throw 'Native family failed or incomplete; preserve evidence and STOP batch, no unchanged retry.'}
$Receipt | ConvertTo-Json -Depth 8
