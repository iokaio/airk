# SPDX-License-Identifier: Apache-2.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^(TRAIN_Q[0-9]{3}|ENGINEERING_FIXED|DEV_Q002_EXPOSED|FD_1394|FD_1162)$')][string]$CaseId,
    [Parameter(Mandatory)][ValidateRange(1, 9999)][int]$Attempt,
    [ValidateSet(512, 1024)][int]$FocusWidth = 512,
    [string]$ReuseAfter,
    [ValidatePattern('^[0-9a-f]{64}$')][string]$ReuseSha256
)
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$campaign = Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z'
$variant = if ($FocusWidth -eq 1024) { "-focus1024" } else { "" }
$target = Join-Path $campaign "compact-development-$CaseId$variant-run-r$Attempt"
$sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$null = Assert-AirkHumanExecution $sid
$null = Assert-AirkOutputParent $target $campaign $sid
if (Test-Path -LiteralPath $target) { throw 'Fresh private capture target required' }
$memory = Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory
$commit = $memory.CommitLimit - $memory.CommittedBytes
if ($memory.AvailableMBytes -lt 32768 -or $commit -lt 32GB) { throw 'Insufficient headroom for full Job cap' }
[pscustomobject]@{ physical_available_MiB=$memory.AvailableMBytes; commit_available_bytes=$commit } | ConvertTo-Json -Compress
$exitCode = 1
$shared = Join-Path $campaign "cal100-focus1024-capture-r3/fixture/control"
function Assert-SharedControl {
    $null = Assert-AirkCreatedOutput $shared $campaign $sid
    $entries = @(Get-ChildItem -LiteralPath $shared -Recurse -Force)
    for ($start=0; $start -lt $entries.Count; $start+=28) {
        $end = [Math]::Min($start + 27, $entries.Count - 1)
        $batch = @(foreach ($entry in $entries[$start..$end]) {
            [pscustomobject]@{path=$entry.FullName;root=$shared;created=$true}
        })
        $audit=Assert-AirkOutputBatch $batch $sid 10
        if ($audit.expired -or @($audit.results | Where-Object {!$_.passed}).Count) {
            throw "Shared physical control ownership failed at entry $start"
        }
    }
}
$extra = @()
$held = $false
$mutex = $null
$priorGuard = $env:AIRK_TRAIN_REUSE_AUDITED_CONTROL
try {
    if ($FocusWidth -eq 1024) {
        if (!$ReuseAfter -or !$ReuseSha256) { throw "A checked settled predecessor receipt and digest are required for static-1024 captures" }
        $receipt = (Resolve-Path -LiteralPath $ReuseAfter).Path
        $null = Assert-AirkCreatedOutput $receipt $campaign $sid
        if ((Get-FileHash -LiteralPath $receipt).Hash -ne $ReuseSha256) { throw "Prior closure identity differs" }
        $mutex = [Threading.Mutex]::new($false, "Local\AIRK-20261116-Focus1024-Reuse")
        if (!$mutex.WaitOne(0)) { throw "Another TRAIN owner holds this installation" }
        $held = $true
        Assert-SharedControl
        $env:AIRK_TRAIN_REUSE_AUDITED_CONTROL = [IO.Path]::GetFullPath($shared)
        $extra = @("--reuse-after",$receipt,"--reuse-sha256",$ReuseSha256)
    }
    python -I -B -S (Join-Path $PSScriptRoot 'capture.py') --case $CaseId --focus-width $FocusWidth --output $target @extra
    $exitCode = $LASTEXITCODE
} finally {
    try {
        if ($held) { Assert-SharedControl }
    } finally {
        $env:AIRK_TRAIN_REUSE_AUDITED_CONTROL = $priorGuard
        if ($held) { $mutex.ReleaseMutex() }
        if ($mutex) { $mutex.Dispose() }
    }
    if (Test-Path -LiteralPath $target) {
        $null = Assert-AirkCreatedOutput $target $campaign $sid
        $entries = @(Get-ChildItem -LiteralPath $target -Recurse -Force)
        for ($start=0; $start -lt $entries.Count; $start+=28) {
            $end = [Math]::Min($start + 27, $entries.Count - 1)
            $batch = @(foreach ($entry in $entries[$start..$end]) {
                [pscustomobject]@{path=$entry.FullName; root=$target; created=$true}
            })
            $audit = Assert-AirkOutputBatch $batch $sid 10
            if ($audit.expired -or @($audit.results | Where-Object { !$_.passed }).Count) {
                throw "Output ownership audit failed at entry $start"
            }
        }
        [pscustomobject]@{ case_id=$CaseId; owned_outputs=$entries.Count; fixture_exit=$exitCode } | ConvertTo-Json -Compress
    }
}
exit $exitCode
