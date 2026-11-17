# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
$location=Get-Content -Raw artifacts/optimizations/f03-runtime-location.json | ConvertFrom-Json
$env:CARGO_BUILD_JOBS='4'
$env:CARGO_NET_OFFLINE='true'
$env:AIRK_BUNDLED_MODEL_DIR='D:/code/Github/airk/target/local-vector-model'
$env:AIRK_BUNDLED_RUNTIME_DIR='D:/code/Github/airk/target/local-onnx-runtime'
$env:PATH='C:/Users/tdotn/.cargo/bin;'+$env:PATH
if(Test-Path -LiteralPath $location.runtime){throw 'fresh runtime destination unexpectedly exists'}
Write-Output ('START fresh-pair '+[DateTime]::UtcNow.ToString('o'))
python scripts/build-service-runtime.py --profile release --destination $location.runtime *> artifacts/optimizations/f03-runtime-build.log
$buildExit=$LASTEXITCODE
Get-Content artifacts/optimizations/f03-runtime-build.log -Tail 28
Write-Output ('EXIT fresh-pair '+$buildExit+' '+[DateTime]::UtcNow.ToString('o'))
exit $buildExit
