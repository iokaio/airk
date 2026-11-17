# SPDX-License-Identifier: Apache-2.0
# Prepared only. Run after release acceptance's compile/full extension suite passes.
# Additional CI qualification gates; no Cargo builds or duplicate full extension suite.
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
        execution='normal user; existing protected fixture base; retained Cargo timings'
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
$extensionReceipt = Get-Content -LiteralPath "$evidence/windows-release-extension-native-r1.json" -Raw | ConvertFrom-Json
if ($extensionReceipt.exit_code -ne 0) { throw 'Complete shipping-runtime extension suite must pass before extra smoke repetitions' }
$env:AIRK_TEST_BINARY = $main
$env:AIRK_NATIVE_TEST_BINARY = $main
$env:AIRK_TEST_F09_QUALIFICATION = '1'
$env:AIRK_QUALIFICATION = 'true'
Invoke-Gate 'windows-release-transformer-r1' 'python' @('scripts/check-transformer-model.py','--binary',$main)
Invoke-Gate 'windows-release-code-benchmark-r1' 'python' @('scripts/benchmark-code.py','--binary',$main,'--files','64','--output',"$evidence/windows-release-code-benchmark-result-r1.json")
Invoke-Gate 'windows-release-document-tables-r1' 'python' @('scripts/check-document-tables.py','--binary',$main)
Invoke-Gate 'windows-release-storage-lifecycle-r1' 'python' @('scripts/check-storage-lifecycle.py','--binary',$main)
Invoke-Gate 'windows-release-usage-inputs-r1' 'python' @('scripts/check-usage-inputs.py','--binary',$main)
Invoke-Gate 'windows-release-publication-faults-r1' 'python' @('scripts/check-publication-faults.py','--binary',$main,'--attempts','3','--output',"$evidence/windows-release-publication-faults-result-r1.json")
foreach ($pass in 2,3) {
    Invoke-Gate "windows-release-mcp-smoke-pass$pass-r1" 'python' @('scripts/ci-metrics.py','run','--name',"mcp-smoke-$pass",'--','node','--test','packages/vscode/out/test/nativeSmoke.test.js')
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $main).Hash.ToLowerInvariant() -ne $executableHash -or
    (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $runtime 'airk-install.json')).Hash.ToLowerInvariant() -ne $manifestHash) {
    throw 'Shipping runtime bytes changed during additional qualification'
}


