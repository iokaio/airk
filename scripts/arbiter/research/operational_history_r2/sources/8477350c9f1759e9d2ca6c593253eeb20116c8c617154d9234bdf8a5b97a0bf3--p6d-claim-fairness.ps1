# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location D:/code/Github/airk
$env:PATH = 'D:/code/Github/airk/artifacts/tools/nextest;C:/Users/tdotn/.cargo/bin;' + $env:PATH
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
$env:CARGO_BUILD_JOBS = '4'
$env:AIRK_PRIVATE_CENSUS_BASE = (Get-Content artifacts/v190/private-fixture-base.txt -Raw).Trim()
$candidate = 'artifacts/v190/p6d-claim-qualified-runtime'
$control = 'artifacts/v190/p6a-p5-control-copy'
if (Get-Process cargo,rustc -ErrorAction SilentlyContinue) { throw 'Compiler still running; admission refused' }
function Run-Step([string]$Name, [string]$Tool, [string[]]$Arguments) {
    $Name | Set-Content artifacts/v190/p6d-final-campaign-stage.txt
    & $Tool @Arguments *> "artifacts/v190/p6d-final-$Name.log"
    $code = $LASTEXITCODE
    if ($code -ne 0) { throw "$Name failed with exit $code; retain its exact log" }
    Write-Output "$Name passed"
}
$build = Get-Content "$candidate/airk-build.json" -Raw | ConvertFrom-Json
if ($build.source.sha256 -ne 'a87a1bc8377699c7ae6da76f2aa7dbb23fe3386816bf5edb87af0546b2d6cbf7') { throw 'Candidate source identity differs' }
$before = (Get-FileHash "$candidate/airk.exe" -Algorithm SHA256).Hash
Run-Step 'claim-fairness' 'python' @('scripts/check-service-source-fairness.py','--runtime',$candidate,'--output','artifacts/v190/p6d-source-fairness-claim.json','--files','384')
Run-Step 'claim-loaded-gates' 'python' @('artifacts/v190/p6d-claim-evaluate-backlog-control.py')
'fairness-complete' | Set-Content artifacts/v190/p6d-final-campaign-stage.txt
