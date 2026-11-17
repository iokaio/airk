# SPDX-License-Identifier: Apache-2.0
param([switch]$SkipChecks)
if (-not $SkipChecks) { throw 'SkipChecks was not forwarded' }
Write-Output ('Build record: ' + (Join-Path $PSScriptRoot '../artifacts/local/run'))
