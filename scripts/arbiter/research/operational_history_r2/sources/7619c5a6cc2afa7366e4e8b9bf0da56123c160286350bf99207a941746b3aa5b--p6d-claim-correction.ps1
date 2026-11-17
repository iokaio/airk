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
if($fixtureOwner -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value){throw 'Private fixture base owner mismatch'}
if(-not (Test-Path -LiteralPath artifacts/tools/nextest/cargo-nextest.exe -PathType Leaf)){throw 'Pinned nextest absent'}
function Run-Step([string]$Name,[string]$Tool,[string[]]$Arguments){
 $Name | Set-Content artifacts/v190/p6d-claim-correction-stage.txt
 & $Tool @Arguments *> "artifacts/v190/p6d-claim-$Name.log"
 if($LASTEXITCODE){throw "$Name failed with $LASTEXITCODE"}
 Write-Output "$Name passed"
}
Run-Step 'format' 'cargo' @('fmt','--all','--check')
Run-Step 'store-background-final' 'cargo' @('nextest','run','-p','airk-store','--all-features','--locked','--profile','ci','-E','test(service::registry::background::tests::)')
Run-Step 'workspace-clippy' 'cargo' @('clippy','--workspace','--all-targets','--all-features','--locked','--','-D','warnings')
Run-Step 'kernel-matrix' 'cargo' @('test','-p','airk-kernel','--all-features','--locked','--test','service_enrichment','--','--test-threads=1')
Run-Step 'release-pair' 'python' @('scripts/build-service-runtime.py','--destination','artifacts/v190/p6d-claim-qualified-runtime','--profile','release')
'complete' | Set-Content artifacts/v190/p6d-claim-correction-stage.txt
