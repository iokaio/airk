# SPDX-License-Identifier: Apache-2.0
# Prepared only: authentic predecessor adoption and negative rollback preservation.
# Genuine 1.8.12 semantic upgrade belongs to the separate schema1 runner.
$ErrorActionPreference = 'Stop'
$env:PATH = 'C:\Users\tdotn\.cargo\bin;D:\code\Github\airk\artifacts\tools\nextest;' + $env:PATH
$evidence = 'D:/code/Github/airk/artifacts/local/one-airk-1.8.14'
$target = 'D:/code/Github/airk/artifacts/v190/p10-target'
$staticEnv = (Get-Content "$evidence/static-environment-supplement.json" -Raw | ConvertFrom-Json).environment
foreach ($property in $staticEnv.PSObject.Properties) {
    [Environment]::SetEnvironmentVariable($property.Name, [string]$property.Value, 'Process')
}
$privateBase = (Get-Content "$evidence/user-private-fixture-base.json" -Raw | ConvertFrom-Json).path
$env:TEMP = $privateBase
$env:TMP = $privateBase
$env:AIRK_PRIVATE_CENSUS_BASE = $privateBase
$env:AIRK_TEST_MODEL_DIR = 'D:/code/Github/airk/target/local-vector-model/minilm-l6-v2-u8-v1'
$env:CARGO_INCREMENTAL = '0'
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'
$cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'

function Invoke-Gate {
    param([string]$Name, [string]$Executable, [string[]]$Arguments)
    $log = "$evidence/$Name.log"
    $receipt = "$evidence/$Name.json"
    if ((Test-Path -LiteralPath $log) -or (Test-Path -LiteralPath $receipt)) {
        throw "Refusing to overwrite gate evidence: $Name"
    }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    & $Executable @Arguments *> $log
    $code = $LASTEXITCODE
    @{
        executable=$Executable; arguments=$Arguments; target=$target
        runtime_manifest_sha256=$manifestHash; runtime_executable_sha256=$executableHash
        incremental='off'; elapsed_seconds=$timer.Elapsed.TotalSeconds; exit_code=$code
        execution='normal user; exact optimized release runtime; existing protected fixture base; no compilation'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $receipt
    Write-Output "$Name exit=$code elapsed=$($timer.Elapsed.TotalSeconds)"
    if ($code -ne 0) {
        Get-Content -LiteralPath $log -Tail 45
        exit $code
    }
}

$runtime = "$evidence/windows-release-r3/runtime"
$main = Join-Path $runtime 'airk.exe'
$manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $runtime 'airk-install.json')).Hash.ToLowerInvariant()
$executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $main).Hash.ToLowerInvariant()
$env:AIRK_PROCESS_DIAGNOSTICS = '1'
$env:AIRK_NATIVE_DIAGNOSTIC_DIR = Join-Path $privateBase 'release-pre14-adoption-diagnostics-r1'
New-Item -ItemType Directory -Path $env:AIRK_NATIVE_DIAGNOSTIC_DIR -ErrorAction Stop | Out-Null
Invoke-Gate 'windows-release-pre14-adoption-r1' 'python' @('scripts/check-standalone-adoption.py','--runtime',$runtime,'--fixture-report',"$evidence/historical-windows/campaign.json",'--model-js','packages/vscode/out/model.js','--tag','v1.3.1','--tag','v1.0.0-build.7','--output',"$evidence/windows-release-pre14-adoption-result-r1.json")
$env:AIRK_NATIVE_DIAGNOSTIC_DIR = Join-Path $privateBase 'release-adoption-failure-diagnostics-r1'
New-Item -ItemType Directory -Path $env:AIRK_NATIVE_DIAGNOSTIC_DIR -ErrorAction Stop | Out-Null
Invoke-Gate 'windows-release-adoption-failure-r1' 'python' @('scripts/check-standalone-adoption-failure.py','--runtime',$runtime,'--fixture-report',"$evidence/historical-windows/campaign.json",'--tag','v1.0.0-build.7','--output',"$evidence/windows-release-adoption-failure-result-r1.json")
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $main).Hash.ToLowerInvariant() -ne $executableHash -or
    (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $runtime 'airk-install.json')).Hash.ToLowerInvariant() -ne $manifestHash) {
    throw 'Optimized shipping runtime bytes changed during migration qualification'
}


