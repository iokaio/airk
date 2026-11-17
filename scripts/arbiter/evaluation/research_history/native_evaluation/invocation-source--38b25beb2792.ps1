# SPDX-License-Identifier: Apache-2.0
param(
    [Parameter(Mandatory=$true)][string]$NativeExe,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$NativeExeSha256
)
# Root runs this only after reviewing the unique-copy regression. R1 output and
# its old hardlink-producing native image remain preserved, never overwritten.
$ErrorActionPreference='Stop'
$Repo='D:\code\Github\airk';Set-Location -LiteralPath $Repo
$NativeExe=(Resolve-Path -LiteralPath $NativeExe).Path
$HarnessPrefix=[IO.Path]::GetFullPath((Join-Path $Repo 'artifacts/advanced-split/native-harnesses'))+[IO.Path]::DirectorySeparatorChar
if(-not $NativeExe.StartsWith($HarnessPrefix,[StringComparison]::OrdinalIgnoreCase)){throw 'Retain reviewed NEW native executable outsideTarget under native-harnesses first'}
if($NativeExeSha256 -eq 'c966bb75a2fb2a3fd617d4554de18b83d22f2167cda9e96511705f7e38b88c33'){throw 'Old R1 hardlink-producing image is not admitted for R2'}
$SelfBytes=[IO.File]::ReadAllBytes($PSCommandPath)
$SelfHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($SelfBytes)).ToLowerInvariant()
$Output=Join-Path $Repo 'artifacts/advanced-split/models/native-hybrid-next-read-profiles-cpu24-r2'
& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'run-native-calibration-step-r1.ps1') -Step materialize -NativeExe $NativeExe -NativeExeSha256 $NativeExeSha256 -Profiles 'artifacts/advanced-split/models/native-weight8-mixed-cpu8-cpu24-profiles-r1/inputs.json' -ProfilesSha256 'ae0f2bd9f19e374190cf0afdb2b86100f7d53933d0654801c1f812be5e616474' -Summary 'artifacts/advanced-split/models/native-hybrid-calibration-empirical-cpu24-r1/empirical.private.json' -SummarySha256 '20359845677e6f0153cf3abc52b9d4760c91601db7fc966cd8be44561f55c90d' -OutputRoot $Output
if($LASTEXITCODE -ne 0){throw 'R2 native materialization failed; preserve evidence and stop'}
if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($PSCommandPath))).ToLowerInvariant() -ne $SelfHash){throw 'R2 wrapper source drift'}
[IO.File]::WriteAllBytes((Join-Path $Output 'invocation-source.ps1'),$SelfBytes)
$Rows=@()
foreach($Name in @('qualification-inputs.json','evaluation-inputs.json')){
    $Path=Join-Path (Join-Path $Output 'materialized') $Name
    $Rows+=@{file=$Path;sha256=(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $Path).Length}
}
@{observed_at_utc=[DateTime]::UtcNow.ToString('o');wrapper_sha256=$SelfHash;native_exe=$NativeExe;native_exe_sha256=$NativeExeSha256;outputs=$Rows;scope='Native unique-copy R2 data only, same original profiles/empirical evidence. No Python CoreProfile construction or qualification/promotion.'}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath (Join-Path $Output 'invocation-receipt.json') -Encoding utf8
$Rows|ConvertTo-Json
