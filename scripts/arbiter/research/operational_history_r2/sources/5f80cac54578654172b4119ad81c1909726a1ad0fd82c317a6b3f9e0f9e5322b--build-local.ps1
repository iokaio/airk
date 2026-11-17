# SPDX-License-Identifier: Apache-2.0
#Requires -Version 7.0
<#
.SYNOPSIS
Build AIRK and install its verified VSIX locally, without publishing anything.
.EXAMPLE
pwsh -NoProfile -File ./build-local.ps1
.EXAMPLE
pwsh -NoProfile -File ./build-local.ps1 -DoChecks
.EXAMPLE
pwsh -NoProfile -File ./build-local.ps1 -Profile 'Development'
.NOTES
Windows x64. Uses scripts/build-windows.ps1 and its documented prerequisites.
Default: skip routine checks and install into VS Code's default user profile.
Use -DoChecks for full validation; compilation and package verification always run.
Rust is discovered on PATH, in CARGO_HOME/bin, or in USERPROFILE/.cargo/bin.
Any PATH adjustment applies only while this script runs.
Run Developer: Reload Window afterward, then reopen the AIRK dashboard.
This file is intentionally Git-ignored and is not a deployment/release hook.
#>
[CmdletBinding()]
param(
    [switch]$DoChecks,
    [string]$Profile,
    [string]$CodeCommand = 'code.cmd'
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'This local build script requires Windows x64.'
}
$airkRoot = $PSScriptRoot
$airkCode = Get-Command $CodeCommand -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $airkCode -and $CodeCommand -eq 'code.cmd') {
    foreach ($airkCandidate in @(
        (Join-Path $env:LOCALAPPDATA 'Programs/Microsoft VS Code/bin/code.cmd'),
        (Join-Path $env:ProgramFiles 'Microsoft VS Code/bin/code.cmd')
    )) {
        if (Test-Path -LiteralPath $airkCandidate -PathType Leaf) {
            $airkCode = Get-Command $airkCandidate -CommandType Application
            break
        }
    }
}
if (-not $airkCode) { throw 'VS Code launcher not found. Supply -CodeCommand "C:/path/to/VS Code/bin/code.cmd".' }
$airkProfileArguments = if ($Profile) { @('--profile', $Profile) } else { @() }
$airkBuildRecord = $null
$airkPreviousPath = $env:PATH
Push-Location $airkRoot
try {
    # VS Code can retain the PATH from before Rust was installed.
    if (-not (Get-Command cargo -CommandType Application -ErrorAction SilentlyContinue) -or
        -not (Get-Command rustup -CommandType Application -ErrorAction SilentlyContinue)) {
        $airkRustDirectories = @(
            if ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME 'bin' }
            if ($env:USERPROFILE) { Join-Path $env:USERPROFILE '.cargo/bin' }
        )
        foreach ($airkRustDirectory in $airkRustDirectories) {
            if ((Test-Path -LiteralPath (Join-Path $airkRustDirectory 'cargo.exe') -PathType Leaf) -and
                (Test-Path -LiteralPath (Join-Path $airkRustDirectory 'rustup.exe') -PathType Leaf)) {
                $env:PATH = $env:PATH + [IO.Path]::PathSeparator + $airkRustDirectory
                Write-Host "Found Rust tools in $airkRustDirectory"
                break
            }
        }
    }
    foreach ($airkRustTool in @('cargo', 'rustup')) {
        if (-not (Get-Command $airkRustTool -CommandType Application -ErrorAction SilentlyContinue)) {
            throw "Cannot find $airkRustTool. Install Rust with rustup, or set CARGO_HOME to your Rust installation directory, then retry."
        }
    }
    $airkPython = if (Get-Command py -CommandType Application -ErrorAction SilentlyContinue) { 'py' } else { 'python' }
    foreach ($airkTool in @('git', 'node', 'npm.cmd', $airkPython)) {
        if (-not (Get-Command $airkTool -CommandType Application -ErrorAction SilentlyContinue)) {
            throw "Required build tool '$airkTool' is missing from PATH. Install it or update PATH, then retry. See scripts/build-windows.ps1 prerequisites in CONTRIBUTING.md."
        }
    }
    # Capture only this successful invocation's receipt path, never a stale VSIX.
    & (Join-Path $airkRoot 'scripts/build-windows.ps1') -SkipChecks:(-not $DoChecks) | ForEach-Object {
        if ($_ -is [string] -and $_.StartsWith('Build record: ')) {
            $airkBuildRecord = $_.Substring('Build record: '.Length)
        }
        Write-Host $_
    }
    if (-not $airkBuildRecord) { throw 'Build did not produce a success record; nothing was installed.' }
    $airkBuildRecord = (Resolve-Path -LiteralPath $airkBuildRecord).Path
    $airkAllowed = [IO.Path]::GetFullPath((Join-Path $airkRoot 'artifacts/local')) + [IO.Path]::DirectorySeparatorChar
    if (-not $airkBuildRecord.StartsWith($airkAllowed, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Build record is outside artifacts/local; nothing was installed.'
    }
    $airkBuild = Get-Content -LiteralPath (Join-Path $airkBuildRecord 'result.json') -Raw | ConvertFrom-Json
    $airkPublish = Join-Path $airkBuildRecord 'vscode/publish'
    $airkReceipt = Get-Content -LiteralPath (Join-Path $airkPublish 'manifest.json') -Raw | ConvertFrom-Json
    if ($airkBuild.schema -ne 'airk-local-build/1' -or $airkReceipt.schema -ne 'airk-extension-candidate/1' -or
        $airkReceipt.extension_id -ne 'iokaio.airk' -or $airkReceipt.platform -ne 'win32-x64' -or
        $airkReceipt.archive -ne "airk-$($airkReceipt.version)-win32-x64.vsix" -or
        $airkReceipt.version -notmatch '^\d+\.\d+\.\d+$') {
        throw 'Unexpected candidate identity; nothing was installed.'
    }
    $airkVsix = (Resolve-Path -LiteralPath (Join-Path $airkPublish $airkReceipt.archive)).Path
    if ($airkVsix -ne $airkBuild.vsix -or
        (Get-FileHash -LiteralPath $airkVsix -Algorithm SHA256).Hash -ne $airkReceipt.archive_sha256) {
        throw 'Built VSIX path or digest differs from its receipt; nothing was installed.'
    }
    Write-Host "Installing AIRK $($airkReceipt.version) from $airkVsix"
    & $airkCode.Source @airkProfileArguments --install-extension $airkVsix --force
    if ($LASTEXITCODE -ne 0) { throw "VS Code installation failed with exit code $LASTEXITCODE. Candidate retained at $airkVsix" }
    $airkInstalled = @(& $airkCode.Source @airkProfileArguments --list-extensions --show-versions)
    if ($LASTEXITCODE -ne 0 -or $airkInstalled -notcontains "iokaio.airk@$($airkReceipt.version)") {
        throw 'VS Code did not report the expected installed AIRK version.'
    }
    @{ schema = 'airk-local-install/1'; vsix = $airkVsix; version = $airkReceipt.version;
        sha256 = $airkReceipt.archive_sha256; profile = $(if ($Profile) { $Profile } else { 'Default' });
        installedAt = [DateTimeOffset]::UtcNow.ToString('o') } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $airkBuildRecord 'local-install.json') -Encoding utf8NoBOM
    Write-Host ''
    Write-Host 'Installed locally. Run Developer: Reload Window in each affected VS Code window.'
    Write-Host 'Reopen the AIRK dashboard; restart connected coding-agent MCP sessions to load the new helper.'
    Write-Host "Build and install records: $airkBuildRecord"
} finally {
    $env:PATH = $airkPreviousPath
    Pop-Location
}
