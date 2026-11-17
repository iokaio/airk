# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory=$true)][ValidateSet('capture','transcripts','score','materialize')][string]$Step,
    [Parameter(Mandatory=$true)][string]$NativeExe,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$NativeExeSha256,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [string]$Profiles,[string]$ProfilesSha256,
    [string]$Capture,[string]$CaptureSha256,
    [string]$Transcripts,[string]$TranscriptsSha256,
    [string]$Summary,[string]$SummarySha256,
    [ValidateSet('arbiter-small-intern-08b','arbiter-small-strands-2b','arbiter-large-intern-4b','arbiter-large-kev-4b')][string]$Model,
    [switch]$PlanOnly
)
# Private execution helper, no Cargo or automatic step chaining. The primary
# schedules each actual model process and reviews the empirical summary before
# materialize. Runtime/native contract instances are consumed unchanged.
$ErrorActionPreference='Stop'
$Repo='D:\code\Github\airk'
$Models=Join-Path $Repo 'artifacts/advanced-split/models'
Set-Location -LiteralPath $Repo
$Pinned=@{}
$SelfBytes=[IO.File]::ReadAllBytes($PSCommandPath)
$Pinned[$PSCommandPath]=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($SelfBytes)).ToLowerInvariant()
function Add-Input([string]$Path,[string]$Expected) {
    if(-not $Path -or $Expected -notmatch '^[0-9a-f]{64}$'){throw 'Exact input path and lowercase SHA256 required'}
    $Full=[IO.Path]::GetFullPath($Path)
    if((Get-FileHash -LiteralPath $Full -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Expected){throw "Input pin mismatch: $Full"}
    $Pinned[$Full]=$Expected
    return $Full
}
$NativeExe=Add-Input $NativeExe $NativeExeSha256
$OutputRoot=[IO.Path]::GetFullPath($OutputRoot)
$PrivatePrefix=[IO.Path]::GetFullPath($Models)+[IO.Path]::DirectorySeparatorChar
if(-not $OutputRoot.StartsWith($PrivatePrefix,[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $OutputRoot)){throw 'Fresh output strictly beneath private models required'}
$FixturePrefix=[IO.Path]::GetFullPath((Join-Path $Models 'native-human-fixtures-r2'))
if($OutputRoot.Equals($FixturePrefix,[StringComparison]::OrdinalIgnoreCase) -or $OutputRoot.StartsWith($FixturePrefix+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Output must be outside frozen fixture tree'}
$OutputAncestor=[IO.DirectoryInfo]::new($OutputRoot)
while($null -ne $OutputAncestor){
    if($OutputAncestor.Exists -and (($OutputAncestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)){throw 'Output ancestor alias is not admitted by this private helper'}
    $OutputAncestor=$OutputAncestor.Parent
}
$EnvironmentValues=@{}
switch($Step){
    'capture' {
        $Selector='arbiter_calibration_capture::hybrid_calibration_choices_retain_actual_owned_encoder_and_canonical_read_spans'
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_CAPTURE_INPUT=Add-Input (Join-Path $Models 'native-calibration-input-r1.json') 'a6149309a5838550a91cf47dd7489380509f82781550fb720fe78f8c059287b7'
        $Encoder=Join-Path $Models 'minilm-exports-mixed-r6/u8'
        $null=Add-Input (Join-Path $Encoder 'model.json') '5dad1e52c1f0710a139e6111898c8ae5cb804fa29d0b854917337adb134d6e32'
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_CAPTURE_ENCODER=$Encoder
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_CAPTURE_OUTPUT=Join-Path $OutputRoot 'native-output.json'
    }
    'transcripts' {
        $Selector='arbiter::tasks::tests::native_calibration_transcripts_bind_exact_task_policy_aliases_and_compiler_inputs'
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_TRANSCRIPT_INPUT=Add-Input $Capture $CaptureSha256
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_TRANSCRIPT_PROFILES=Add-Input $Profiles $ProfilesSha256
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_TRANSCRIPT_OUTPUT=Join-Path $OutputRoot 'native-output.json'
    }
    'score' {
        if(-not $Model){throw 'One explicit closed model required'}
        $Selector='inference_engine::pool::arbiter::qualification::actual_native_calibration_task_vectors_produce_raw_scores_without_release_admission'
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_SCORE_TRANSCRIPT=Add-Input $Transcripts $TranscriptsSha256
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_SCORE_TRANSCRIPT_SHA256=$TranscriptsSha256
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_SCORE_PROFILES=Add-Input $Profiles $ProfilesSha256
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_SCORE_PROFILES_SHA256=$ProfilesSha256
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_SCORE_MODEL=$Model
        $EnvironmentValues.AIRK_ARBITER_CALIBRATION_SCORE_OUTPUT=Join-Path $OutputRoot 'native-output.json'
    }
    'materialize' {
        $Selector='inference_engine::pool::arbiter::qualification::materialize_private_next_read_profiles_from_actual_native_empirical_evidence'
        $EnvironmentValues.AIRK_ARBITER_EVALUATION_MATERIALIZE_SOURCES=Add-Input $Profiles $ProfilesSha256
        $EnvironmentValues.AIRK_ARBITER_EVALUATION_MATERIALIZE_SOURCES_SHA256=$ProfilesSha256
        $EnvironmentValues.AIRK_ARBITER_EVALUATION_MATERIALIZE_SUMMARY=Add-Input $Summary $SummarySha256
        $EnvironmentValues.AIRK_ARBITER_EVALUATION_MATERIALIZE_SUMMARY_SHA256=$SummarySha256
        $EnvironmentValues.AIRK_ARBITER_EVALUATION_MATERIALIZE_OUTPUT=Join-Path $OutputRoot 'materialized'
    }
}
$Arguments=@($Selector,'--ignored','--exact','--nocapture','--test-threads=1')
$List=& $NativeExe $Selector --list --exact 2>&1
if($LASTEXITCODE -ne 0 -or ($List -join "`n") -notmatch '1 test, 0 benchmarks'){throw 'Native executable must expose exactly one selected test'}
$Plan=[ordered]@{schema='airk.private-native-calibration-step/1';step=$Step;command=@($NativeExe)+$Arguments;input_pins=$Pinned;environment=$EnvironmentValues;output_root=$OutputRoot;plan_only=[bool]$PlanOnly}
if($PlanOnly){$Plan|ConvertTo-Json -Depth 8;return}
New-Item -ItemType Directory -Path $OutputRoot|Out-Null
[IO.File]::WriteAllBytes((Join-Path $OutputRoot 'runner-source.ps1'),$SelfBytes)
$PrivateTemp=Join-Path $OutputRoot 'private-temp';New-Item -ItemType Directory -Path $PrivateTemp|Out-Null
$UserSid=[Security.Principal.WindowsIdentity]::GetCurrent().User
$Acl=[Security.AccessControl.DirectorySecurity]::new();$Acl.SetOwner($UserSid);$Acl.SetAccessRuleProtection($true,$false)
$Acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($UserSid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow))
Set-Acl -LiteralPath $PrivateTemp -AclObject $Acl
$EnvironmentValues.TEMP=$PrivateTemp;$EnvironmentValues.TMP=$PrivateTemp;$EnvironmentValues.AIRK_PRIVATE_CENSUS_BASE=$PrivateTemp
$Plan.started_at_utc=[DateTime]::UtcNow.ToString('o')
$Plan|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputRoot 'started.json') -Encoding utf8
foreach($Entry in $EnvironmentValues.GetEnumerator()){[Environment]::SetEnvironmentVariable($Entry.Key,[string]$Entry.Value,'Process')}
& $NativeExe @Arguments 2>&1|Tee-Object -FilePath (Join-Path $OutputRoot 'native.log')
$NativeExit=$LASTEXITCODE
$Unchanged=$true
foreach($Entry in $Pinned.GetEnumerator()){if((Get-FileHash -LiteralPath $Entry.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Entry.Value){$Unchanged=$false}}
$Plan.finished_at_utc=[DateTime]::UtcNow.ToString('o');$Plan.returncode=$NativeExit;$Plan.input_pins_unchanged=$Unchanged
$Plan|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $OutputRoot 'run-receipt.json') -Encoding utf8
if($NativeExit -ne 0 -or -not $Unchanged -or (Get-Content -LiteralPath (Join-Path $OutputRoot 'native.log') -Raw) -notmatch '1 passed; 0 failed'){throw 'Native step failed; stop and retain evidence, no unchanged retry'}
