# SPDX-License-Identifier: Apache-2.0
#Requires -Version 7.0
<#
.SYNOPSIS
Repeat the routine native smokes under hosted-runner CPU limits before qualification.

.DESCRIPTION
High-core developer machines rarely expose the deadline races seen on 4-vCPU
hosted runners. This script restricts itself, and therefore every child it
starts (Node, Python, the owned AIRK service and workers), to the requested
logical processors, optionally adds competing CPU load, and repeats the same
native checks routine CI runs. It never relaxes an assertion or retries a
failed iteration. Output stays under ignored artifacts/ci-local/.
#>
[CmdletBinding()]
param(
    [ValidateRange(1, 64)][int]$Cores = 4,
    [ValidateRange(1, 100)][int]$Repeat = 5,
    [ValidateSet('smoke', 'clients', 'install')][string[]]$Suites = @('smoke', 'clients', 'install'),
    [ValidateRange(0, 64)][int]$Load = 0,
    [switch]$SkipBuild,
    [string]$RuntimeDir,
    [switch]$DownloadInputs,
    [string]$StaticOrtDirectory
)
$ErrorActionPreference = 'Stop'
if (-not $Suites.Count) { throw 'At least one suite is required.' }
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'This script reproduces the Windows x64 host. On Linux use taskset -c 0-3 with the same commands.'
}
if ($Cores -gt [Environment]::ProcessorCount) { throw "Only $([Environment]::ProcessorCount) logical processors are available." }
$airkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$airkTarget = 'x86_64-pc-windows-msvc'
$airkRuntime = Join-Path $airkRoot "artifacts/runtime/$airkTarget"
if ($RuntimeDir) {
    if (-not $SkipBuild) { throw '-RuntimeDir requires -SkipBuild; stage a fresh approved runtime before selecting it.' }
    $airkRuntime = [IO.Path]::GetFullPath($RuntimeDir, $airkRoot)
}
$airkRun = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
$airkOutput = Join-Path $airkRoot "artifacts/ci-local/$airkRun"
New-Item -ItemType Directory -Path $airkOutput | Out-Null
Set-Location $airkRoot

function Invoke-AirkTool {
    param([string]$Tool, [string[]]$Arguments, [string]$Log)
    & $Tool @Arguments *>> $Log
    return $LASTEXITCODE
}

if (-not $SkipBuild) {
    # The build runs on all processors; only the measured checks are restricted.
    $airkModelDirectory = if ($env:AIRK_BUNDLED_MODEL_DIR) { $env:AIRK_BUNDLED_MODEL_DIR } else { Join-Path $airkRoot 'target/local-vector-model' }
    $airkModelArgs = @('scripts/provision-local-model.py', '--destination', $airkModelDirectory)
    if ($DownloadInputs) { $airkModelArgs += '--download' }
    if ((Invoke-AirkTool python $airkModelArgs (Join-Path $airkOutput 'model-inputs.log')) -ne 0) { throw 'Model input verification failed.' }
    $airkStaticRoot = if ($StaticOrtDirectory) { [IO.Path]::GetFullPath($StaticOrtDirectory, $airkRoot) } else { Join-Path $airkRoot "target/static-onnx-runtime/$airkTarget" }
    $airkStaticReport = Join-Path $airkOutput 'static-onnx-environment.json'
    $airkStaticArgs = @('scripts/provision-static-onnx-runtime.py', '--target', $airkTarget, '--destination', $airkStaticRoot, '--environment-file', $airkStaticReport)
    if ($DownloadInputs) { $airkStaticArgs += @('--download', '--download-tools', '--build', '--parallel', '2') }
    else { $airkStaticArgs += '--verify' }
    if ((Invoke-AirkTool python $airkStaticArgs (Join-Path $airkOutput 'static-onnx.log')) -ne 0) { throw 'Static ONNX Runtime input verification failed.' }
    # Use a fresh owned output; never delete an existing runtime to start a build.
    $airkRuntime = Join-Path $airkOutput 'runtime'

    if ((Invoke-AirkTool python @('scripts/static-runtime-environment.py', '--environment-file', $airkStaticReport, '--', 'python', 'scripts/build-service-runtime.py', '--profile', 'release', '--destination', $airkRuntime) (Join-Path $airkOutput 'build.log')) -ne 0) {
        throw "Release runtime build failed; see $airkOutput/build.log"
    }
}
if (-not (Test-Path (Join-Path $airkRuntime 'airk.exe'))) { throw "No release runtime at $airkRuntime; omit -SkipBuild." }
# Children inherit this affinity when they are created, including detached service roles.
$airkSelf = [Diagnostics.Process]::GetCurrentProcess()
$airkSelf.ProcessorAffinity = [IntPtr]$(if ($Cores -ge 64) { -1 } else { ([int64]1 -shl $Cores) - 1 })
$burners = @()
try {
    for ($index = 0; $index -lt $Load; $index++) {
        $burners += Start-Process -PassThru -WindowStyle Hidden pwsh -ArgumentList @('-NoProfile', '-Command', 'while ($true) { }')
    }
    $env:AIRK_TEST_BINARY = Join-Path $airkRuntime 'airk.exe'
    $env:AIRK_PROCESS_DIAGNOSTICS = '1'
    $results = [Collections.Generic.List[object]]::new()
    :checks foreach ($iteration in 1..$Repeat) {
        foreach ($suite in $Suites) {
            $log = Join-Path $airkOutput "$suite-$iteration.log"
            $started = Get-Date
            $code = switch ($suite) {
                'smoke' { Invoke-AirkTool python @('scripts/check-service-clients.py', '--runtime-dir', $airkRuntime, '--cold-clients-only', '--report', (Join-Path $airkOutput "cold-$iteration.json")) $log }
                'clients' { Invoke-AirkTool python @('scripts/check-service-clients.py', '--runtime-dir', $airkRuntime, '--files', '16', '--model', '--report', (Join-Path $airkOutput "clients-$iteration.json")) $log }
                'install' { Invoke-AirkTool python @('scripts/check-service-install.py', '--runtime', $airkRuntime) $log }
            }
            $results.Add([pscustomobject]@{ Iteration = $iteration; Suite = $suite; Passed = ($code -eq 0); Seconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 1); Log = $log })
            if ($code -ne 0) { break checks }
        }
    }
} finally {
    foreach ($burner in $burners) { if (-not $burner.HasExited) { $burner.Kill() } }
}
$results | Format-Table -AutoSize | Out-String | Write-Output
$results | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $airkOutput 'summary.json')
$failed = @($results | Where-Object { -not $_.Passed })
Write-Output "$($results.Count - $failed.Count)/$($results.Count) passed with $Cores logical processors and $Load load workers. Logs: $airkOutput"
if ($failed.Count) { exit 1 }
