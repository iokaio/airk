# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Force local builds of both AIRK products on Windows (split plan section 10.5.1, U21, U37).

.DESCRIPTION
Runs scripts/build-local-service-installer.ps1 -ForceBuild, then scripts/build-local-vsix.ps1
-ForceBuild, and stops at the first failure with that step's exit code. Each step prints its
output path and receipt. Nothing is installed, started, trusted or published; installation is a
separate, explicit action.
#>
param(
    [switch] $ForceBuild,
    [switch] $Unsigned,
    [string] $Iscc,
    [ValidateSet('stable', 'preview')] [string] $Channel = 'stable'
)
$ErrorActionPreference = 'Stop'
if (-not $ForceBuild) { throw 'Pass -ForceBuild to build from this checkout.' }
$service = @{ ForceBuild = $true }
if ($Unsigned) { $service.Unsigned = $true }
if ($Iscc) { $service.Iscc = $Iscc }
& "$PSScriptRoot/build-local-service-installer.ps1" @service
if ($LASTEXITCODE -ne 0) {
    Write-Error "AIRK Service installer build failed ($LASTEXITCODE); the VSIX was not built." -ErrorAction Continue
    exit $LASTEXITCODE
}
& "$PSScriptRoot/build-local-vsix.ps1" -ForceBuild -Channel $Channel
if ($LASTEXITCODE -ne 0) {
    Write-Error "AIRK VS Code Extension VSIX build failed ($LASTEXITCODE)." -ErrorAction Continue
    exit $LASTEXITCODE
}
exit 0
