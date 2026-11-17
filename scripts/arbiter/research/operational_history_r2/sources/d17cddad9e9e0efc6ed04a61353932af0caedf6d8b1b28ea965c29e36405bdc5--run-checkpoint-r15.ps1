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

Invoke-Gate 'tls-certificate-fmt-r15' $cargo @('fmt','--all')
Invoke-Gate 'tls-certificate-clippy-r15' 'python' @('scripts/dev-check.py','-p','airk-store','-p','airk-cli','--all-features','--skip-fmt','--target-dir',$target,'--incremental','on','--cargo',$cargo)
Invoke-Gate 'tls-certificate-tests-r15' $cargo @('nextest','run','--locked','-p','airk-store','-p','airk-cli','--all-features','--lib','--profile','ci','--no-fail-fast','--timings','--target-dir',$target,'-E','test(checkpoint) | test(reservation_and_native_preparation_remain_pending_until_exact_once_ready) | test(frozen_finalization_attention_has_separate_cas_and_old_ack_cannot_clear_later_failure) | test(real_claimed_cancellation_disposes_paid_shell_then_publishes_and_acknowledges_fixed_slots)')$env:AIRK_PROCESS_DIAGNOSTICS = '1'
$env:AIRK_NATIVE_DIAGNOSTIC_DIR = Join-Path $privateBase 'native-split-checkpoint-r15'
New-Item -ItemType Directory -Path $env:AIRK_NATIVE_DIAGNOSTIC_DIR -ErrorAction Stop | Out-Null
Invoke-Gate 'split-checkpoint-transport-r15' $cargo @('nextest','run','--locked','-p','airk-cli','--all-features','--test','combined_reads','--profile','ci','--no-fail-fast','--timings','--target-dir',$target,'-E','test(split::)')