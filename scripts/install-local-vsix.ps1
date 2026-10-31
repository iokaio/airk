# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Install a locally built AIRK VSIX, replacing an installed copy with the same version.

.DESCRIPTION
VS Code skips a VSIX whose version is already installed, so a rebuilt local package
looks installed while the old code keeps running. This script runs
`code --install-extension <vsix> --force`, which replaces the same-version copy, then
verifies that the installed extension code matches the VSIX byte for byte.
A forced reinstall is an update, not an uninstall, so AIRK's uninstall cleanup does
not run and indexes are untouched. Reload VS Code windows afterwards.

.PARAMETER Vsix
The VSIX to install. Defaults to the newest artifacts\**\extension\publish\*.vsix.

.PARAMETER Code
The VS Code command line: code (default) or code-insiders.

.PARAMETER ExtensionsDir
Extensions folder to verify. Defaults to %USERPROFILE%\.vscode\extensions
(.vscode-insiders for code-insiders).
#>
param([string]$Vsix, [string]$Code = 'code', [string]$ExtensionsDir)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
if (-not $Vsix) {
    $latest = Get-ChildItem artifacts -Recurse -Filter *.vsix -ErrorAction SilentlyContinue |
        Where-Object { $_.Directory.Name -eq 'publish' } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $latest) { throw 'No VSIX found under artifacts\. Build one first or pass -Vsix.' }
    $Vsix = $latest.FullName
}
$Vsix = (Resolve-Path -LiteralPath $Vsix).Path
if (-not (Get-Command $Code -ErrorAction SilentlyContinue)) { throw "$Code was not found on PATH. In VS Code run 'Shell Command: Install code command in PATH', or pass -Code." }
if (-not $ExtensionsDir) {
    $ExtensionsDir = Join-Path $env:USERPROFILE ($(if ($Code -like '*insiders*') { '.vscode-insiders' } else { '.vscode' }) + '\extensions')
}

Add-Type -AssemblyName System.IO.Compression.FileSystem
function EntryHash([string]$Archive, [string]$Name) {
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $entry = $zip.GetEntry($Name)
        if (-not $entry) { throw "$Name is missing from $Archive" }
        $stream = $entry.Open()
        try { return [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($stream)).Replace('-', '') }
        finally { $stream.Dispose() }
    } finally { $zip.Dispose() }
}
$zip = [IO.Compression.ZipFile]::OpenRead($Vsix)
try {
    $reader = New-Object IO.StreamReader($zip.GetEntry('extension/package.json').Open())
    $package = $reader.ReadToEnd() | ConvertFrom-Json
    $reader.Dispose()
} finally { $zip.Dispose() }
$id = "$($package.publisher).$($package.name)"
Write-Host "Installing $id $($package.version) from $Vsix (forced; replaces the same version)"
& $Code --install-extension $Vsix --force
if ($LASTEXITCODE -ne 0) { throw "$Code --install-extension failed with exit code $LASTEXITCODE" }

$installed = Join-Path $ExtensionsDir "$id-$($package.version)"
$file = Join-Path $installed 'out\serviceExtension.js'
if (-not (Test-Path -LiteralPath $file)) { throw "Installed extension code not found at $file" }
$expected = EntryHash $Vsix 'extension/out/serviceExtension.js'
$actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash
if ($expected -ne $actual) { throw "The installed code at $installed does not match the VSIX. Close all VS Code windows and run this script again." }
Write-Host "Verified: $installed matches the VSIX."
Write-Host 'Reload each open VS Code window (Developer: Reload Window, or AIRK''s "Reload to finish update" action) to run the new code.'
