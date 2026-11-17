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
 $Name | Set-Content artifacts/v190/p6e-focused-stage.txt
 & $Tool @Arguments *> "artifacts/v190/p6e-$Name.log"
 if($LASTEXITCODE){throw "$Name failed with $LASTEXITCODE"}
 Write-Output "$Name passed"
}
Run-Step 'format' 'cargo' @('fmt','--all','--check')
Run-Step 'mcp-remote-focused' 'cargo' @('test','-p','airk-mcp','--all-features','--locked','--lib','remote::tests::','--','--test-threads=1')
Run-Step 'cli-pipe-focused' 'cargo' @('test','-p','airk-cli','--all-features','--locked','--test','mcp','mcp_pipe_','--','--test-threads=1')
'complete' | Set-Content artifacts/v190/p6e-focused-stage.txt
