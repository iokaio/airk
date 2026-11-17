# SPDX-License-Identifier: Apache-2.0
$env:PATH='C:\Users\tdotn\.cargo\bin;D:\code\Github\airk\artifacts\tools\node-v24.15.0-win-x64;' + $env:PATH
$env:PYTHONDONTWRITEBYTECODE='1'
$env:PYTHONIOENCODING='utf-8'
$env:TEMP='C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\temp'
$env:TMP=$env:TEMP
$env:TMPDIR=$env:TEMP
$env:CARGO_INCREMENTAL='1'
$env:CARGO_TARGET_DIR='D:\code\Github\airk\target'
$env:AIRK_BUNDLED_MODEL_DIR='D:/code/Github/airk/artifacts/advanced-split/bundled-model-v3'
$ortEnvironment = Get-Content artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json -Raw | ConvertFrom-Json
$ortEnvironment.environment.psobject.Properties | ForEach-Object { [Environment]::SetEnvironmentVariable($_.Name,$_.Value,'Process') }