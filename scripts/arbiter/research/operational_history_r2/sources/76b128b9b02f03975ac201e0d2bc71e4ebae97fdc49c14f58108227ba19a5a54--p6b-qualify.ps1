# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Set-Location D:/code/Github/airk
$env:PATH = 'D:/code/Github/airk/artifacts/tools/nextest;C:/Users/tdotn/.cargo/bin;' + $env:PATH
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
function Check-Step([string]$Name, [string]$Tool, [string[]]$Arguments) {
    $Name | Set-Content artifacts/v190/p6b-qualification-stage.txt
    & $Tool @Arguments *> "artifacts/v190/p6b-$Name.log"
    if ($LASTEXITCODE -ne 0) { throw "$Name failed: $LASTEXITCODE" }
    Write-Output "$Name passed"
}
# Format and workspace Clippy already passed in the first invocation.
Check-Step 'workspace' 'cargo' @('nextest','run','--workspace','--all-features','--locked','--profile','ci')
Check-Step 'doctests' 'cargo' @('test','--doc','--workspace','--locked')
Check-Step 'release-build' 'python' @('scripts/build-service-runtime.py','--destination','artifacts/v190/p6b-qualified-runtime','--profile','release')
'complete' | Set-Content artifacts/v190/p6b-qualification-stage.txt
