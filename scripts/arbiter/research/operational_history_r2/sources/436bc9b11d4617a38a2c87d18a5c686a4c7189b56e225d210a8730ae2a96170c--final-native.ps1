# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference='Stop'
Set-Location 'D:/code/Github/airk'
$env:AIRK_PRIVATE_CENSUS_BASE=Get-Content 'artifacts/release-1.8.7-review/private-base.txt'
$env:PATH="$env:USERPROFILE/.cargo/bin;C:/Program Files/Git/cmd;"+$env:PATH
$env:CARGO_BUILD_JOBS='4'
& cargo fmt --all --check *> 'artifacts/release-1.8.7-review/format-final.log'
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings *> 'artifacts/release-1.8.7-review/clippy-final.log'
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo test --workspace --all-features --locked --offline --no-fail-fast -- --test-threads=2 *> 'artifacts/release-1.8.7-review/workspace-complete.log'
$airkExit=$LASTEXITCODE
@{exit_code=$airkExit;finished=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content 'artifacts/release-1.8.7-review/workspace-complete-result.json'
exit $airkExit
