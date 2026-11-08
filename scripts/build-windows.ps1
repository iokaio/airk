# SPDX-License-Identifier: Apache-2.0
#Requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$SkipChecks,
    [switch]$OpenReview,
    [switch]$RunEditorTests,
    [switch]$DownloadInputs,
    [string]$StaticOrtDirectory
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
    if ($airkStaticEnvironment -and ($Tool -eq 'cargo' -or ($Arguments.Count -gt 0 -and $Arguments[0] -eq 'scripts/build-service-runtime.py'))) {
        $airkWrappedArgs = @('scripts/static-runtime-environment.py', '--environment-file', $airkStaticEnvironment, '--', $Tool) + $Arguments
        & $airkPython @airkWrappedArgs
    } else {
        & $Tool @Arguments
    }
    if ($LASTEXITCODE -ne 0) { throw "$Tool failed with exit code $LASTEXITCODE" }
}
$airkPython = if (Get-Command py -ErrorAction SilentlyContinue) { 'py' } else { 'python' }
foreach ($airkTool in @('git', 'cargo', 'rustup', 'node', 'npm.cmd', $airkPython)) { Get-Command $airkTool -ErrorAction Stop | Out-Null }
Push-Location $airkRoot
$airkPreviousBinary = $env:AIRK_TEST_BINARY
$airkStaticEnvironment = $null
try {
    Start-Transcript -LiteralPath (Join-Path $airkOutput 'build.log') | Out-Null
    Invoke-AirkTool rustup @('show')
    Invoke-AirkTool node @('--version')
    Invoke-AirkTool npm.cmd @('ci', '--ignore-scripts', '--no-audit', '--no-fund', '--prefix', 'packages/vscode')
    $airkModelDirectory = if ($env:AIRK_BUNDLED_MODEL_DIR) { $env:AIRK_BUNDLED_MODEL_DIR } else { Join-Path $airkRoot 'target/local-vector-model' }
    $airkModelArgs = @('scripts/provision-local-model.py', '--destination', $airkModelDirectory)
    if ($DownloadInputs) { $airkModelArgs += '--download' }
    Invoke-AirkTool $airkPython $airkModelArgs
    $airkStaticRoot = if ($StaticOrtDirectory) { [IO.Path]::GetFullPath($StaticOrtDirectory, $airkRoot) } else { Join-Path $airkRoot "target/static-onnx-runtime/$airkTarget" }
    $airkStaticReport = Join-Path $airkOutput 'static-onnx-environment.json'
    $airkStaticArgs = @('scripts/provision-static-onnx-runtime.py', '--target', $airkTarget, '--destination', $airkStaticRoot, '--environment-file', $airkStaticReport)
    if ($DownloadInputs) { $airkStaticArgs += @('--download', '--download-tools', '--build', '--parallel', '2') }
    else { $airkStaticArgs += '--verify' }
    Invoke-AirkTool $airkPython $airkStaticArgs
    $airkStaticEnvironment = $airkStaticReport
    if (-not $SkipChecks) {
        Invoke-AirkTool cargo @('fmt', '--all', '--check')
        Invoke-AirkTool cargo @('clippy', '--timings', '--workspace', '--all-targets', '--all-features', '--locked', '--target', $airkTarget, '--', '-D', 'warnings')
        Invoke-AirkTool cargo @('test', '--timings', '--workspace', '--all-features', '--locked', '--target', $airkTarget)
        Invoke-AirkTool cargo @('test', '--timings', '--doc', '--workspace', '--locked', '--target', $airkTarget)
    }
    $airkRuntime = Join-Path $airkOutput 'runtime'
    Invoke-AirkTool $airkPython @('scripts/build-service-runtime.py', '--profile', 'release', '--destination', $airkRuntime)
    $env:AIRK_TEST_BINARY = Join-Path $airkRuntime 'airk.exe'
    # Default retrieval is hybrid: even a quick build must prove the real model/graph works.
    $airkVectorReport = Join-Path $airkOutput 'vector-check.json'
    Invoke-AirkTool $airkPython @('scripts/check-transformer-model.py', '--binary', $env:AIRK_TEST_BINARY, '--report', $airkVectorReport)
    if (-not $SkipChecks) {
        Invoke-AirkTool $airkPython @('scripts/benchmark-code.py', '--binary', $env:AIRK_TEST_BINARY, '--files', '64', '--output', (Join-Path $airkOutput 'code-benchmark.json'))
        Invoke-AirkTool $airkPython @('scripts/check-document-tables.py', '--binary', $env:AIRK_TEST_BINARY)
        Invoke-AirkTool $airkPython @('scripts/check-storage-lifecycle.py', '--binary', $env:AIRK_TEST_BINARY)
        Invoke-AirkTool $airkPython @('scripts/check-usage-inputs.py', '--binary', $env:AIRK_TEST_BINARY)
    }
    if ($SkipChecks) { Invoke-AirkTool npm.cmd @('run', 'compile', '--prefix', 'packages/vscode') }
    else { Invoke-AirkTool npm.cmd @('test', '--prefix', 'packages/vscode') }
    if (-not $SkipChecks) {
        # Packaging contracts inspect out/. Build the current closure through
        # npm test first; an old extension.js must not hide missing new modules.
        Invoke-AirkTool $airkPython @('-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v')
        & (Join-Path $PSScriptRoot 'check-repo.ps1')
        Invoke-AirkTool git @('diff', '--check')
    }
    Invoke-AirkTool $airkPython @('scripts/package-native.py', '--target', $airkTarget, '--runtime-dir', $airkRuntime, '--output', (Join-Path $airkOutput 'native'))
    Invoke-AirkTool $airkPython @('scripts/package-extension.py', '--target', $airkTarget, '--native-dir', (Join-Path $airkOutput 'native/publish'), '--output', (Join-Path $airkOutput 'vscode'))
    $airkReceipt = Get-Content (Join-Path $airkOutput 'vscode/publish/manifest.json') -Raw | ConvertFrom-Json
    $airkVsix = Join-Path $airkOutput "vscode/publish/$($airkReceipt.archive)"
    if ($RunEditorTests -or $OpenReview) {
        $airkCode = (Get-Command code.cmd -ErrorAction Stop).Source
        $airkProfile = Join-Path $airkOutput 'review-profile'
        $airkExtensions = Join-Path $airkOutput 'review-extensions'
        $airkWorkspace = Join-Path $airkOutput 'review-workspace'
        Copy-Item -LiteralPath (Join-Path $airkRoot 'examples/workspaces/project-a') -Destination $airkWorkspace -Recurse
        New-Item -ItemType Directory -Path (Join-Path $airkProfile 'User') -Force | Out-Null
        @{'airk.service.installDir' = (Join-Path $airkOutput 'review-native-service'); 'airk.storage.path' = (Join-Path $airkOutput 'review-index-data')} |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $airkProfile 'User/settings.json') -Encoding utf8NoBOM
        Invoke-AirkTool $airkCode @('--user-data-dir', $airkProfile, '--extensions-dir', $airkExtensions, '--install-extension', $airkVsix, '--force')
        if ($RunEditorTests) {
            $airkEditor = Join-Path (Split-Path (Split-Path $airkCode -Parent) -Parent) 'Code.exe'
            if (-not (Test-Path -LiteralPath $airkEditor -PathType Leaf)) { throw 'Cannot locate Code.exe beside the code.cmd launcher.' }
            $airkLauncherText = Get-Content -LiteralPath $airkCode -Raw
            $airkCliRelative = [regex]::Match($airkLauncherText, '"%~dp0\.\.\\([^"\r\n]*resources\\app\\out\\cli\.js)"').Groups[1].Value
            if (-not $airkCliRelative) { throw 'Cannot locate the matching VS Code CLI script from code.cmd.' }
            $airkCli = Join-Path (Split-Path (Split-Path $airkCode -Parent) -Parent) $airkCliRelative
            if (-not (Test-Path -LiteralPath $airkCli -PathType Leaf)) { throw 'The matching VS Code CLI script is missing.' }
            Invoke-AirkTool node @('scripts/check-native-editor.cjs', $airkEditor, $airkCli, $airkVsix)
        }
        if ($OpenReview) {
            Invoke-AirkTool $airkCode @('--user-data-dir', $airkProfile, '--extensions-dir', $airkExtensions, '--new-window', $airkWorkspace)
        }
    }
    @{ schema = 'airk-local-build/1'; target = $airkTarget; native = $env:AIRK_TEST_BINARY; vsix = $airkVsix;
        checksSkipped = [bool]$SkipChecks; editorTests = [bool]$RunEditorTests; output = $airkOutput;
        vectorCheck = $airkVectorReport } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $airkOutput 'result.json') -Encoding utf8NoBOM
    Write-Output "Windows executable: $env:AIRK_TEST_BINARY"
    Write-Output "Review VSIX: $airkVsix"
    Write-Output "Build record: $airkOutput"
} finally {
    $env:AIRK_TEST_BINARY = $airkPreviousBinary
    Stop-Transcript -ErrorAction SilentlyContinue | Out-Null
    Pop-Location
}
