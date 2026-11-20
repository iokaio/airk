# SPDX-License-Identifier: Apache-2.0
# Create the private menu corpus through checked parents, then enumerate and
# authenticate the exact files and ancestors before consuming the manifest.
[CmdletBinding()]
param([switch]$VerifyOnly)
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$campaign = Join-Path $workspace 'artifacts/v190/final-plan/exec-20261116T052942Z'
$out = Join-Path $campaign 'techqa-full-menu-corpus-r1'
$sid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
. (Join-Path $workspace 'scripts/dev/assert-human-execution.ps1')
$null = Assert-AirkHumanExecution $sid
$collector = Join-Path $PSScriptRoot 'stage_techqa_full_menus.py'
$paths = @(python -I -B -S $collector --layout | ConvertFrom-Json)
if ($LASTEXITCODE -or $paths.Count -ne 3527) { throw 'Canonical output layout differs' }
$dirs = @($out, (Join-Path $out 'pilot-menu'), (Join-Path $out 'remaining-documents'))
if (!$VerifyOnly) {
    foreach ($dir in $dirs) {
        $null = Assert-AirkOutputParent $dir $workspace $sid
        if (Test-Path -LiteralPath $dir) { throw 'Fresh destination required' }
        [void][IO.Directory]::CreateDirectory($dir)
        $null = Assert-AirkCreatedOutput $dir $workspace $sid
    }
}
function Test-TechqaOutputPaths([bool]$Created) {
    # All source files are direct children of one of two already checked
    # directories. Use the campaign output as the boundary after verifying
    # its ancestors up to the workspace root.
    foreach ($dir in $dirs) { $null = Assert-AirkCreatedOutput $dir $workspace $sid }
    for ($i=0; $i -lt $paths.Count; $i+=48) {
        $last = [Math]::Min($i+47, $paths.Count-1)
        $batch = @(foreach ($name in $paths[$i..$last]) {
            [pscustomobject]@{path=(Join-Path $out $name); root=$out; created=$Created}
        })
        $audit = Assert-AirkOutputBatch $batch $sid 10
        if ($audit.expired -or @($audit.results | Where-Object { !$_.passed }).Count) {
            throw "Menu output metadata audit failed starting at path $i"
        }
    }
}
if (!$VerifyOnly) {
    Test-TechqaOutputPaths $false
    python -I -B -S $collector
    if ($LASTEXITCODE) { throw 'Menu corpus stage failed' }
}
Test-TechqaOutputPaths $true
$manifestPath = Join-Path $out 'manifest.private.json'
if ((Get-FileHash -LiteralPath $manifestPath).Hash -ne '34109e91b1aa8acd13af56fc706ef3f73006502e3fdffed23843d13b263d1821') {
    throw 'Complete menu manifest differs from the reviewed materialization'
}
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -AsHashtable
$files = @(Get-ChildItem -LiteralPath $out -Recurse -File -Force)
$children = @(Get-ChildItem -LiteralPath $out -Recurse -Directory -Force)
if ($files.Count -ne $paths.Count -or $children.Count -ne 2 -or
    @($children | Where-Object { $_.FullName -notin $dirs }).Count) { throw 'Private source inventory differs' }
$freezePath = Join-Path $campaign 'techqa-original-dev-ordered-menu-freeze-r1.json'
if ((Get-FileHash -LiteralPath $freezePath).Hash -ne '50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7') {
    throw 'Original menu freeze changed'
}
$freeze = (Get-Content -LiteralPath $freezePath -Raw | ConvertFrom-Json -AsHashtable).menu_freeze
foreach ($entry in $manifest.documents.GetEnumerator()) {
    $doc = $entry.Value
    $original = $freeze.documents[$entry.Key]
    $path = Join-Path $out $doc.path
    if ($doc.sha256 -cne $original.sha256 -or $doc.bytes -ne $original.bytes -or
        (Get-Item -LiteralPath $path).Length -ne $original.bytes -or (Get-FileHash -LiteralPath $path).Hash -ne $original.sha256) {
        throw 'Materialized UTF-8 source bytes differ'
    }
}
foreach ($entry in $freeze.cases.GetEnumerator()) {
    $selected = $manifest.ordered_menus[$entry.Key]
    if ($entry.Value.source_status -ne 'exact_publisher_span') {
        if ($null -ne $selected) { throw 'Withheld case present' }
    } elseif ($selected.Count -ne 50 -or ($selected -join '\n') -cne ($entry.Value.ordered_document_ids -join '\n')) {
        throw 'Publisher menu order changed'
    }
}
[ordered]@{cases=$manifest.case_count; memberships=$manifest.case_document_memberships; documents=$manifest.documents.Count;
    source_bytes=$manifest.source_bytes; manifest_sha256=(Get-FileHash -LiteralPath $manifestPath).Hash} | ConvertTo-Json -Compress
