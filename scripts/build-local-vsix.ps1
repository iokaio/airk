# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Force a local universal AIRK VS Code Extension VSIX build (split plan section 10.5.1, U21, U29, U37).

.DESCRIPTION
Delegates to scripts/build-local-vsix.py. -ForceBuild is required. Runs the locked npm install,
the I2 contract drift check, compilation and UI-only packaging; needs no native build and no
service. Prints the VSIX path and its receipt. Nothing is installed or published.
#>
param(
    [switch] $ForceBuild,
    [ValidateSet('stable', 'preview')] [string] $Channel = 'stable'
)
$ErrorActionPreference = 'Stop'
if (-not $ForceBuild) { throw 'Pass -ForceBuild to build from this checkout.' }
& python "$PSScriptRoot/build-local-vsix.py" --force-build --channel $Channel
exit $LASTEXITCODE
