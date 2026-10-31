# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Build an installation-free AIRK runtime for this platform under builds\<UTC timestamp>-<platform>\.

.DESCRIPTION
For advanced users experimenting with the native engine from a terminal, without VS Code.
Nothing is installed. The output folder contains the runtime and a README.md that explains
how to start each process and set up a headless test environment.

.PARAMETER BuildProfile
Cargo profile: release (default) or debug.
#>
param([ValidateSet('release', 'debug')][string]$BuildProfile = 'release')
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}
foreach ($tool in 'cargo', 'python') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is required on PATH (see CONTRIBUTING.md)." }
}
& python -c "import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)"
if ($LASTEXITCODE -ne 0) { throw 'Python 3.11 or later is required.' }
& python scripts/local-build.py --profile $BuildProfile
if ($LASTEXITCODE -ne 0) { throw "Local build failed with exit code $LASTEXITCODE." }
