# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Force a local Windows AIRK Service installer build (split plan section 10.5.1, U37).

.DESCRIPTION
Delegates to scripts/build-local-service-installer.py. -ForceBuild is required. Without the
local signing adapter the build stops; -Unsigned produces a labeled packaging check that setup
refuses to install. Nothing is installed, started, trusted or published.
#>
param(
    [switch] $ForceBuild,
    [switch] $Unsigned,
    [string] $Iscc
)
$ErrorActionPreference = 'Stop'
if (-not $ForceBuild) { throw 'Pass -ForceBuild to build from this checkout.' }
$arguments = @("$PSScriptRoot/build-local-service-installer.py", '--force-build')
if ($Unsigned) { $arguments += '--unsigned' }
if ($Iscc) { $arguments += @('--iscc', $Iscc) }
& python @arguments
exit $LASTEXITCODE
