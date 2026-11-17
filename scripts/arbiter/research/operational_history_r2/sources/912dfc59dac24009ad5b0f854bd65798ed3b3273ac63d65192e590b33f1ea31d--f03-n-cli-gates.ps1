# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$env:CARGO_BUILD_JOBS='4'
$env:CARGO_NET_OFFLINE='true'
$env:AIRK_BUNDLED_MODEL_DIR='D:/code/Github/airk/target/local-vector-model'
$env:AIRK_BUNDLED_RUNTIME_DIR='D:/code/Github/airk/target/local-onnx-runtime'
$cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
& $cargo fmt --all --check *> artifacts/optimizations/f03-fmt.log
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
Write-Output ('START clippy '+[DateTime]::UtcNow.ToString('o'))
& $cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings *> artifacts/optimizations/f03-clippy.log
$gateExit=$LASTEXITCODE
Get-Content artifacts/optimizations/f03-clippy.log -Tail 36
Write-Output ('EXIT clippy '+$gateExit+' '+[DateTime]::UtcNow.ToString('o'))
if($gateExit -ne 0){exit $gateExit}
Write-Output ('START N-CLI '+[DateTime]::UtcNow.ToString('o'))
& $cargo test -p airk-cli -p airk-mcp --all-features --locked --offline *> artifacts/optimizations/f03-n-cli.log
$gateExit=$LASTEXITCODE
Get-Content artifacts/optimizations/f03-n-cli.log -Tail 24
Write-Output ('EXIT N-CLI '+$gateExit+' '+[DateTime]::UtcNow.ToString('o'))
exit $gateExit
