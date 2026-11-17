# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$base = 'D:/code/Github/airk/artifacts/local/one-airk-1.8.14'
$privateBase = (Get-Content "$base/user-private-fixture-base.json" -Raw | ConvertFrom-Json).path
$env:TEMP = $privateBase
$env:TMP = $privateBase
$env:AIRK_PRIVATE_CENSUS_BASE = $privateBase
$env:GIT_CONFIG_COUNT='1'
$env:GIT_CONFIG_KEY_0='safe.directory'
$env:GIT_CONFIG_VALUE_0='D:/code/Github/airk'
$log = "$base/python-full-final-r12.log"
$receipt = "$base/python-full-final-r12.json"
if ((Test-Path $log) -or (Test-Path $receipt)) { throw 'Evidence exists' }
$timer = [Diagnostics.Stopwatch]::StartNew()
python -m unittest discover -s scripts/tests -v *> $log
$code = $LASTEXITCODE
@{command='python -m unittest discover -s scripts/tests -v'; execution='normal user; protected fixture base'; elapsed_seconds=$timer.Elapsed.TotalSeconds; exit_code=$code} | ConvertTo-Json | Set-Content $receipt
Get-Content $log -Tail 30
exit $code







