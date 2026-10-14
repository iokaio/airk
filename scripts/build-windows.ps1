# SPDX-License-Identifier: Apache-2.0
#Requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$SkipChecks,
    [switch]$OpenReview,
    [switch]$RunEditorTests
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') { throw 'This script builds Windows x64. Use hosted CI for the other native platforms.' }
$airkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$airkTarget = 'x86_64-pc-windows-msvc'
$airkRun = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
$airkOutput = Join-Path $airkRoot "artifacts/local/$airkRun"
New-Item -ItemType Directory -Path $airkOutput | Out-Null
function Invoke-AirkTool {
    param([string]$Tool, [string[]]$Arguments)
    & $Tool @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Tool failed with exit code $LASTEXITCODE" }
}
$airkPython = if (Get-Command py -ErrorAction SilentlyContinue) { 'py' } else { 'python' }
foreach ($airkTool in @('git', 'cargo', 'rustup', 'node', 'npm.cmd', $airkPython)) { Get-Command $airkTool -ErrorAction Stop | Out-Null }
Push-Location $airkRoot
$airkPreviousBinary = $env:AIRK_TEST_BINARY
try {
    Start-Transcript -LiteralPath (Join-Path $airkOutput 'build.log') | Out-Null
    Invoke-AirkTool rustup @('show')
    Invoke-AirkTool node @('--version')
    Invoke-AirkTool npm.cmd @('ci', '--ignore-scripts', '--no-audit', '--no-fund', '--prefix', 'packages/vscode')
    if (-not $SkipChecks) {
        Invoke-AirkTool cargo @('fmt', '--all', '--check')
        Invoke-AirkTool cargo @('clippy', '--workspace', '--all-targets', '--locked', '--target', $airkTarget, '--', '-D', 'warnings')
        Invoke-AirkTool cargo @('test', '--workspace', '--locked', '--target', $airkTarget)
        Invoke-AirkTool $airkPython @('-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v')
        & (Join-Path $PSScriptRoot 'check-repo.ps1')
        Invoke-AirkTool git @('diff', '--check')
    }
    Invoke-AirkTool cargo @('build', '--release', '--locked', '-p', 'airk-cli', '--target', $airkTarget)
    $env:AIRK_TEST_BINARY = Join-Path $airkRoot "target/$airkTarget/release/airk.exe"
    if (-not $SkipChecks) {
        Invoke-AirkTool $airkPython @('scripts/benchmark-code.py', '--binary', $env:AIRK_TEST_BINARY, '--files', '64', '--output', (Join-Path $airkOutput 'code-benchmark.json'))
        Invoke-AirkTool $airkPython @('scripts/check-document-tables.py', '--binary', $env:AIRK_TEST_BINARY)
        Invoke-AirkTool $airkPython @('scripts/check-storage-lifecycle.py', '--binary', $env:AIRK_TEST_BINARY)
        Invoke-AirkTool $airkPython @('scripts/check-usage-inputs.py', '--binary', $env:AIRK_TEST_BINARY)
    }
    if ($SkipChecks) { Invoke-AirkTool npm.cmd @('run', 'compile', '--prefix', 'packages/vscode') }
    else { Invoke-AirkTool npm.cmd @('test', '--prefix', 'packages/vscode') }
    Invoke-AirkTool $airkPython @('scripts/package-native.py', '--target', $airkTarget, '--output', (Join-Path $airkOutput 'native'))
    Invoke-AirkTool $airkPython @('scripts/package-extension.py', '--target', $airkTarget, '--native-dir', (Join-Path $airkOutput 'native/publish'), '--output', (Join-Path $airkOutput 'vscode'))
    $airkReceipt = Get-Content (Join-Path $airkOutput 'vscode/publish/manifest.json') -Raw | ConvertFrom-Json
    $airkVsix = Join-Path $airkOutput "vscode/publish/$($airkReceipt.archive)"
    if ($RunEditorTests -or $OpenReview) {
        $airkCode = (Get-Command code.cmd -ErrorAction Stop).Source
        $airkProfile = Join-Path $airkOutput 'review-profile'
        $airkExtensions = Join-Path $airkOutput 'review-extensions'
        $airkWorkspace = Join-Path $airkOutput 'review-workspace'
        Copy-Item -LiteralPath (Join-Path $airkRoot 'examples/workspaces/project-a') -Destination $airkWorkspace -Recurse
        Invoke-AirkTool $airkCode @('--user-data-dir', $airkProfile, '--extensions-dir', $airkExtensions, '--install-extension', $airkVsix, '--force')
        if ($RunEditorTests) {
            $airkEditor = Join-Path (Split-Path (Split-Path $airkCode -Parent) -Parent) 'Code.exe'
            if (-not (Test-Path -LiteralPath $airkEditor -PathType Leaf)) { throw 'Cannot locate Code.exe beside the code.cmd launcher.' }
            $airkExtracted = Join-Path $airkOutput 'verified-vsix'
            [IO.Compression.ZipFile]::ExtractToDirectory($airkVsix, $airkExtracted)
            Invoke-AirkTool node @('packages/vscode/scripts/editor-test.cjs', $airkEditor, (Join-Path $airkExtracted 'extension'), $airkProfile, $airkExtensions, $airkWorkspace)
        }
        if ($OpenReview) {
            Invoke-AirkTool $airkCode @('--user-data-dir', $airkProfile, '--extensions-dir', $airkExtensions, '--new-window', $airkWorkspace)
        }
    }
    @{ schema = 'airk-local-build/1'; target = $airkTarget; native = $env:AIRK_TEST_BINARY; vsix = $airkVsix;
        checksSkipped = [bool]$SkipChecks; editorTests = [bool]$RunEditorTests; output = $airkOutput } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $airkOutput 'result.json') -Encoding utf8NoBOM
    Write-Output "Windows executable: $env:AIRK_TEST_BINARY"
    Write-Output "Review VSIX: $airkVsix"
    Write-Output "Build record: $airkOutput"
} finally {
    $env:AIRK_TEST_BINARY = $airkPreviousBinary
    Stop-Transcript -ErrorAction SilentlyContinue | Out-Null
    Pop-Location
}
