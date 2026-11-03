# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
& python (Join-Path $PSScriptRoot 'bootstrap.py') @args
exit $LASTEXITCODE
