# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location D:/code/Github/airk
$env:PATH = 'D:/code/Github/airk/artifacts/tools/nextest;C:/Users/tdotn/.cargo/bin;' + $env:PATH
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
$env:CARGO_BUILD_JOBS = '4'
$env:AIRK_PRIVATE_CENSUS_BASE = (Get-Content artifacts/v190/private-fixture-base.txt -Raw).Trim()
$candidate = 'artifacts/v190/p6b-qualified-runtime'
$control = 'artifacts/v190/p6a-p5-control-copy'
if (Get-Process cargo,rustc -ErrorAction SilentlyContinue) { throw 'Compiler still running; admission refused' }
function Run-Step([string]$Name, [string]$Tool, [string[]]$Arguments) {
    $Name | Set-Content artifacts/v190/p6b-r3-campaign-stage.txt
    & $Tool @Arguments *> "artifacts/v190/p6b-r3-$Name.log"
    $code = $LASTEXITCODE
    if ($code -ne 0) { throw "$Name failed with exit $code; retain its exact log" }
    Write-Output "$Name passed"
}
Run-Step 'cli-lib-tests' 'cargo' @('nextest','run','-p','airk-cli','--lib','--all-features','--locked','--profile','ci')
Run-Step 'cli-clippy' 'cargo' @('clippy','-p','airk-cli','--all-targets','--all-features','--locked','--','-D','warnings')
Run-Step 'release-build' 'python' @('scripts/build-service-runtime.py','--destination',$candidate,'--profile','release')
$build = Get-Content "$candidate/airk-build.json" -Raw | ConvertFrom-Json
if ($build.source.sha256 -ne 'be6e29680ceacc11d056f04dfbb7a6cf63487876d01f6bccba7bd2b217542053') { throw 'Candidate source identity differs' }
$before = (Get-FileHash "$candidate/airk.exe" -Algorithm SHA256).Hash
$env:AIRK_TEST_RUNTIME = (Resolve-Path $candidate).Path
Run-Step 'supervisor-native' 'cargo' @('test','-p','airk-cli','--all-features','--lib','project_supervisor::pool_tests::native_supervisor_repairs_missing_grant_without_restarting_owners','--locked','--','--ignored','--exact','--test-threads=1')
Remove-Item Env:AIRK_TEST_RUNTIME
Run-Step 'smokes' 'pwsh' @('-NoProfile','-File','scripts/ci-local.ps1','-SkipBuild','-RuntimeDir',$candidate,'-Repeat','1')
Run-Step 'pool-smoke' 'python' @('scripts/check-service-pool.py','--runtime',$candidate)
Run-Step 'native-outages' 'python' @('artifacts/v190/p6b-native-outages.py')
if (Get-Process cargo,rustc -ErrorAction SilentlyContinue) { throw 'Compiler still running before measurement' }
$hostProof = @{logical_processors=[Environment]::ProcessorCount; affinity=[Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity.ToInt64(); build_source=$build.source; binary_sha256=$before; started_utc=[DateTime]::UtcNow.ToString('o')}
$hostProof | ConvertTo-Json | Set-Content artifacts/v190/p6b-r3-measurement-host.json
Run-Step 'off' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$candidate,'--output','artifacts/v190/p6b-off-mcp','--label','candidate','--samples','30','--concurrency','1','--verification','off')
Run-Step 'control-off' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$control,'--output','artifacts/v190/p6b-control-off-mcp','--label','baseline','--samples','30','--concurrency','1','--verification','off')
Run-Step 'report' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$candidate,'--output','artifacts/v190/p6b-report-mcp','--label','candidate','--samples','30','--concurrency','1')
Run-Step 'control-report' 'python' @('scripts/benchmark-mcp-investigation.py','owned','--runtime-dir',$control,'--output','artifacts/v190/p6b-control-report-mcp','--label','baseline','--samples','30','--concurrency','1')
if ((Get-FileHash "$candidate/airk.exe" -Algorithm SHA256).Hash -ne $before) { throw 'Candidate binary changed during campaign' }
Run-Step 'gates' 'python' @('artifacts/v190/p6b-evaluate-gates.py')
'complete' | Set-Content artifacts/v190/p6b-r3-campaign-stage.txt
