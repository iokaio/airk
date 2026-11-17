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

Invoke-Gate 'windows-query-scope-format-r3' $cargo @('fmt','--all')
Invoke-Gate 'windows-query-scope-clippy-r3' 'python' @('scripts/dev-check.py','-p','airk-store','-p','airk-kernel','-p','airk-cli','--all-features','--skip-fmt','--target-dir',$target,'--incremental','on','--cargo',$cargo)
Invoke-Gate 'windows-query-scope-tests-r3' $cargo @('nextest','run','--locked','-p','airk-store','-p','airk-kernel','-p','airk-cli','-p','airk-mcp','--all-features','--profile','ci','--ignore-default-filter','-E','test(installed_query_scope_calls_and_shell_retain_canonical_authority) | test(tools::parity_tests::) | test(tools_help_works_without_a_workspace_and_exposes_live_schemas) | test(every_public_command_has_a_test_and_prints_help) | test(native_http_commands_preserve_private_selection_and_revocation) | test(legacy_discovery_does_not_invent_scope_or_publication) | test(native_and_parse_legacy_discovery_share_the_pure_metadata_body)','--no-fail-fast','--timings','--target-dir',$target)

