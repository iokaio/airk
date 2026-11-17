# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location D:/code/Github/airk
$env:PATH = 'D:/code/Github/airk/artifacts/tools/nextest;C:/Users/tdotn/.cargo/bin;' + $env:PATH
$env:CARGO_BUILD_JOBS = '4'
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
$env:AIRK_PRIVATE_CENSUS_BASE = (Get-Content artifacts/v190/private-fixture-base.txt -Raw).Trim()
function Run-Step([string]$Name, [string[]]$Arguments) {
    $Name | Set-Content artifacts/v190/p6c-focused-stage.txt
    & cargo @Arguments *> "artifacts/v190/p6c-$Name.log"
    if ($LASTEXITCODE) { throw "$Name failed with $LASTEXITCODE" }
    Write-Output "$Name passed"
}
Run-Step 'kernel-clippy' @('clippy','-p','airk-kernel','--all-targets','--all-features','--locked','--','-D','warnings')
Run-Step 'core-execution' @('test','-p','airk-core','--lib','--all-features','--locked','search_execution','--','--test-threads=2')
Run-Step 'core-guidance' @('test','-p','airk-core','--lib','--all-features','--locked','guidance','--','--test-threads=2')
Run-Step 'mcp-lib' @('nextest','run','-p','airk-mcp','--lib','--all-features','--locked','--profile','ci')
Run-Step 'kernel-workbench' @('test','-p','airk-kernel','--lib','--all-features','--locked','workbench','--','--test-threads=2')
Run-Step 'native-outage-parity' @('test','-p','airk-cli','--test','helper_outage_reads','--all-features','--locked','--','--test-threads=1')
'complete' | Set-Content artifacts/v190/p6c-focused-stage.txt
