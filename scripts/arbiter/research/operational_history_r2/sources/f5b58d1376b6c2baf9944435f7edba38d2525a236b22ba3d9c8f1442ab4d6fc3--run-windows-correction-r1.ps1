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

Invoke-Gate 'windows-correction-format-r1' $cargo @('fmt','--all')
Invoke-Gate 'windows-correction-workspace-clippy-r1' 'python' @('scripts/dev-check.py','--workspace','--all-features','--skip-fmt','--target-dir',$target,'--incremental','on','--cargo',$cargo)
Invoke-Gate 'windows-correction-lightweight-r1' $cargo @('check','--locked','-p','airk-cli','--no-default-features','--all-targets','--timings','--target-dir',$target)
$selection = 'test(runtime_install::staging::) | test(runtime_install::payload_repair::) | test(runtime_install::checkpoint::) | test(runtime_pair::) | test(runtime_contract::) | test(source_watch::) | test(embedded_default_setup_is_shared_without_extraction_and_preserves_custom_inputs) | test(native_http_commands_preserve_private_selection_and_revocation) | test(every_public_command_has_a_test_and_prints_help) | test(legacy_runtime_commands_require_an_existing_owned_installation) | test(default_search_uses_vectors_and_live_mcp_connections_observe_panel_opt_out) | test(durable_key_reads_match_cli_and_mcp_and_reject_filter_combinations) | test(setup_outputs_client_config_and_mcp_exposes_specialized_tools) | test(held_quota_ledger_preserves_same_read_hint_through_native_cli_and_mcp) | test(profile_families::opted_file_family_report_keeps_full_native_cli_console_mcp_and_input_catalog_parity)'
Invoke-Gate 'windows-correction-tests-r1' $cargo @('nextest','run','--locked','--workspace','--all-features','--profile','ci','--no-fail-fast','--timings','--target-dir',$target,'-E',$selection)
