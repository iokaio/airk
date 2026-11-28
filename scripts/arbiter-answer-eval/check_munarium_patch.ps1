# SPDX-License-Identifier: Apache-2.0
# Run from the human shell after reviewing the patch. The caller grades behavior separately.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Session,
    [Parameter(Mandatory)][string]$Snapshot,
    [Parameter(Mandatory)][string]$Python,
    [Parameter(Mandatory)][string]$Output,
    [string]$ProbeA,
    [string]$ProbeB,
    [switch]$TraceFirstFailure,
    [string]$ParentSession
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$identity = Assert-AirkHumanExecution $sid
$outputRoot = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Output)) (Join-Path $root 'artifacts')
$sessionPath = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Session)) (Join-Path $root 'artifacts')
$source = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Snapshot)) (Join-Path $root 'artifacts')
$executable = (Resolve-Path -LiteralPath $Python).Path
function Before($path) { $null = Assert-AirkOutputPathObservation $path $root $identity }
function After($path) { $null = Assert-AirkCreatedOutput $path $root $sid }
function MakeDir($path) { Before $path; [void][IO.Directory]::CreateDirectory($path); After $path }
function VerifyTree($dir) {
    After $dir
    foreach ($item in Get-ChildItem -LiteralPath $dir -Force -Recurse) { After $item.FullName }
}
$receipt = Join-Path $sessionPath 'session.json'
$patch = Join-Path $sessionPath 'changes.patch'
After $receipt
After $patch
$info = Get-Content -Raw -LiteralPath $receipt | ConvertFrom-Json
$steps = @()
if ($ParentSession) {
    $parentPath = Get-AirkCheckedPath ([IO.Path]::GetFullPath($ParentSession)) (Join-Path $root 'artifacts')
    $parentReceipt = Join-Path $parentPath 'session.json'
    $parentPatch = Join-Path $parentPath 'changes.patch'
    After $parentReceipt
    After $parentPatch
    $parentInfo = Get-Content -Raw -LiteralPath $parentReceipt | ConvertFrom-Json
    if (!$parentInfo.completed -or !$parentInfo.changed_files.Count) { throw 'A completed original patch is required' }
    $steps += @{ patch=$parentPatch; changes=@($parentInfo.changed_files) }
}
$steps += @{ patch=$patch; changes=@($info.changed_files) }
$rows = @($steps | ForEach-Object { $_.changes })
if (!$info.completed -or !$rows.Count) { throw 'A completed session with a patch is required' }
foreach ($row in $rows) {
    if ($row.path -cnotmatch '^clients/python/tests/[A-Za-z0-9_./-]+\.py$' -or
        $row.path.Split('/') -contains '..' -or $row.state -notin 'added','modified','removed') {
        throw "Patch needs separate grading for $($row.path) ($($row.state))"
    }
}
$modules = @($rows | ForEach-Object { $_.path } | Where-Object { [IO.Path]::GetFileName($_) -like 'test_*.py' })
$modules = @($modules | Sort-Object -Unique)
if (!$modules.Count) { throw 'No changed pytest modules identified' }
$probeRequested = $PSBoundParameters.ContainsKey('ProbeA') -or $PSBoundParameters.ContainsKey('ProbeB')
if ($probeRequested) {
    if (!$ProbeA -or !$ProbeB -or $ProbeA -ceq $ProbeB) { throw 'Specify two distinct pytest node IDs' }
    foreach ($node in @($ProbeA,$ProbeB)) {
        $module, $selector = $node -csplit '::', 2
        if ($modules -cnotcontains $module -or
            $selector -cnotmatch '^(?:[A-Za-z_]\w*::)*[A-Za-z_]\w*(?:\[[^\x00-\x1f\\/:]*\])?\z') {
            throw "Expected an exact pytest node in a changed module: $node"
        }
    }
}
if (Test-Path -LiteralPath $outputRoot) { throw 'Choose a fresh grading output directory' }
MakeDir $outputRoot
$work = Join-Path $outputRoot 'work'
$tmp = Join-Path $outputRoot 'tmp'
MakeDir $work
MakeDir $tmp
$log = Join-Path $outputRoot 'result.log'
$meta = Join-Path $outputRoot 'pytest.json'
Before $log
Before $meta
$settings = @{}
$values = @{
    GIT_CEILING_DIRECTORIES=$outputRoot; PYTHONDONTWRITEBYTECODE='1'
    PYTEST_DISABLE_PLUGIN_AUTOLOAD='1'
    PYTHONPATH=(@((Join-Path $source 'clients/python/src'), (Join-Path $source 'clients/python/tests')) -join [IO.Path]::PathSeparator)
    TMP=$tmp; TEMP=$tmp; TMPDIR=$tmp
}
Push-Location -LiteralPath $work
try {
    foreach ($key in $values.Keys) {
        $settings[$key] = [Environment]::GetEnvironmentVariable($key)
        [Environment]::SetEnvironmentVariable($key, $values[$key])
    }
    $pathsToReview = @($rows.path | Sort-Object -Unique)
    $active = @{}
    foreach ($step in $steps) {
        $stepPatch = $step.patch
        if (!(Get-Item -LiteralPath $stepPatch).Length) {
            if ($step.changes.Count) { throw 'Empty patch has a changed-file inventory' }
        } else {
            $stats = @(& git apply --numstat -- $stepPatch)
            if ($LASTEXITCODE -ne 0) { throw 'git apply could not inventory the patch' }
            $paths = @($stats | ForEach-Object { ($_ -split "`t", 3)[2] })
            if (Compare-Object -CaseSensitive @($step.changes.path | Sort-Object) @($paths | Sort-Object)) {
                throw 'Patch inventory differs from the session receipt'
            }
            foreach ($row in $step.changes) {
                $file = Get-AirkCheckedPath (Join-Path $work $row.path) $work
                if ($active.ContainsKey($row.path) -and $active[$row.path]) {
                    if ($row.state -eq 'added') { throw "Added file already exists: $file" }
                    Before $file
                    continue
                }
                $dir = $work
                $parts = $row.path.Split('/')
                foreach ($part in $parts[0..($parts.Length - 2)]) {
                    $dir = Join-Path $dir $part
                    if (!(Test-Path -LiteralPath $dir)) { MakeDir $dir }
                }
                Before $file
                $original = Get-AirkCheckedPath (Join-Path $source $row.path) $source
                if ($row.state -ne 'added') { [IO.File]::Copy($original, $file) }
                elseif ((Test-Path -LiteralPath $original) -and !$active.ContainsKey($row.path)) { throw "Added file exists: $original" }
                else { [IO.File]::WriteAllBytes($file, [byte[]]@()) }
                After $file
                $active[$row.path] = $true
            }
            & git apply --check -- $stepPatch
            if ($LASTEXITCODE -ne 0) { throw 'Patch does not apply to the expected input' }
            & git apply -- $stepPatch
            if ($LASTEXITCODE -ne 0) { throw 'Patch application failed' }
            foreach ($row in $step.changes) {
                if ($row.state -eq 'removed') {
                    $file = Get-AirkCheckedPath (Join-Path $work $row.path) $work
                    After $file
                    if ((Get-Item -LiteralPath $file).Length) { throw "Removed file still has content: $file" }
                    Remove-Item -LiteralPath $file
                    $active[$row.path] = $false
                }
            }
        }
        VerifyTree $work
        if ($ParentSession) {
            $helper = Join-Path $PSScriptRoot 'munarium_repair_evidence.py'
            $argsEvidence = @('-B',$helper,'--snapshot',$source,'--work',$work,'--paths') + $pathsToReview
            $beforePath = Join-Path $outputRoot 'first-pass-tests.json'
            if ($stepPatch -eq $parentPatch) {
                Before $beforePath
                $initial = & $executable @argsEvidence
                if ($LASTEXITCODE) { throw 'Could not inventory first-pass tests' }
                [IO.File]::WriteAllText($beforePath, $initial)
                After $beforePath
            } else {
                $raw = & $executable @argsEvidence --before $beforePath
                if ($LASTEXITCODE) { throw 'Could not inventory repaired tests' }
                $comparison = $raw | ConvertFrom-Json
                $finalPatch = Join-Path $outputRoot 'final.patch'
                Before $finalPatch
                if ((Get-Item -LiteralPath $patch).Length) {
                    [IO.File]::WriteAllText($finalPatch, [string]$comparison.patch)
                } else {
                    [IO.File]::Copy($parentPatch, $finalPatch)
                }
                After $finalPatch
                $comparison.PSObject.Properties.Remove('patch')
                $auditPath = Join-Path $outputRoot 'test-comparison.json'
                Before $auditPath
                $comparison | ConvertTo-Json -Depth 18 | Out-File -LiteralPath $auditPath -Encoding utf8NoBOM
                After $auditPath
            }
        }
    }
    $testArgs = @('-B','-m','pytest','-p','no:cacheprovider','-p','pytest_asyncio.plugin',
                  '-o','asyncio_mode=auto','--tb=line')
    $pytestBase = $testArgs
    $testArgs = $pytestBase + @('-q') + $modules
    $result = @(& $executable @testArgs 2>&1)
    $testExit = $LASTEXITCODE
    $result | Out-File -LiteralPath $log -Encoding utf8NoBOM
    After $log
    $verdict = [ordered]@{
        pytest_exit=$testExit; python=$executable; args=$testArgs; snapshot=$source
        patch_sha256=(Get-FileHash -LiteralPath $patch).Hash.ToLowerInvariant()
        session_sha256=(Get-FileHash -LiteralPath $receipt).Hash.ToLowerInvariant()
        result_sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()
    }
    if ($ParentSession) {
        $verdict.repair = [ordered]@{
            parent_session=$parentPath; parent_patch_sha256=(Get-FileHash $parentPatch).Hash.ToLowerInvariant()
            parent_session_sha256=(Get-FileHash $parentReceipt).Hash.ToLowerInvariant()
            parent_result_sha256=(Get-FileHash (Join-Path $parentPath 'pytest/result.log')).Hash.ToLowerInvariant()
            delta_empty=!(Get-Item -LiteralPath $patch).Length
            final_patch_sha256=(Get-FileHash $finalPatch).Hash.ToLowerInvariant()
            first_pass_tests_sha256=(Get-FileHash $beforePath).Hash.ToLowerInvariant()
            comparison_sha256=(Get-FileHash $auditPath).Hash.ToLowerInvariant()
        }
    }
    if ($TraceFirstFailure) {
        $traceLog = Join-Path $outputRoot 'first-failure.log'
        Before $traceLog
        $traceArgs = @($pytestBase | Where-Object { $_ -ne '--tb=line' }) + @('--tb=short','--maxfail=1','-q') + $modules
        $traceLines = @(& $executable @traceArgs 2>&1)
        $traceCode = $LASTEXITCODE
        $traceLines | Out-File -LiteralPath $traceLog -Encoding utf8NoBOM
        After $traceLog
        $verdict.first_failure = [ordered]@{
            args=$traceArgs; pytest_exit=$traceCode; log='first-failure.log'
            log_sha256=(Get-FileHash -LiteralPath $traceLog).Hash.ToLowerInvariant()
        }
    }
    $overallExit = $testExit
    if ($probeRequested) {
        $runs = [ordered]@{ a=@($ProbeA); b=@($ProbeB); ab=@($ProbeA,$ProbeB); ba=@($ProbeB,$ProbeA) }
        $probes = foreach ($label in $runs.Keys) {
            $nodes = $runs[$label]
            $probeArgs = $pytestBase + @('-v','--color=no','-o','console_output_style=classic','--') + $nodes
            $probeLog = Join-Path $outputRoot "probe-$label.log"
            Before $probeLog
            $lines = @(& $executable @probeArgs 2>&1)
            $code = $LASTEXITCODE
            $lines | Out-File -LiteralPath $probeLog -Encoding utf8NoBOM
            After $probeLog
            # A bare parameterized function can select several items. Require each exact item in requested order.
            $executed = @(foreach ($line in $lines) {
                if ([string]$line -cmatch '^(clients/python/tests/.+?) (PASSED|FAILED|ERROR|SKIPPED|XFAIL|XPASS)(?:\s|$)') {
                    [pscustomobject]@{ node=$Matches[1]; outcome=$Matches[2] }
                }
            })
            $exact = $executed.Count -eq $nodes.Count
            for ($i = 0; $exact -and $i -lt $nodes.Count; $i++) { $exact = $executed[$i].node -ceq $nodes[$i] }
            $passed = $exact -and $code -eq 0 -and @($executed | Where-Object outcome -ne 'PASSED').Count -eq 0
            if (!$passed -and !$overallExit) { $overallExit = 1 }
            [ordered]@{
                run=$label; nodes=$nodes; args=$probeArgs; pytest_exit=$code; executed=$executed
                exact_order=$exact; passed=$passed; log=[IO.Path]::GetFileName($probeLog)
                log_sha256=(Get-FileHash -LiteralPath $probeLog).Hash.ToLowerInvariant()
            }
        }
        $verdict.probes = @($probes)
        $verdict.overall_exit = $overallExit
    }
    $verdict | ConvertTo-Json -Depth 6 | Out-File -LiteralPath $meta -Encoding utf8NoBOM
    After $meta
    Write-Output ($result | Select-Object -Last 1)
} finally {
    Pop-Location
    foreach ($key in $settings.Keys) { [Environment]::SetEnvironmentVariable($key, $settings[$key]) }
    foreach ($dir in @($work,$tmp)) {
        $checked = Get-AirkCheckedPath $dir $outputRoot
        VerifyTree $checked
        Remove-Item -LiteralPath $checked -Recurse -Force
    }
}
exit $overallExit