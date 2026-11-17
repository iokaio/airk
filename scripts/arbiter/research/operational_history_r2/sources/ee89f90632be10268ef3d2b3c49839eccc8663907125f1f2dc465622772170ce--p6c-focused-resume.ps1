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
Run-Step 'mcp-projection-corrected' @('test','-p','airk-mcp','--lib','--all-features','--locked','verification_tests::search_execution_survives_verification_results_and_all_reply_projections','--','--exact','--test-threads=1')
Run-Step 'kernel-workbench' @('test','-p','airk-kernel','--lib','--all-features','--locked','workbench','--','--test-threads=2')
Run-Step 'native-outage-parity' @('test','-p','airk-cli','--test','helper_outage_reads','--all-features','--locked','--','--test-threads=1')
'complete' | Set-Content artifacts/v190/p6c-focused-stage.txt
