# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$rootPath = (Resolve-Path -LiteralPath 'D:\code\Github\airk').ProviderPath.TrimEnd('\')
$retentionPath = Join-Path $rootPath 'artifacts\worktree-retention\2026-11-10'
$gitLog = Join-Path $retentionPath 'retirement-git.log'
$receiptPath = Join-Path $retentionPath 'retirement.json'
$receipt = [ordered]@{ main_checkout = $rootPath; moves = @(); removed = @() }
function Save-Receipt { $receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $receiptPath -Encoding utf8 }
function Assert-InRoot([string]$targetPath) {
    $absolute = [IO.Path]::GetFullPath($targetPath)
    if (-not $absolute.StartsWith($rootPath + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Path outside workspace: $absolute" }
    return $absolute
}
function Assert-ExistingDirectory([string]$targetPath) {
    $absolute = Assert-InRoot $targetPath
    $resolved = (Resolve-Path -LiteralPath $absolute).ProviderPath.TrimEnd('\')
    if (-not $resolved.Equals($absolute.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) { throw "Unexpected resolved path: $resolved" }
    $item = Get-Item -LiteralPath $resolved -Force
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Unsafe directory: $resolved" }
    return $resolved
}
$moves = @(
    @('artifacts\v190\p10-worktree\artifacts', 'p10-worktree\ignored-artifacts'),
    @('artifacts\v190\p10-worktree\target\nextest', 'p10-worktree\nextest'),
    @('artifacts\v190\p10-worktree\target\cargo-timings', 'p10-worktree\cargo-timings'),
    @('artifacts\v190\p6f-worktree\target\nextest', 'p6f-worktree\nextest'),
    @('artifacts\v190\p6f-worktree\target\cargo-timings', 'p6f-worktree\cargo-timings'),
    @('artifacts\worktrees\one-process-workers\artifacts', 'one-process-workers\ignored-artifacts'),
    @('artifacts\worktrees\one-process-workers\target\nextest', 'one-process-workers\nextest'),
    @('artifacts\worktrees\one-process-workers\target\cargo-timings', 'one-process-workers\cargo-timings')
)
foreach ($move in $moves) {
    $source = Assert-InRoot (Join-Path $rootPath $move[0])
    if (-not (Test-Path -LiteralPath $source)) { continue }
    $source = Assert-ExistingDirectory $source
    $destination = Assert-InRoot (Join-Path $retentionPath $move[1])
    if (-not $destination.StartsWith($retentionPath + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Wrong retention destination' }
    if (Test-Path -LiteralPath $destination) { throw "Destination already exists: $destination" }
    $parent = Split-Path -Parent $destination
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    Assert-ExistingDirectory $parent | Out-Null
    Move-Item -LiteralPath $source -Destination $destination
    if ((Test-Path -LiteralPath $source) -or -not (Test-Path -LiteralPath $destination)) { throw "Move incomplete: $source" }
    $receipt.moves += [ordered]@{ source = $source; destination = $destination }
    Save-Receipt
    Write-Output "Retained $($move[0])"
}
$snapshots = Get-Content -LiteralPath (Join-Path $retentionPath 'source-checkpoints.json') -Raw | ConvertFrom-Json
$worktrees = @(
    'artifacts\v190\p10-worktree',
    'artifacts\v190\p6e-worktree',
    'artifacts\v190\p6f-worktree',
    'artifacts\worktrees\one-process-workers',
    'artifacts\worktrees\cimac-cold-status-combined',
    'artifacts\worktrees\cold-status-55',
    'artifacts\worktrees\cold-status-fix'
)
foreach ($relative in $worktrees) {
    $worktree = Assert-ExistingDirectory (Join-Path $rootPath $relative)
    $gitArgs = @('-c', "safe.directory=$worktree", '-C', $worktree)
    & git @gitArgs diff --quiet HEAD -- 2>> $gitLog
    if ($LASTEXITCODE -ne 0) { throw "Unpreserved source changes: $relative" }
    $untracked = & git @gitArgs ls-files --others --exclude-standard 2>> $gitLog
    if ($LASTEXITCODE -ne 0 -or $untracked) { throw "Unpreserved untracked source: $relative" }
    $head = (& git @gitArgs rev-parse HEAD).Trim()
    $snapshot = @($snapshots | Where-Object { $_.worktree.Replace('/', '\') -eq $relative })
    $force = $snapshot.Count -eq 1
    if ($force) {
        if ($snapshot[0].source_snapshot -ne $head) { throw "Snapshot identity changed: $relative" }
        & git -c "safe.directory=$rootPath" -C $rootPath merge-base --is-ancestor $head feat/simplify
        if ($LASTEXITCODE -ne 0) { throw "Snapshot not retained by branch: $relative" }
        $archivePath = Join-Path $retentionPath ((Split-Path -Leaf $worktree) + '\raw-working-source.zip')
        if (-not (Test-Path -LiteralPath $archivePath)) { throw "Missing raw source archive: $relative" }
    } else {
        $branch = (& git @gitArgs symbolic-ref --short HEAD).Trim()
        $retainedHead = (& git -c "safe.directory=$rootPath" -C $rootPath rev-parse $branch).Trim()
        if ($retainedHead -ne $head) { throw "Historical branch not retained: $relative" }
    }
    # The only forced removals have exact source snapshots in branch ancestry,
    # raw-byte archives and empty normalized diffs; force handles stat-only CRLF.
    $removeArgs = @('-c', "safe.directory=$rootPath", '-C', $rootPath, 'worktree', 'remove')
    if ($force) { $removeArgs += '--force' }
    $removeArgs += @('--', $worktree)
    & git @removeArgs 2>> $gitLog
    if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $worktree)) { throw "Worktree removal incomplete: $relative" }
    $receipt.removed += [ordered]@{ worktree = $worktree; head = $head; raw_source_retained = $force }
    Save-Receipt
    Write-Output "Removed $relative"
}
& git -c "safe.directory=$rootPath" -C $rootPath worktree list --porcelain
if ($LASTEXITCODE -ne 0) { throw 'Final worktree listing failed' }
