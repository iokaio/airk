# SPDX-License-Identifier: Apache-2.0
# Materialize and verify a failed private patch, then build matched repair prompts.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Stage','Prompts','Verify','Cleanup')][string]$Mode,
    [Parameter(Mandatory)][string]$Design,
    [Parameter(Mandatory)][string]$CaseId,
    [Parameter(Mandatory)][string]$Task,
    [string]$DiagnosticLog
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$identity = Assert-AirkHumanExecution $sid
function Before($p) { $null = Assert-AirkOutputPathObservation $p $root $identity }
function Owned($p) {
    $f = Get-Item -LiteralPath $p -Force
    if (($f.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        (Get-Acl -LiteralPath $p).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid) { throw "Unowned path or link: $p" }
}
function FreshDir($p) {
    if (Test-Path -LiteralPath $p) { throw "Occupied path: $p" }
    Before $p
    $null = [IO.Directory]::CreateDirectory($p)
    Owned $p
}
function SaveJson($p,$v) {
    if (Test-Path -LiteralPath $p) { throw "Occupied output: $p" }
    Before $p
    [IO.File]::WriteAllText($p,($v | ConvertTo-Json -Depth 12))
    Owned $p
}
function Hash($p) { (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() }
function ReadJson($p) { Owned $p; Get-Content -LiteralPath $p -Raw | ConvertFrom-Json }
$privateRoot = Join-Path $root 'artifacts'
$designPath = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Design)) $privateRoot
$plan = ReadJson $designPath
if ($CaseId -cnotmatch '^[A-Za-z0-9_-][A-Za-z0-9_.-]+$') { throw 'Invalid case id' }
$matchesCase = @($plan.cases | Where-Object id -ceq $CaseId)
if ($matchesCase.Count -ne 1) { throw 'Case is not uniquely frozen in design' }
$entry = $matchesCase[0]
$series = [IO.Path]::GetDirectoryName($designPath)
$inputs = Join-Path $series 'inputs'
$out = Join-Path $inputs $CaseId
$staged = Join-Path $out 'snapshot'
$base = Get-AirkCheckedPath ([IO.Path]::GetFullPath($plan.snapshot)) $privateRoot
$session = Get-AirkCheckedPath ([IO.Path]::GetFullPath($entry.source)) $privateRoot
$patch = Join-Path $session 'changes.patch'
$oldReceipt = Join-Path $session 'session.json'
$oldLog = Join-Path $session 'pytest/result.log'
foreach ($file in @($patch,$oldReceipt,$oldLog,$Task)) { Owned $file }
if ((Hash $patch) -ne $entry.patch_sha256 -or (Hash $oldReceipt) -ne $entry.session_sha256 -or
    (Hash $oldLog) -ne $entry.pytest_log_sha256) { throw 'Original archive differs from the frozen design' }
$old = ReadJson $oldReceipt
$taskInfo = ReadJson $Task
$oldTask = ReadJson $old.task.path
if (!$old.completed -or (Hash $old.task.path) -ne $old.task.sha256 -or
    $oldTask.instruction -cne $taskInfo.instruction -or $taskInfo.source_commit -ne $plan.source_commit) {
    throw 'Source task or completion provenance mismatch'
}
$rows = @($old.changed_files)
if (!$rows.Count) { throw 'No failed patch' }
foreach ($row in $rows) {
    if ($row.path -cnotmatch '^clients/python/tests/[A-Za-z0-9_./-]+\.py$' -or $row.path.Split('/') -contains '..' -or
        $row.state -notin 'added','modified') { throw "Unsupported original change $($row.path)" }
}
$manifest = Join-Path $out 'snapshot.json'
if ($Mode -eq 'Cleanup') {
    $receipt = ReadJson $manifest
    $finished = ReadJson (Join-Path $out 'staging.json')
    if ($receipt.snapshot -cne $staged -or $receipt.parent_patch_sha256 -ne $entry.patch_sha256 -or
        $finished.snapshot -cne $staged) { throw 'Unexpected cleanup target' }
    $checked = Get-AirkCheckedPath $staged $out
    Owned $checked
    foreach ($f in Get-ChildItem -LiteralPath $checked -Recurse -Force) { Owned $f.FullName }
    Remove-Item -LiteralPath $checked -Recurse -Force
    return
}
if ($Mode -eq 'Stage') {
    if (!(Test-Path -LiteralPath $inputs)) { FreshDir $inputs }
    FreshDir $out
    & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'checked_cost_fixture.ps1') -Mode CopySnapshot -Session $staged -Snapshot $base
    if ($LASTEXITCODE) { throw 'Checked snapshot copy failed' }
    $previous = $env:GIT_CEILING_DIRECTORIES
    Push-Location -LiteralPath $staged
    try {
        $env:GIT_CEILING_DIRECTORIES = $out
        $stats = @(& git apply --numstat -- $patch)
        if ($LASTEXITCODE) { throw 'Cannot inventory original patch' }
        $paths = @($stats | ForEach-Object { ($_ -split "`t",3)[2] })
        if (Compare-Object -CaseSensitive @($rows.path | Sort-Object) @($paths | Sort-Object)) { throw 'Patch inventory mismatch' }
        foreach ($row in $rows) {
            $file = Get-AirkCheckedPath (Join-Path $staged $row.path) $staged
            $src = Get-AirkCheckedPath (Join-Path $base $row.path) $base
            if ($row.state -eq 'added') {
                if ((Test-Path -LiteralPath $file) -or (Test-Path -LiteralPath $src)) { throw 'Addition already exists' }
                $p = $staged
                foreach ($part in $row.path.Split('/')[0..($row.path.Split('/').Length - 2)]) {
                    $p = Join-Path $p $part
                    if (!(Test-Path -LiteralPath $p)) { FreshDir $p }
                }
                Before $file
                [IO.File]::WriteAllBytes($file,[byte[]]@())
            } elseif ((Hash $file) -ne (Hash $src)) { throw 'Original changed file differs from frozen source' }
            Before $file
            Owned $file
        }
        & git apply --whitespace=nowarn --check -- $patch
        if ($LASTEXITCODE) { throw 'Original patch does not apply' }
        & git apply --whitespace=nowarn -- $patch
        if ($LASTEXITCODE) { throw 'Original patch could not be applied' }
        & git apply --reverse --check -- $patch
        if ($LASTEXITCODE) { throw 'Reversal check failed on staged original patch' }
    } finally {
        Pop-Location
        $env:GIT_CEILING_DIRECTORIES = $previous
    }
    $changed = @(foreach ($row in $rows) {
        $f = Join-Path $staged $row.path
        Owned $f
        [ordered]@{path=$row.path; state=$row.state; sha256=(Hash $f)}
    })
    SaveJson $manifest ([ordered]@{snapshot=$staged; original_snapshot=$base; parent_session=$session
        parent_patch_sha256=(Hash $patch); parent_session_sha256=(Hash $oldReceipt); source_commit=$plan.source_commit
        original_task_sha256=(Hash $Task); changed=$changed})
    return
}
$stage = ReadJson $manifest
if ($stage.snapshot -cne $staged -or $stage.original_snapshot -cne $base -or $stage.parent_patch_sha256 -ne $entry.patch_sha256) {
    throw 'Staged input and design differ'
}
foreach ($f in $stage.changed) { if ((Hash (Join-Path $staged $f.path)) -ne $f.sha256) { throw 'Staged failed file changed' } }
if ($Mode -eq 'Verify') {
    # Compare all original bytes and allowed changed files, including dotfiles, without consuming Git metadata.
    $baseline = @{}
    $expected = @{}
    foreach ($file in [IO.Directory]::EnumerateFiles($base,'*',[IO.SearchOption]::AllDirectories)) {
        $name = [IO.Path]::GetRelativePath($base,$file).Replace('\','/')
        $baseline[$name] = Hash $file
        $expected[$name] = $baseline[$name]
    }
    foreach ($f in $stage.changed) { $expected[$f.path] = $f.sha256 }
    $actual = @{}
    foreach ($file in [IO.Directory]::EnumerateFiles($staged,'*',[IO.SearchOption]::AllDirectories)) {
        $name = [IO.Path]::GetRelativePath($staged,$file).Replace('\','/')
        $actual[$name] = Hash $file
        if (!$expected.ContainsKey($name) -or $actual[$name] -cne $expected[$name]) { throw "Unexpected staged bytes $name" }
    }
    if ($actual.Count -ne $expected.Count) { throw 'Staged snapshot omitted files' }
    function TreeHash($map) {
        [string[]]$paths = @($map.Keys)
        [Array]::Sort($paths,[StringComparer]::Ordinal)
        $manifestText = ($paths | ForEach-Object { $map[$_] + '  ' + $_ }) -join "`n"
        [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($manifestText))).ToLowerInvariant()
    }
    SaveJson (Join-Path $out 'verified-snapshot.json') ([ordered]@{source_files=$baseline.Count; staged_files=$actual.Count
        source_tree_sha256=(TreeHash $baseline); staged_tree_sha256=(TreeHash $actual); parent_patch_sha256=$stage.parent_patch_sha256})
    return
}
if (!$DiagnosticLog) { throw 'Provide the real first failure traceback' }
$traceFile = Get-AirkCheckedPath ([IO.Path]::GetFullPath($DiagnosticLog)) $privateRoot
Owned $traceFile
$traceRoot = [IO.Path]::GetDirectoryName($traceFile)
$verified = ReadJson (Join-Path $traceRoot 'pytest.json')
if (!$verified.first_failure.pytest_exit -or $verified.first_failure.log -cne [IO.Path]::GetFileName($traceFile) -or
    $verified.first_failure.log_sha256 -ne (Hash $traceFile) -or $verified.patch_sha256 -ne $entry.patch_sha256 -or
    $verified.session_sha256 -ne $entry.session_sha256 -or $verified.snapshot -ne $base -or
    '--tb=short' -notin $verified.first_failure.args -or '--maxfail=1' -notin $verified.first_failure.args) {
    throw 'Traceback provenance differs from the archived failure'
}
$trace = [IO.File]::ReadAllText($traceFile).TrimEnd()
$summaryMatch = [regex]::Match([IO.File]::ReadAllText($oldLog),'(?ms)^=+ short test summary info =+[ \t]*\r?$.*\z')
if (!$summaryMatch.Success -or $trace -notmatch '(?m)\b(?:failed|errors?)\b[^\r\n]*\bin [0-9.]+s') { throw 'Missing pytest failure diagnostics' }
function ReportText([string]$value) {
    # Pytest formats the same original modules with different ancestry in first-pass and replay directories.
    foreach ($dir in @((Join-Path $traceRoot 'work'),$base,$session)) {
        $value = $value.Replace(($dir.TrimEnd('\') + '\'),'').Replace(($dir.Replace('\','/').TrimEnd('/') + '/'),'')
    }
    $frozenName = [regex]::Escape([IO.Path]::GetFileName($base))
    $value = [regex]::Replace($value, "(?:\.\.[\\/])+fixture-[^\\/\s]+[\\/]sources[\\/]$frozenName[\\/]", '')
    $value = [regex]::Replace($value, '(?im)^[A-Z]:[\\/][^\r\n]*?[\\/]\.venv[\\/](?=Lib[\\/]site-packages[\\/])', '.venv/')
    foreach ($row in $rows) {
        $filename = [regex]::Escape([IO.Path]::GetFileName($row.path))
        $pattern = '(?m)^(FAILED|ERROR) [^\r\n]*[\\/](?=' + $filename + '(?:::| -|\r?$))'
        $value = [regex]::Replace($value, $pattern, ('$1 ' + $row.path.Substring(0,$row.path.LastIndexOf('/')+1)))
    }
    if ($value -cmatch '(?i)(?:Aardvark|Badger|Caribou|Dragonfly|agent-cost)[\\/_-]') { throw 'Historical condition appears in diagnostic path' }
    $value
}
$summary = ReportText $summaryMatch.Value.TrimEnd()
$trace = ReportText $trace
$firstItemPattern = '(?m)^(?:FAILED|ERROR) (.+?)(?: - [^\r\n]*)?\r?$'
$firstOld = [regex]::Match($summary,$firstItemPattern)
$firstNew = [regex]::Match($trace,$firstItemPattern)
if (!$firstOld.Success -or !$firstNew.Success -or $firstOld.Groups[1].Value -cne $firstNew.Groups[1].Value) {
    throw "Rerun first item differs from archived full run: $($firstOld.Value) / $($firstNew.Value)"
}
$files = ($rows.path | Sort-Object) -join ', '
$prompt = @"
$($taskInfo.instruction)

A previous implementation is already in this checkout. The full pytest run of its changed test modules failed. Changed files: $files.

Make one repair attempt. Keep the required behavior covered on both transports and keep the tests offline. Correct a mistaken test expectation if the source warrants it. Do not change product code, skip required cases, or remove or weaken assertions just to get a passing run. Finish by describing your edits.
"@
$tasks = [ordered]@{}
foreach ($arm in 'blind','feedback') {
    $instructions = $prompt
    if ($arm -eq 'feedback') { $instructions += "`n`nPytest full-run summary:`n$summary`n`nPytest output from rerunning the modules in the same order through the first failure:`n$trace" }
    $file = Join-Path $out "$arm.json"
    SaveJson $file ([ordered]@{repository=$taskInfo.repository;slot=$taskInfo.slot;source_commit=$plan.source_commit;instruction=$instructions})
    $tasks[$arm] = (Hash $file)
}
SaveJson (Join-Path $out 'staging.json') ([ordered]@{snapshot=$staged;original_snapshot=$base;parent_session=$session
    parent_patch_sha256=(Hash $patch);parent_session_sha256=(Hash $oldReceipt);source_commit=$plan.source_commit
    changed=$stage.changed; task_sha256=$tasks; full_log_sha256=(Hash $oldLog); traceback_sha256=(Hash $traceFile)})