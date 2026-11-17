# SPDX-License-Identifier: Apache-2.0
param([switch]$SkipChecks)
$cargo = (Get-Command cargo -CommandType Application -ErrorAction Stop).Source
$rustup = (Get-Command rustup -CommandType Application -ErrorAction Stop).Source
& $cargo --version
if ($LASTEXITCODE) { throw 'Cargo could not run' }
& $rustup --version
if ($LASTEXITCODE) { throw 'Rustup could not run' }
if ($SkipChecks.IsPresent -ne ($env:AIRK_WRAPPER_EXPECT_SKIP -eq 'true')) { throw 'Wrong check mode' }
throw 'AIRK_WRAPPER_PREFLIGHT_VERIFIED'
