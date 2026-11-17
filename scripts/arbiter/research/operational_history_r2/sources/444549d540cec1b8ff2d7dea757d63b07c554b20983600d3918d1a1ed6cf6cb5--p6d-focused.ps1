# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
Set-Location D:/code/Github/airk
$env:PATH='D:/code/Github/airk/artifacts/tools/nextest;C:/Users/tdotn/.cargo/bin;'+$env:PATH
$env:CARGO_BUILD_JOBS='4'
$env:GIT_CONFIG_COUNT='1'
$env:GIT_CONFIG_KEY_0='safe.directory'
$env:GIT_CONFIG_VALUE_0='D:/code/Github/airk'
$env:AIRK_PRIVATE_CENSUS_BASE=(Get-Content artifacts/v190/private-fixture-base.txt -Raw).Trim()
if(-not (Test-Path -LiteralPath $env:AIRK_PRIVATE_CENSUS_BASE -PathType Container)){throw 'Private fixture base absent'}
$fixtureOwner=(Get-Acl -LiteralPath $env:AIRK_PRIVATE_CENSUS_BASE).GetOwner([Security.Principal.SecurityIdentifier]).Value
if($fixtureOwner -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value){throw 'Private fixture base has different owner'}
if(-not (Get-Command cargo-nextest -ErrorAction Stop)){throw 'Pinned nextest absent'}
function Run-Step([string]$Name,[string]$Tool,[string[]]$Arguments){
 $Name | Set-Content artifacts/v190/p6d-focused-stage.txt
 & $Tool @Arguments *> "artifacts/v190/p6d-$Name.log"
 if($LASTEXITCODE){throw "$Name failed with $LASTEXITCODE"}
 Write-Output "$Name passed"
}
Run-Step 'kernel-matrix' 'cargo' @('test','-p','airk-kernel','--all-features','--locked','--test','service_enrichment','--','--test-threads=1')
Run-Step 'protocol' 'cargo' @('test','-p','airk-cli','--all-features','--locked','--lib','project_protocol::helper_admission_tests::source_yield_control_requires_an_explicit_compatible_private_frame','--','--exact')
Run-Step 'debug-pair' 'cargo' @('build','-p','airk-cli','--all-features','--locked','--bin','airk','--bin','airk-embed')
'complete' | Set-Content artifacts/v190/p6d-focused-stage.txt
