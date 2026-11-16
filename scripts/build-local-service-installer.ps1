# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Force a local Windows AIRK Service installer build (split plan section 10.5.1, U37).

.DESCRIPTION
Delegates to scripts/build-local-service-installer.py. Use -ForceBuild, or consume an already
verified pair with -RuntimeDir and -AdminInventory (no Cargo/npm). Without the
local signing adapter (scripts/local_signing.py, Azure CLI access to the Artifact Signing
account) the build stops; -Unsigned produces a labeled packaging check that setup
refuses to install. Nothing is installed, started, trusted or published.
#>
param(
    [switch] $ForceBuild,
    [string] $RuntimeDir,
    [string] $AdminInventory,
    [switch] $Unsigned,
    [string] $Iscc
)
$ErrorActionPreference = 'Stop'
if (-not $ForceBuild -and -not $RuntimeDir) { throw 'Pass -ForceBuild or -RuntimeDir with -AdminInventory.' }
if ($ForceBuild -and $RuntimeDir) { throw '-ForceBuild and -RuntimeDir are mutually exclusive.' }
if ([bool]$RuntimeDir -ne [bool]$AdminInventory) { throw '-RuntimeDir and -AdminInventory are required together.' }
$arguments = @("$PSScriptRoot/build-local-service-installer.py")
if ($ForceBuild) { $arguments += '--force-build' }
if ($RuntimeDir) { $arguments += @('--runtime-dir', $RuntimeDir, '--admin-inventory', $AdminInventory) }
if ($Unsigned) { $arguments += '--unsigned' }
if ($Iscc) { $arguments += @('--iscc', $Iscc) }
& python @arguments
exit $LASTEXITCODE
