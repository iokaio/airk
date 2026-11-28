# SPDX-License-Identifier: Apache-2.0
# Independent native validation of an end-to-end panel coding patch.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Session, [Parameter(Mandatory)][string]$Snapshot,
      [Parameter(Mandatory)][string]$Output, [ValidateSet('sse','word-diff')][string]$Kind,
      [string]$Python)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$identity = Assert-AirkHumanExecution $sid
function Before($p) { $null = Assert-AirkOutputPathObservation $p $root $identity }
function Owned($p) {
    $item = Get-Item -LiteralPath $p -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint -or
        (Get-Acl -LiteralPath $p).GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid) { throw "Unexpected file owner or link: $p" }
}
function Fresh($p) { Before $p; if(Test-Path $p){throw "Occupied: $p"}; [void][IO.Directory]::CreateDirectory($p); Owned $p }
function Hash($p) { (Get-FileHash -LiteralPath $p).Hash.ToLowerInvariant() }
$boundary = Join-Path $root 'artifacts'
$s = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Session)) $boundary
$base = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Snapshot)) $boundary
$out = Get-AirkCheckedPath ([IO.Path]::GetFullPath($Output)) $boundary
$patch = Join-Path $s 'changes.patch'
$receipt = Join-Path $s 'session.json'
Owned $patch
Owned $receipt
$data = Get-Content -Raw -LiteralPath $receipt | ConvertFrom-Json
if (!$data.completed -or !$data.changed_files.Count) { throw 'A completed session with a nonempty patch is required' }
if ($Kind -eq 'sse') { $Python = (Resolve-Path -LiteralPath $Python).Path }
Fresh $out
$work = Join-Path $out 'work'
$tmp = Join-Path $out 'tmp'
Fresh $work
Fresh $tmp
$old = @{}
foreach ($n in 'PYTHONDONTWRITEBYTECODE','PYTEST_DISABLE_PLUGIN_AUTOLOAD','TMP','TEMP','TMPDIR','PYTHONPATH','GIT_CEILING_DIRECTORIES') {
    $old[$n] = [Environment]::GetEnvironmentVariable($n)
}
function ScopedDir($relative) {
    $dir = $work
    foreach ($part in $relative.Split('/')) {
        $dir = Get-AirkCheckedPath (Join-Path $dir $part) $work
        if (!(Test-Path -LiteralPath $dir)) { Fresh $dir }
    }
}
$records = [ordered]@{}
function RunCheck($name,$exe,$argv) {
    $log = Join-Path $out ($name+'.log')
    Before $log
    $lines = @(& $exe @argv 2>&1)
    $rc = $LASTEXITCODE
    $lines | Out-File -LiteralPath $log -Encoding utf8NoBOM
    Owned $log
    $records[$name] = @{executable=$exe;args=$argv;exit=$rc;sha256=(Hash $log)}
}
Push-Location $work
try {
    $env:PYTHONDONTWRITEBYTECODE='1'; $env:PYTEST_DISABLE_PLUGIN_AUTOLOAD='1'
    $env:TMP=$tmp; $env:TEMP=$tmp; $env:TMPDIR=$tmp; $env:GIT_CEILING_DIRECTORIES=$out
    if ($Kind -eq 'sse') {
        ScopedDir 'clients'
        & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'checked_cost_fixture.ps1') -Mode CopySnapshot -Session (Join-Path $work 'clients/python') -Snapshot (Join-Path $base 'clients/python')
        if ($LASTEXITCODE) { throw 'Checked Python client copy failed' }
    }
    $stats = @(& git apply --numstat -- $patch)
    if ($LASTEXITCODE) { throw 'Cannot inventory submitted diff' }
    $paths = @($stats | ForEach-Object { ($_ -split "`t",3)[2] })
    if (Compare-Object -CaseSensitive @($data.changed_files.path | Sort-Object) @($paths | Sort-Object)) { throw 'Diff inventory mismatch' }
    foreach($row in $data.changed_files) {
        $rel = [string]$row.path
        if($rel.Split('/') -contains '..' -or $row.state -notin 'modified','added') { throw 'Unexpected change needs individual review' }
        $dst = Get-AirkCheckedPath (Join-Path $work $rel) $work
        $src = Get-AirkCheckedPath (Join-Path $base $rel) $base
        ScopedDir ($rel.Substring(0,$rel.LastIndexOf('/')))
        Before $dst
        if ($row.state -eq 'added') {
            if ((Test-Path $dst) -or (Test-Path $src)) { throw 'Added path exists' }
            [IO.File]::WriteAllBytes($dst,[byte[]]@())
        } elseif (!(Test-Path $dst)) { [IO.File]::Copy($src,$dst) }
        Owned $dst
        if ($row.state -eq 'modified' -and (Hash $src) -ne (Hash $dst)) { throw 'Base bytes differ' }
    }
    & git apply --check -- $patch
    if ($LASTEXITCODE) { throw 'Patch preflight failed' }
    & git apply -- $patch
    if ($LASTEXITCODE) { throw 'Patch application failed' }
    foreach ($row in $data.changed_files) { Owned (Join-Path $work $row.path) }
    if ($Kind -eq 'sse') {
        $tests = @('clients/python/tests/test_sse.py') + @($paths | Where-Object { $_ -match '^clients/python/tests/(.*/)?test_.*\.py$' })
        $tests = @($tests | Sort-Object -Unique)
        $argsTests = @('-B','-m','pytest','-p','no:cacheprovider','-p','pytest_asyncio.plugin','-o','asyncio_mode=auto','--tb=line','-q','--') + $tests
        $env:PYTHONPATH = Join-Path $work 'clients/python/src'
        RunCheck 'patched-public-tests' $Python $argsTests
        RunCheck 'independent-vectors' $Python @('-B',(Join-Path $PSScriptRoot 'panel_sse_check.py'),$env:PYTHONPATH)
        # The candidate's new/updated tests now see the untouched frozen implementation.
        foreach ($row in $data.changed_files | Where-Object { $_.path.StartsWith('clients/python/src/') }) {
            $dst = Get-AirkCheckedPath (Join-Path $work $row.path) $work
            Before $dst
            if ($row.state -eq 'added') { Remove-Item -LiteralPath $dst }
            else { [IO.File]::Copy((Join-Path $base $row.path),$dst,$true); Owned $dst }
        }
        RunCheck 'candidate-tests-stock-code' $Python $argsTests
    } else {
        $node = (Get-Command node.exe).Source
        $mod = Join-Path $work 'pro/src/WritingForgePro-ui/src/utils/textDiff.ts'
        if (!(Test-Path $mod)) { $mod = Join-Path $base 'pro/src/WritingForgePro-ui/src/utils/textDiff.ts' }
        RunCheck 'independent-vectors' $node @('--experimental-vm-modules','--disable-warning=ExperimentalWarning',(Join-Path $PSScriptRoot 'panel_word_diff_check.mjs'),$mod)
    }
    $record = Join-Path $out 'checks.json'
    Before $record
    [ordered]@{patch_sha256=(Hash $patch);session_sha256=(Hash $receipt);snapshot=$base;checks=$records} |
        ConvertTo-Json -Depth 5 | Out-File -LiteralPath $record -Encoding utf8NoBOM
    Owned $record
    $records.Keys | ForEach-Object { Write-Output "$_ exit=$($records[$_].exit)" }
} finally {
    Pop-Location
    foreach($n in $old.Keys) { [Environment]::SetEnvironmentVariable($n,$old[$n]) }
    foreach ($dir in @($work,$tmp)) {
        $checked = Get-AirkCheckedPath $dir $out
        Owned $checked
        foreach($item in Get-ChildItem $checked -Recurse -Force) { Owned $item.FullName }
        Remove-Item -LiteralPath $checked -Recurse -Force
    }
}
