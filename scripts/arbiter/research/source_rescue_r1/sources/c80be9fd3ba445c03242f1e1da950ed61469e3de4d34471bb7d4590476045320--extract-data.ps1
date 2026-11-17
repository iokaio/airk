# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$packEvidence='D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z\windows-packaging-candidate-r3-plan'
$extractor='D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z\compiler-provenance-r1\extractor\innounp.exe'
if((Get-FileHash -LiteralPath $extractor -Algorithm SHA256).Hash.ToLowerInvariant() -ne '1c8453c198dd3fe7947f3705e59d2294ddce3dfa9f96a737e94449b119f44181'){throw 'Extractor differs'}
$proof=Get-Content -LiteralPath "$packEvidence\phase1-artifact-proof.json" -Raw | ConvertFrom-Json
$installer=$proof.signatures.installer.path
if((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ne $proof.signatures.installer.sha256){throw 'Installer differs'}
$extractEvidence="$packEvidence\exe-data"
if(Test-Path -LiteralPath $extractEvidence){throw 'Extraction destination not fresh'}
New-Item -ItemType Directory -Path $extractEvidence | Out-Null
$destination="$extractEvidence\extracted"
$extractArgs=@('-x','-b','-q','-o',"-d$destination",$installer)
$started=[DateTime]::UtcNow
$watch=[Diagnostics.Stopwatch]::StartNew()
$extractProcess=Start-Process -FilePath $extractor -ArgumentList $extractArgs -WindowStyle Hidden -PassThru -RedirectStandardOutput "$extractEvidence\stdout.log" -RedirectStandardError "$extractEvidence\stderr.log"
$actualProcess=Get-CimInstance Win32_Process -Filter "ProcessId=$($extractProcess.Id)"
$actualOwner=Invoke-CimMethod -InputObject $actualProcess -MethodName GetOwnerSid
$observation=[ordered]@{pid=$extractProcess.Id;owner_sid=$actualOwner.Sid;owner_return=$actualOwner.ReturnValue;started_utc=$started.ToString('o');executable=$extractor;argv=$extractArgs;installer_sha256=$proof.signatures.installer.sha256;data_only=$true;installer_executed=$false}
$observation|ConvertTo-Json -Depth 6|Set-Content -LiteralPath "$extractEvidence\start.json" -Encoding utf8NoBOM
if($actualOwner.Sid -ne 'S-1-5-21-3678370870-3858642417-826968105-1002'){throw 'Wrong extractor identity'}
$extractProcess.WaitForExit()
$watch.Stop()
$extractProcess.Refresh()
$observation.exit_code=$extractProcess.ExitCode
$observation.elapsed_seconds=$watch.Elapsed.TotalSeconds
$observation.ended_utc=[DateTime]::UtcNow.ToString('o')
$observation.actual_pid_absent=($null -eq (Get-CimInstance Win32_Process -Filter "ProcessId=$($extractProcess.Id)"))
$observation|ConvertTo-Json -Depth 6|Set-Content -LiteralPath "$extractEvidence\result.json" -Encoding utf8NoBOM
if($extractProcess.ExitCode -ne 0 -or -not $observation.actual_pid_absent){throw 'Data-only extraction failed or process unsettled'}
$observation|ConvertTo-Json -Depth 6
