# SPDX-License-Identifier: Apache-2.0
param([Parameter(Mandatory=$true)][ValidateSet('arbiter-small-intern-08b','arbiter-small-strands-2b','arbiter-large-intern-4b','arbiter-large-kev-4b')][string]$Model)
$ErrorActionPreference='Stop'
$Repo='D:\code\Github\airk';Set-Location -LiteralPath $Repo
$Models=Join-Path $Repo 'artifacts/advanced-split/models'
$Run=Join-Path $Models ('native-hybrid-calibration-scores-cpu24-r1-'+$Model)
if(Test-Path -LiteralPath $Run){throw 'Fresh score output required; preserve any previous failure'}
$Exe=Join-Path $Repo 'artifacts/advanced-split/native-harnesses/calibration-cpu24-r1/airk_inference.exe'
$Profiles=Join-Path $Models 'native-weight8-mixed-cpu8-cpu24-profiles-r1/inputs.json'
$Transcripts=Join-Path $Models 'native-hybrid-calibration-transcripts-cpu24-r1.json'
$Pinned=@{$Exe='c966bb75a2fb2a3fd617d4554de18b83d22f2167cda9e96511705f7e38b88c33';$Profiles='ae0f2bd9f19e374190cf0afdb2b86100f7d53933d0654801c1f812be5e616474';$Transcripts='bbe797c2cab08c22e9faf26c960617c0a5c2af3ea305dabb1bfaac99759d6c99'}
$SelfBytes=[IO.File]::ReadAllBytes($PSCommandPath)
$Pinned[$PSCommandPath]=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($SelfBytes)).ToLowerInvariant()
foreach($Entry in $Pinned.GetEnumerator()){if((Get-FileHash -LiteralPath $Entry.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Entry.Value){throw 'Frozen scoring input mismatch'}}
$Selector='inference_engine::pool::arbiter::qualification::actual_native_calibration_task_vectors_produce_raw_scores_without_release_admission'
$List=& $Exe $Selector --list --exact 2>&1
if($LASTEXITCODE -ne 0 -or ($List -join "`n") -notmatch '1 test, 0 benchmarks'){throw 'Exactly one native scorer test required'}
New-Item -ItemType Directory -Path $Run|Out-Null
[IO.File]::WriteAllBytes((Join-Path $Run 'runner-source.ps1'),$SelfBytes)
$List|Set-Content -LiteralPath (Join-Path $Run 'selector-list.log') -Encoding utf8
$Temporary=Join-Path $Run 'private-temp';New-Item -ItemType Directory -Path $Temporary|Out-Null
$UserSid=[Security.Principal.WindowsIdentity]::GetCurrent().User
$Acl=[Security.AccessControl.DirectorySecurity]::new();$Acl.SetOwner($UserSid);$Acl.SetAccessRuleProtection($true,$false)
$Acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($UserSid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow))
Set-Acl -LiteralPath $Temporary -AclObject $Acl
$Output=Join-Path $Run 'native-scores.json'
$EnvironmentValues=@{TEMP=$Temporary;TMP=$Temporary;AIRK_PRIVATE_CENSUS_BASE=$Temporary;AIRK_ARBITER_CALIBRATION_SCORE_TRANSCRIPT=$Transcripts;AIRK_ARBITER_CALIBRATION_SCORE_TRANSCRIPT_SHA256=$Pinned[$Transcripts];AIRK_ARBITER_CALIBRATION_SCORE_PROFILES=$Profiles;AIRK_ARBITER_CALIBRATION_SCORE_PROFILES_SHA256=$Pinned[$Profiles];AIRK_ARBITER_CALIBRATION_SCORE_MODEL=$Model;AIRK_ARBITER_CALIBRATION_SCORE_OUTPUT=$Output}
$Start=[Diagnostics.ProcessStartInfo]::new();$Start.FileName=$Exe;$Start.WorkingDirectory=$Repo;$Start.UseShellExecute=$false;$Start.CreateNoWindow=$true;$Start.RedirectStandardOutput=$true;$Start.RedirectStandardError=$true
foreach($Argument in @($Selector,'--ignored','--exact','--nocapture','--test-threads=1')){$Start.ArgumentList.Add($Argument)}
foreach($Entry in $EnvironmentValues.GetEnumerator()){$Start.Environment[$Entry.Key]=[string]$Entry.Value}
$Stdout=[IO.File]::Create((Join-Path $Run 'native-stdout.log'));$Stderr=[IO.File]::Create((Join-Path $Run 'native-stderr.log'))
$Process=[Diagnostics.Process]::new();$Process.StartInfo=$Start
$Timer=[Diagnostics.Stopwatch]::StartNew();$Started=[DateTime]::UtcNow.ToString('o')
if(-not $Process.Start()){throw 'Actual native scorer did not start'}
$CopyOut=$Process.StandardOutput.BaseStream.CopyToAsync($Stdout);$CopyError=$Process.StandardError.BaseStream.CopyToAsync($Stderr)
$Receipt=[ordered]@{schema='airk.private-native-calibration-score-run/1';model=$Model;pid=$Process.Id;command=@($Exe)+@($Start.ArgumentList);environment=$EnvironmentValues;input_pins=$Pinned;started_at_utc=$Started;maximum_seconds=3600;scope='Developer-only actual native calibration, real per-process lease/control/TEMP, same experimental retriever. Concurrent quality only; no resource/cold/performance or release claim.'}
$Receipt|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $Run 'started.json') -Encoding utf8
$Capped=-not $Process.WaitForExit(3600000)
if($Capped){$Process.Kill($true);$Process.WaitForExit()}
$null=$CopyOut.GetAwaiter().GetResult();$null=$CopyError.GetAwaiter().GetResult();$Stdout.Dispose();$Stderr.Dispose();$Timer.Stop()
$Stable=$true;foreach($Entry in $Pinned.GetEnumerator()){if((Get-FileHash -LiteralPath $Entry.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Entry.Value){$Stable=$false}}
$Complete=$false;if(Test-Path -LiteralPath $Output){$Native=Get-Content -LiteralPath $Output -Raw|ConvertFrom-Json;$Complete=$Native.complete -eq $true -and $Native.model -eq $Model -and $Native.rows.Count -eq 19}
$Receipt.finished_at_utc=[DateTime]::UtcNow.ToString('o');$Receipt.elapsed_seconds=$Timer.Elapsed.TotalSeconds;$Receipt.returncode=$Process.ExitCode;$Receipt.hour_cap_terminated_process=$Capped;$Receipt.input_pins_unchanged=$Stable;$Receipt.native_complete=$Complete
$Receipt|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $Run 'run-receipt.json') -Encoding utf8
$Log=(Get-Content -LiteralPath (Join-Path $Run 'native-stdout.log') -Raw)+(Get-Content -LiteralPath (Join-Path $Run 'native-stderr.log') -Raw)
if($Capped -or $Process.ExitCode -ne 0 -or -not $Complete -or -not $Stable -or $Log -notmatch '1 passed; 0 failed'){throw 'Native calibration failed/incomplete; preserve actual evidence, do not retry unchanged or invent cleanup'}
$Receipt|ConvertTo-Json -Depth 8
