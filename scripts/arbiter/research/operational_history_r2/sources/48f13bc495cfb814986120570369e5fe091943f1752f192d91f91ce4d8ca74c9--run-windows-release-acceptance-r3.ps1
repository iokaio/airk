# SPDX-License-Identifier: Apache-2.0
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
        incremental='off'; elapsed_seconds=$timer.Elapsed.TotalSeconds; exit_code=$code
        execution='normal user; existing protected fixture base; retained Cargo timings'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $receipt
    Write-Output "$Name exit=$code elapsed=$($timer.Elapsed.TotalSeconds)"
    if ($code -ne 0) {
        Get-Content -LiteralPath $log -Tail 45
        exit $code
    }
}

$env:CARGO_TARGET_DIR = $target
$runtime = "$evidence/windows-release-r3/runtime"
$main = Join-Path $runtime 'airk.exe'
$approved = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $runtime 'airk-install.json')).Hash.ToLowerInvariant()
Invoke-Gate 'windows-release-model-clients-r4' 'python' @('scripts/check-service-clients.py','--runtime-dir',$runtime,'--files','16','--model','--report',"$evidence/windows-release-model-clients-result-r4.json")
Invoke-Gate 'windows-release-retrieval-r1' 'python' @('scripts/check-retrieval-quality.py','--binary',$main,'--output',"$evidence/windows-release-retrieval-result-r1.json")
Invoke-Gate 'windows-release-install-r1' 'python' @('scripts/check-service-install.py','--runtime',$runtime)
Invoke-Gate 'windows-release-managed-readonly-r1' 'python' @('scripts/check-managed-runtime.py','--runtime',$runtime)
Invoke-Gate 'windows-release-issuer-rotation-r1' 'python' @('scripts/check-issuer-rotation.py','--runtime-dir',$runtime,'--output',"$evidence/windows-release-issuer-rotation-private-r1")
Invoke-Gate 'windows-release-port-relocation-r1' 'python' @('scripts/check-http-port-relocation.py','--runtime',$runtime,'--approved-sha256',$approved,'--report',"$evidence/windows-release-port-relocation-result-r1.json")
$env:AIRK_TEST_BINARY = $main
$env:AIRK_NATIVE_TEST_BINARY = $main
$env:AIRK_TEST_F09_QUALIFICATION = '1'
Invoke-Gate 'windows-release-extension-compile-r1' 'npm.cmd' @('--prefix','packages/vscode','run','compile')
$testFiles = @(Get-ChildItem -LiteralPath 'packages/vscode/out/test' -Filter '*.test.js' | Sort-Object Name | ForEach-Object { $_.FullName })
if ($testFiles.Count -eq 0) { throw 'Compiled extension tests missing' }
Invoke-Gate 'windows-release-extension-native-r1' 'node' (@('--test','--test-concurrency=2') + $testFiles)
Invoke-Gate 'windows-release-package-native-r1' 'python' @('scripts/package-native.py','--target','x86_64-pc-windows-msvc','--runtime-dir',$runtime,'--output',"$evidence/windows-release-native-r1")
Invoke-Gate 'windows-release-package-vsix-r1' 'python' @('scripts/package-extension.py','--target','x86_64-pc-windows-msvc','--native-dir',"$evidence/windows-release-native-r1/publish",'--output',"$evidence/windows-release-vsix-r1")




