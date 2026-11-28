# SPDX-License-Identifier: Apache-2.0
# Stage neutral grading packets and a separate private first/final mapping. Run once per settled session.
param([Parameter(Mandatory)][string]$Session)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$identity = Assert-AirkHumanExecution $sid
$sessionPath = (Resolve-Path -LiteralPath $Session).Path
$series = [IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($sessionPath))
if ((Split-Path (Split-Path $sessionPath) -Leaf) -ne 'runs' -or (Split-Path $series -Leaf) -ne 'E2E-single-r1') { throw 'Unexpected session root' }
$record = Get-Content -Raw -LiteralPath (Join-Path $sessionPath 'session.json') | ConvertFrom-Json
$task = Get-Content -Raw -LiteralPath $record.task.path | ConvertFrom-Json
if (!$record.completed -or (Get-FileHash $record.task.path).Hash -ne $record.task.sha256) { throw 'Session or frozen task mismatch' }
$queue = Join-Path $series 'blind-staging'
$map = Join-Path $series ('blind-provenance/' + (Split-Path $sessionPath -Leaf) + '.json')
function CheckParent($p) { $null=Assert-AirkOutputParent $p $root $sid }
function Owned($p) { $null=Assert-AirkCreatedOutput $p $root $sid }
function Fresh($p) { CheckParent $p; if (Test-Path -LiteralPath $p) { throw "Occupied output $p" } }
function Put($p,$v) { Fresh $p; [IO.File]::WriteAllText($p, (ConvertTo-Json -InputObject $v -Depth 9)); Owned $p }
Fresh $map
$final = Join-Path $sessionPath 'changes.patch'
if ($task.kind -eq 'coding') {
    $first = Join-Path $sessionPath 'first-attempt.patch'
    if (!(Test-Path -LiteralPath $first)) {
        Fresh $first
        python -B (Join-Path $PSScriptRoot 'session_first_attempt.py') $sessionPath $first
        if ($LASTEXITCODE) { throw 'Unable to authenticate first patch' }
    }
    Owned $first
} else {
    $first = $final
    $work = (Resolve-Path -LiteralPath (Join-Path $sessionPath 'work')).Path
    $base = (Resolve-Path -LiteralPath $record.prepared_workspace.baseline).Path
    $result = python -B -c "import sys;from pathlib import Path;sys.path.insert(0,sys.argv[1]);from research_frozen_index import tree_digest;print(tree_digest(Path(sys.argv[2]))==tree_digest(Path(sys.argv[3])))" $PSScriptRoot $work $base
    if ($LASTEXITCODE) { throw 'Source verification failed' }
    $attest = [ordered]@{tree_equals_baseline=($result -eq 'True'); empty_patch=((Get-Item -LiteralPath $final).Length -eq 0); mutating_tool_calls=([int]$record.tool_uses.Edit+[int]$record.tool_uses.Write)}
}
$packets = @()
$seen = @{}
foreach ($phase in 'first','final') {
    $path = if ($phase -eq 'first') { $first } else { $final }
    $sha = (Get-FileHash -LiteralPath $path).Hash
    if (!$seen.ContainsKey($sha)) {
        $id = [Guid]::NewGuid().ToString('N')
        $folder = Join-Path $queue $id
        Fresh $folder
        $null=New-Item -ItemType Directory -Path $folder
        Owned $folder
        Put (Join-Path $folder 'case.json') ([ordered]@{id=$task.id;kind=$task.kind})
        $dst = Join-Path $folder 'changes.patch'
        Fresh $dst
        [IO.File]::Copy($path,$dst)
        Owned $dst
        if ($task.kind -eq 'research') {
            $dst = Join-Path $folder 'answer.md'
            Fresh $dst
            [IO.File]::Copy((Join-Path $sessionPath 'answer.md'),$dst)
            Owned $dst
            Put (Join-Path $folder 'zero-mutation.json') $attest
        }
        foreach ($file in Get-ChildItem -File -LiteralPath $folder) {
            [IO.File]::SetCreationTimeUtc($file.FullName,[datetime]'2000-01-01')
            [IO.File]::SetLastWriteTimeUtc($file.FullName,[datetime]'2000-01-01')
        }
        [IO.Directory]::SetCreationTimeUtc($folder,[datetime]'2000-01-01')
        [IO.Directory]::SetLastWriteTimeUtc($folder,[datetime]'2000-01-01')
        $seen[$sha] = $id
    }
    $packets += [ordered]@{packet=$seen[$sha];phase=$phase;patch_sha256=$sha}
}
Put $map $packets
$seen.Values | Sort-Object