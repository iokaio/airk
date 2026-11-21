# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Runs one portable-CPU selector of airk-inference through the receipt runner.

.DESCRIPTION
Checks the human execution identity, caps this process to the given affinity
mask (the test process inherits it), records which build and Python processes
were running before and after, and writes a launcher record beside the
receipts. It builds nothing on purpose: build the test binary first with the
unit-test command in README.md, so that no compiler runs under the cap.

Windows only. On other hosts cap the shell yourself and run the Cargo command
from README.md.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('emit', 'load', 'score', 'timing')][string]$Step,
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9.-]*$')][string]$Name,
    # Existing private receipt root under artifacts/. The run writes <Root>\<Name>.
    [Parameter(Mandatory)][string]$Root,
    # The emit request for 'emit', otherwise one emitted tier-input.private.json.
    [Parameter(Mandatory)][string]$InputPath,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$InputSha256,
    [Parameter(Mandatory)][string]$ExpectedSid,
    # Hexadecimal mask covering exactly eight physical cores. Required except for 'emit'.
    [string]$AffinityMask,
    [string]$StaticEnvironment = 'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json',
    # Must be the same string the test binary was built with, or Cargo rebuilds.
    [string]$BundledModelDir,
    [string]$TargetDir,
    [string]$Cargo = (Join-Path $HOME '.cargo/bin/cargo.exe')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'run-selector.ps1 sets a Windows affinity mask; see README.md for other hosts' }

$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
Assert-AirkHumanExecution $ExpectedSid | Out-Null
if (-not $BundledModelDir) { $BundledModelDir = Join-Path $repo 'artifacts\advanced-split\bundled-model-v3' }
if (-not $TargetDir) { $TargetDir = Join-Path $repo 'target' }

$module = 'inference_engine::pool::arbiter::qualification::portable_cpu'
$selector = @{
    emit   = 'portable_cpu_emit_unqualified_profiles'
    load   = 'portable_cpu_load_only_records_resources_with_zero_forwards'
    score  = 'portable_cpu_score_references_timing_and_cast_profile'
    timing = 'portable_cpu_timing_only'
}[$Step]

function Get-Sha256([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }

# Build, test and Python processes plus the machine-wide CPU load at one instant.
function Get-ProcessCensus {
    $pattern = '^(cargo|cargo-nextest|rustc|clippy-driver|link|python|pythonw|airk.*)\.exe$'
    $rows = Get-CimInstance Win32_Process | Where-Object { $_.Name -match $pattern } | ForEach-Object {
        [ordered]@{
            pid               = $_.ProcessId
            parent            = $_.ParentProcessId
            name              = $_.Name
            cpu_seconds       = [math]::Round(($_.KernelModeTime + $_.UserModeTime) / 1e7, 1)
            working_set_bytes = [uint64]$_.WorkingSetSize
            command           = if ($_.CommandLine) { $_.CommandLine.Substring(0, [Math]::Min(240, $_.CommandLine.Length)) } else { $null }
        }
    }
    $total = Get-CimInstance Win32_PerfFormattedData_PerfOS_Processor -Filter "Name='_Total'"
    [ordered]@{
        observed_utc      = (Get-Date).ToUniversalTime().ToString('o')
        total_cpu_percent = [int]$total.PercentProcessorTime
        processes         = @($rows)
    }
}

$affinity = $null
if ($Step -ne 'emit') {
    if (-not $AffinityMask) { throw "step '$Step' needs -AffinityMask covering exactly eight physical cores" }
    $mask = [Convert]::ToUInt64(($AffinityMask -replace '^0[xX]', ''), 16)
    $self = [Diagnostics.Process]::GetCurrentProcess()
    $self.ProcessorAffinity = [IntPtr][BitConverter]::ToInt64([BitConverter]::GetBytes($mask), 0)
    $self.Refresh()
    $observed = [BitConverter]::ToUInt64([BitConverter]::GetBytes($self.ProcessorAffinity.ToInt64()), 0)
    if ($observed -ne $mask) { throw "affinity mask was not applied: requested $AffinityMask, observed 0x$($observed.ToString('x16'))" }
    $affinity = [ordered]@{
        requested          = '0x' + $mask.ToString('x16')
        observed           = '0x' + $observed.ToString('x16')
        logical_processors = [Numerics.BitOperations]::PopCount($mask)
        note               = 'Child processes inherit this mask. The selector itself requires eight usable physical cores and records the mask it observes.'
    }
}

$out = Join-Path $Root $Name
Assert-AirkOutputParent $out $Root $ExpectedSid | Out-Null
New-Item -ItemType Directory -Path $out | Out-Null
Assert-AirkCreatedOutput $out $Root $ExpectedSid | Out-Null

$static = Join-Path $repo $StaticEnvironment
foreach ($entry in (Get-Content -Raw -LiteralPath $static | ConvertFrom-Json).environment.PSObject.Properties) {
    Set-Item -Path "Env:$($entry.Name)" -Value $entry.Value
}
$env:AIRK_BUNDLED_MODEL_DIR = $BundledModelDir
$variable = if ($Step -eq 'emit') { 'AIRK_PORTABLE_CPU_REQUEST' } else { 'AIRK_PORTABLE_CPU_INPUT' }
$output = if ($Step -eq 'emit') { Join-Path $out 'profiles' } else { Join-Path $out "$Step.json" }
Set-Item -Path "Env:$variable" -Value $InputPath
Set-Item -Path "Env:${variable}_SHA256" -Value $InputSha256
$env:AIRK_PORTABLE_CPU_OUTPUT = $output

$python = (Get-Command python).Source
$command = @(
    $python, '-I', '-B', '-S', (Join-Path $repo 'scripts/dev/receipt_runner.py'),
    '--groups', 'native', '--tier', 'T1', '--out', $out, '--target-dir', $TargetDir, $Name,
    $Cargo, 'nextest', 'run', '--profile', 'ci', '-p', 'airk-inference', '--no-default-features',
    '--features', 'local-inference,test-support', '--locked', '--offline', '--timings',
    '--run-ignored', 'only', '-E', "test(=${module}::$selector)"
)
$processor = Get-CimInstance Win32_Processor | Select-Object -First 1
$aggregate = Join-Path (Split-Path $static) 'static-aggregate-receipt.json'
$record = [ordered]@{
    schema             = 'airk.private.portable-cpu-launch/1'
    step               = $Step
    name               = $Name
    selector           = "${module}::$selector"
    command            = $command
    input              = [ordered]@{ variable = $variable; path = $InputPath; sha256 = $InputSha256 }
    output             = $output
    affinity           = $affinity
    processor          = [ordered]@{ name = $processor.Name.Trim(); cores = $processor.NumberOfCores; logical_processors = $processor.NumberOfLogicalProcessors }
    static_environment = [ordered]@{ path = $static; sha256 = Get-Sha256 $static }
    static_aggregate   = if (Test-Path -LiteralPath $aggregate) { [ordered]@{ path = $aggregate; sha256 = Get-Sha256 $aggregate } } else { $null }
    runtime_source     = [ordered]@{ path = 'crates/airk-retrieval/src/arbiter/runtime.rs'; sha256 = Get-Sha256 (Join-Path $repo 'crates/airk-retrieval/src/arbiter/runtime.rs') }
    bundled_model_dir  = $BundledModelDir
    target_directory   = $TargetDir
    volume_free_bytes  = [ordered]@{ before = (Get-Item -LiteralPath $out).PSDrive.Free }
    census_before      = Get-ProcessCensus
    started_utc        = (Get-Date).ToUniversalTime().ToString('o')
}
$clock = [Diagnostics.Stopwatch]::StartNew()
& $command[0] $command[1..($command.Count - 1)]
$record.exit_code = $LASTEXITCODE
$record.elapsed_seconds = [math]::Round($clock.Elapsed.TotalSeconds, 3)
$record.ended_utc = (Get-Date).ToUniversalTime().ToString('o')
$record.census_after = Get-ProcessCensus
$record.volume_free_bytes.after = (Get-Item -LiteralPath $out).PSDrive.Free
$launcher = Join-Path $out 'launcher.json'
$record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $launcher -Encoding utf8NoBOM
Get-ChildItem -LiteralPath $out -Recurse -File | ForEach-Object {
    Assert-AirkCreatedOutput $_.FullName $Root $ExpectedSid | Out-Null
}
Write-Host "launcher record: $launcher (exit $($record.exit_code), $($record.elapsed_seconds) s)"
exit $record.exit_code
