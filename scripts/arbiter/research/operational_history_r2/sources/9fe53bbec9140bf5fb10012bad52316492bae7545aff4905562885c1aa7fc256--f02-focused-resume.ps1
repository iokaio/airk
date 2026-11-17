# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$env:CARGO_BUILD_JOBS='4'
$env:CARGO_NET_OFFLINE='true'
$env:AIRK_BUNDLED_MODEL_DIR='D:/code/Github/airk/target/local-vector-model'
$env:AIRK_BUNDLED_RUNTIME_DIR='D:/code/Github/airk/target/local-onnx-runtime'
$cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
$checks=@(
    @{log='scalar-corruption-corrected';args=@('-p','airk-kernel','--lib','current::indexing::tests::scalar_summary_preserves_policy_semantic_and_discovery_corruption_precedence','--','--exact')},
    @{log='scalar-adjacent';args=@('-p','airk-kernel','--test','current_runtime','scheduled_base_is_readable_before_restart_resumes_semantic_work','--','--exact')},
    @{log='list-size';args=@('-p','airk-kernel','--lib','discovery::tests::inventory_json_size_matches_independent_serializer_for_controls_and_unicode','--','--exact')},
    @{log='list-discovery';args=@('-p','airk-kernel','--test','integration','discovery::')},
    @{log='help';args=@('-p','airk-mcp','--lib','help::tests::')},
    @{log='catalog-local';args=@('-p','airk-mcp','--lib','local::tests::immutable_catalog_preserves_exact_definitions_and_isolates_caller_mutation','--','--exact')},
    @{log='catalog-controls';args=@('-p','airk-mcp','--lib','local::tests::every_tool_advertises_response_controls_and_full_mode_guidance','--','--exact')},
    @{log='catalog-unified';args=@('-p','airk-mcp','--lib','unified::tests::')}
)
foreach($check in $checks){
    $arguments=@('test','--all-features','--locked','--offline')+$check.args
    $log=Join-Path 'artifacts/optimizations' ('f02-'+$check.log+'.log')
    Write-Output ('START '+$check.log+' '+[DateTime]::UtcNow.ToString('o'))
    & $cargo @arguments *> $log
    $checkExit=$LASTEXITCODE
    Get-Content -LiteralPath $log -Tail 12
    Write-Output ('EXIT '+$check.log+' '+$checkExit+' '+[DateTime]::UtcNow.ToString('o'))
    if($checkExit -ne 0){exit $checkExit}
}
