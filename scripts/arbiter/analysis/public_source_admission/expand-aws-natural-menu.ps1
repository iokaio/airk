# SPDX-License-Identifier: Apache-2.0
# Materialize every verified publisher file in a fresh private collection.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$ExpectedSid)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$identity = Assert-AirkHumanExecution $ExpectedSid
$campaign = Join-Path $root 'artifacts/v190/final-plan/exec-20261116T052942Z'
$source = Join-Path $campaign 'aws-natural-source-pack-r1'
$archive = Join-Path $source 'documents.private.zip'
$proof = Join-Path $source 'pack.private.json'
$null = Assert-AirkCreatedOutput $archive $root $ExpectedSid
$null = Assert-AirkCreatedOutput $proof $root $ExpectedSid
if ((Get-FileHash $archive).Hash -ne '35afc7ac6651933c932af13cfffcdb0f102e4fecb4cf9ede1a9a50466657ec5e' -or
    (Get-FileHash $proof).Hash -ne '065678389e33c963d9f011b5d6f5d6f1cb6a34c42e3b1b3b573a2372017c1fb3') {
    throw 'Pinned publisher source or per-file proof differs'
}
$manifest = Get-Content -LiteralPath $proof -Raw | ConvertFrom-Json -AsHashtable
$output = Join-Path $campaign 'aws-docs-full-r1'
$null = Assert-AirkOutputParent $output $root $ExpectedSid
if (Test-Path -LiteralPath $output) { throw 'Fresh corpus root required' }
$null = [IO.Directory]::CreateDirectory($output)
$null = Assert-AirkCreatedOutput $output $root $ExpectedSid
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
$verified = 0
try {
    if ($zip.Entries.Count -ne 35515) { throw 'Incomplete source population' }
    $paths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $directories = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $zip.Entries) {
        if (!$entry.FullName.StartsWith('documents/', [StringComparison]::Ordinal) -or !$manifest.files.ContainsKey($entry.FullName)) { throw 'Invalid corpus member' }
        $path = [IO.Path]::GetFullPath((Join-Path $output $entry.FullName.Substring(10)))
        if (!$path.StartsWith($output+'\',[StringComparison]::OrdinalIgnoreCase) -or !$paths.Add($path)) { throw 'Path collision or escape' }
        $parent = [IO.Path]::GetDirectoryName($path)
        while ($parent -ne $output -and $directories.Add($parent)) { $parent = [IO.Path]::GetDirectoryName($parent) }
    }
    foreach ($path in ($directories | Sort-Object Length)) {
        $null = Assert-AirkOutputPathObservation $path $output $identity
        $null = [IO.Directory]::CreateDirectory($path)
        $null = Assert-AirkOutputPathObservation $path $output $identity
    }
    foreach ($entry in $zip.Entries) {
        $path = Join-Path $output $entry.FullName.Substring(10)
        $null = Assert-AirkOutputPathObservation $path $output $identity
        $dst = [IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try { $src = $entry.Open(); try { $src.CopyTo($dst) } finally { $src.Dispose() } } finally { $dst.Dispose() }
        $null = Assert-AirkOutputPathObservation $path $output $identity
        if ((Get-FileHash -LiteralPath $path).Hash -ne $manifest.files[$entry.FullName]) { throw 'Materialized source hash differs' }
        $verified++
    }
} finally { $zip.Dispose() }
if ($verified -ne 35515) { throw 'Materialization incomplete' }
[pscustomobject]@{verified_files=$verified; source_bytes=263271678; collection_root=$output; native_visibility_measured=$false}
