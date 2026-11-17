# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$env:CARGO_BUILD_JOBS='4'
$env:CARGO_NET_OFFLINE='true'
$env:AIRK_BUNDLED_MODEL_DIR='D:/code/Github/airk/target/local-vector-model'
$env:AIRK_BUNDLED_RUNTIME_DIR='D:/code/Github/airk/target/local-onnx-runtime'
$cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
& $cargo fmt --all *> artifacts/optimizations/f02-fmt-corrected-apply.log
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& $cargo fmt --all --check *> artifacts/optimizations/f02-fmt.log
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
Write-Output ('START clippy '+[DateTime]::UtcNow.ToString('o'))
& $cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings *> artifacts/optimizations/f02-clippy-corrected.log
$gateExit=$LASTEXITCODE
Get-Content artifacts/optimizations/f02-clippy-corrected.log -Tail 20
Write-Output ('EXIT clippy '+$gateExit+' '+[DateTime]::UtcNow.ToString('o'))
if($gateExit -ne 0){exit $gateExit}
Write-Output ('START workspace '+[DateTime]::UtcNow.ToString('o'))
& $cargo test --workspace --all-features --locked --offline *> artifacts/optimizations/f02-workspace.log
$gateExit=$LASTEXITCODE
Get-Content artifacts/optimizations/f02-workspace.log -Tail 28
Write-Output ('EXIT workspace '+$gateExit+' '+[DateTime]::UtcNow.ToString('o'))
exit $gateExit
