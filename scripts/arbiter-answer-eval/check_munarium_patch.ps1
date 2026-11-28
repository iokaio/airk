# SPDX-License-Identifier: Apache-2.0
# Run from the human shell after reviewing the patch. The caller grades behavior separately.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Session,
    [Parameter(Mandatory)][string]$Snapshot,
    [Parameter(Mandatory)][string]$Python,
    [Parameter(Mandatory)][string]$Output
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
$rows = @($info.changed_files)
if (!$info.completed -or !$rows.Count) { throw 'A completed session with a patch is required' }
foreach ($row in $rows) {
    if ($row.path -cnotmatch '^clients/python/tests/[A-Za-z0-9_./-]+\.py$' -or
        $row.path.Split('/') -contains '..' -or $row.state -notin 'added','modified') {
        throw "Patch needs separate grading for $($row.path) ($($row.state))"
    }
}
$modules = @($rows | ForEach-Object { $_.path } | Where-Object { [IO.Path]::GetFileName($_) -like 'test_*.py' })
if (!$modules.Count) { throw 'No changed pytest modules identified' }
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
    $stats = @(& git apply --numstat -- $patch)
    if ($LASTEXITCODE -ne 0) { throw 'git apply could not inventory the patch' }
    $paths = @($stats | ForEach-Object { ($_ -split "`t", 3)[2] })
    if (Compare-Object -CaseSensitive @($rows.path | Sort-Object) @($paths | Sort-Object)) {
        throw 'Patch inventory differs from the session receipt'
    }
    foreach ($row in $rows) {
        $file = Get-AirkCheckedPath (Join-Path $work $row.path) $work
        $dir = $work
        foreach ($part in ($row.path.Split('/')[0..($row.path.Split('/').Length - 2)])) {
            $dir = Join-Path $dir $part
            if (!(Test-Path -LiteralPath $dir)) { MakeDir $dir }
        }
        Before $file
        $original = Get-AirkCheckedPath (Join-Path $source $row.path) $source
        if ($row.state -eq 'modified') { [IO.File]::Copy($original, $file) }
        elseif (Test-Path -LiteralPath $original) { throw "Added file exists in snapshot: $original" }
        else { [IO.File]::WriteAllBytes($file, [byte[]]@()) }
        After $file
    }
    & git apply --check -- $patch
    if ($LASTEXITCODE -ne 0) { throw 'Patch does not apply to frozen input' }
    & git apply -- $patch
    if ($LASTEXITCODE -ne 0) { throw 'Patch application failed' }
    VerifyTree $work
    $testArgs = @('-B','-m','pytest','-p','no:cacheprovider','-p','pytest_asyncio.plugin',
                  '-o','asyncio_mode=auto','--tb=line','-q') + $modules
    $result = @(& $executable @testArgs 2>&1)
    $testExit = $LASTEXITCODE
    $result | Out-File -LiteralPath $log -Encoding utf8NoBOM
    After $log
    [ordered]@{
        pytest_exit=$testExit; python=$executable; args=$testArgs; snapshot=$source
        patch_sha256=(Get-FileHash -LiteralPath $patch).Hash.ToLowerInvariant()
        session_sha256=(Get-FileHash -LiteralPath $receipt).Hash.ToLowerInvariant()
        result_sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()
    } | ConvertTo-Json -Depth 4 | Out-File -LiteralPath $meta -Encoding utf8NoBOM
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
exit $testExit