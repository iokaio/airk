# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location D:/code/Github/airk
$env:PATH = 'D:/code/Github/airk/artifacts/tools/nextest;C:/Users/tdotn/.cargo/bin;' + $env:PATH
$env:CARGO_BUILD_JOBS = '4'
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
$env:AIRK_PRIVATE_CENSUS_BASE = (Get-Content artifacts/v190/private-fixture-base.txt -Raw).Trim()
function Run-Step([string]$Name, [string]$Tool, [string[]]$Arguments) {
    $Name | Set-Content artifacts/v190/p6c-qualification-stage.txt
    & $Tool @Arguments *> "artifacts/v190/p6c-$Name.log"
    if ($LASTEXITCODE) { throw "$Name failed with $LASTEXITCODE" }
    Write-Output "$Name passed"
}
Run-Step 'format' 'cargo' @('fmt','--all','--check')
Run-Step 'workspace' 'cargo' @('nextest','run','--workspace','--all-features','--locked','--profile','ci')
Run-Step 'doctests' 'cargo' @('test','--doc','--workspace','--locked')
Run-Step 'release-build' 'python' @('scripts/build-service-runtime.py','--destination','artifacts/v190/p6c-qualified-runtime','--profile','release')
$build = Get-Content artifacts/v190/p6c-qualified-runtime/airk-build.json -Raw | ConvertFrom-Json
if ($build.source.sha256 -ne '28f6f23ab1860db27b21c6cef252525b5f7c668cd6e4aeb037049221fe55ef86' -or $build.source.files -ne 789) { throw 'Qualified source differs from frozen P6c inputs' }
'complete' | Set-Content artifacts/v190/p6c-qualification-stage.txt
