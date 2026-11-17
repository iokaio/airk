# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location D:/code/Github/airk
$env:PATH = 'D:/code/Github/airk/artifacts/tools/nextest;C:/Users/tdotn/.cargo/bin;' + $env:PATH
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
$env:CARGO_BUILD_JOBS = '4'
$env:AIRK_PRIVATE_CENSUS_BASE = (Get-Content artifacts/v190/private-fixture-base.txt -Raw).Trim()
$candidate = 'artifacts/v190/p6e-qualified-runtime'
$control = 'artifacts/v190/p6a-p5-control-copy'
if (Get-Process cargo,rustc -ErrorAction SilentlyContinue) { throw 'Compiler still running; admission refused' }
function Run-Step([string]$Name, [string]$Tool, [string[]]$Arguments) {
    $Name | Set-Content artifacts/v190/p6e-final-campaign-stage.txt
    & $Tool @Arguments *> "artifacts/v190/p6e-final-$Name.log"
    $code = $LASTEXITCODE
    if ($code -ne 0) { throw "$Name failed with exit $code; retain its exact log" }
    Write-Output "$Name passed"
}
$build = Get-Content "$candidate/airk-build.json" -Raw | ConvertFrom-Json
if ($build.source.sha256 -ne '62db764503f4f76ede4cf512f30211aff0e3dd95aa8476062a739a124cbc4b59') { throw 'Candidate source identity differs' }
$before = (Get-FileHash "$candidate/airk.exe" -Algorithm SHA256).Hash
Run-Step 'smokes' 'pwsh' @('-NoProfile','-File','scripts/ci-local.ps1','-SkipBuild','-RuntimeDir',$candidate,'-Repeat','1')
Run-Step 'pool-smoke' 'python' @('scripts/check-service-pool.py','--runtime',$candidate)
if (Get-Process cargo,rustc -ErrorAction SilentlyContinue) { throw 'Compiler still running before measurement' }
$hostProof = @{logical_processors=[Environment]::ProcessorCount; affinity=[Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity.ToInt64(); build_source=$build.source; binary_sha256=$before; started_utc=[DateTime]::UtcNow.ToString('o')}
$hostProof | ConvertTo-Json | Set-Content artifacts/v190/p6e-measurement-host.json
Run-Step 'off' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$candidate,'--output','artifacts/v190/p6e-off-mcp','--label','candidate','--samples','30','--concurrency','1','--verification','off')
Run-Step 'control-off' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$control,'--output','artifacts/v190/p6e-control-off-mcp','--label','baseline','--samples','30','--concurrency','1','--verification','off')
Run-Step 'report' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$candidate,'--output','artifacts/v190/p6e-report-mcp','--label','candidate','--samples','30','--concurrency','1')
Run-Step 'control-report' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$control,'--output','artifacts/v190/p6e-control-report-mcp','--label','baseline','--samples','30','--concurrency','1')
if ((Get-FileHash "$candidate/airk.exe" -Algorithm SHA256).Hash -ne $before) { throw 'Candidate binary changed during campaign' }
Run-Step 'gates' 'python' @('artifacts/v190/p6e-evaluate-gates.py')
'complete' | Set-Content artifacts/v190/p6e-final-campaign-stage.txt
