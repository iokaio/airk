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
$env:CARGO_INCREMENTAL = '1'
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
        incremental='on'; elapsed_seconds=$timer.Elapsed.TotalSeconds; exit_code=$code
        execution='normal user; existing protected fixture base; retained Cargo timings'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $receipt
    Write-Output "$Name exit=$code elapsed=$($timer.Elapsed.TotalSeconds)"
    if ($code -ne 0) {
        Get-Content -LiteralPath $log -Tail 45
        exit $code
    }
}

# Reuse completed unchanged-source format-r1 check.
Invoke-Gate 'legacy-cli-integration-clippy-r2' 'python' @('scripts/dev-check.py','-p','airk-cli','-p','airk-mcp','--all-features','--skip-fmt','--target-dir',$target,'--incremental','on','--cargo',$cargo)
$failures = @()
foreach ($line in Select-String -Path "$evidence/cli-combined-correction-tests-r1.log" -Pattern '^\s+FAIL \[[^\]]+\] \([^)]*\) airk-cli::\S+ (\S+)$') {
    $name = $line.Matches[0].Groups[1].Value
    if ($name -notmatch '^[A-Za-z0-9_:]+$') { throw 'Unexpected test selector' }
    $failures += $name
}
$failures = @($failures | Sort-Object -Unique)
if ($failures.Count -ne 9) { throw "Expected complete nine-case failed roster, got $($failures.Count)" }
$selectors = @('test(legacy)', 'test(adoption)', 'test(adopted_authority)', 'test(changed_origin_rolls_back)', 'test(adoption_layout)', 'test(superseded)', 'test(lookup_http)', 'test(http_lookup)', 'test(selected_public_routes_preserve_original_attempt_and_lifetime)', 'test(http_gateway::tests)', 'test(http_endpoint::tests)')
$selectors += @($failures | ForEach-Object { "test($_)" })
Invoke-Gate 'legacy-cli-integration-tests-r2' $cargo @('nextest','run','--locked','-p','airk-cli','-p','airk-core','-p','airk-store','-p','airk-kernel','-p','airk-mcp','--all-features','--profile','ci','--no-fail-fast','--timings','--target-dir',$target,'-E',($selectors -join ' | '))
Get-Content "$evidence/legacy-cli-integration-tests-r2.log" -Tail 10

