# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$airkDesignRoot = Join-Path $PSScriptRoot '../docs/research/contracts-v190'
$airkFixtures = Get-Content -LiteralPath (Join-Path $airkDesignRoot 'fixtures.json') -Raw | ConvertFrom-Json -Depth 100
$airkSchemaCases = @{
    scope_cases = 'query-controls.schema.json'
    discovery_cases = 'scope-discovery.schema.json'
    policy_cases = 'source-policy.schema.json'
    receipt_cases = 'verification-receipt.schema.json'
}
$airkChecked = 0
foreach ($airkGroup in $airkSchemaCases.Keys) {
    foreach ($airkCase in $airkFixtures.$airkGroup) {
        $airkJson = ConvertTo-Json -InputObject $airkCase.value -Depth 100 -Compress
        $airkValid = Test-Json -Json $airkJson -SchemaFile (Join-Path $airkDesignRoot $airkSchemaCases[$airkGroup]) -ErrorAction SilentlyContinue
        if ($airkValid -ne $airkCase.valid) { throw "Schema fixture mismatch: $airkGroup/$($airkCase.name)" }
        $airkChecked++
    }
}

function Assert-Design($Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Test-Subtree([string]$Path, [string]$Prefix) {
    return $Prefix -eq '' -or $Path -ceq $Prefix -or $Path.StartsWith($Prefix + '/', [StringComparison]::Ordinal)
}
function Assert-SameSet($Actual, $Expected, [string]$Message) {
    $airkActualSet = @($Actual | Sort-Object -Unique)
    $airkExpectedSet = @($Expected | Sort-Object -Unique)
    Assert-Design (($airkActualSet -join "`n") -ceq ($airkExpectedSet -join "`n")) $Message
}

$airkGraph = $airkFixtures.graph
$airkSelected = @($airkGraph.projects | Where-Object { $_.id -in $airkGraph.selection })
foreach ($airkProject in $airkSelected) {
    Assert-Design ($airkProject.id -in $airkGraph.granted_projects) 'Selected ungranted Project'
    foreach ($airkDefault in $airkProject.defaults) {
        Assert-Design ($airkDefault -in $airkProject.members.index_id) 'Default outside Project membership'
    }
}
Assert-SameSet $airkSelected.members.index_id $airkGraph.expected_indexes 'Unique-index union differs'
Assert-SameSet @($airkSelected.members | Where-Object index_id -EQ $airkFixtures.ids.shared | ForEach-Object path) $airkGraph.expected_shared_paths 'Shared scope broadened'
foreach ($airkHit in $airkGraph.hits) {
    $airkAttribution = foreach ($airkProject in $airkSelected) {
        if (@($airkProject.members | Where-Object {
            $_.index_id -ceq $airkHit.index_id -and $_.source_root_id -ceq $airkHit.source_root_id -and
            (Test-Subtree $airkHit.path $_.path)
        }).Count -gt 0) { $airkProject.id }
    }
    Assert-SameSet $airkAttribution $airkHit.expected_projects 'Incorrect shared-index attribution'
}
Assert-Design (-not (Test-Subtree 'src-other/a.rs' 'src')) 'Component containment broadened'

$airkPartition = $airkFixtures.partition
function Test-PartitionDenied([string]$Path) {
    return @($airkPartition.existing_denies | Where-Object { Test-Subtree $Path $_ }).Count -gt 0
}
function Test-PartitionSelected([string]$Path) {
    if (Test-PartitionDenied $Path) { return $false }
    return @($airkPartition.selection | Where-Object {
        ($_.kind -ceq 'exact' -and $Path -ceq $_.path) -or
        ($_.kind -ceq 'subtree' -and (Test-Subtree $Path $_.path))
    }).Count -gt 0
}
Assert-SameSet @($airkPartition.eligible | Where-Object { Test-PartitionSelected $_ }) $airkPartition.expected_selected 'Split selection differs'
Assert-SameSet @($airkPartition.eligible | Where-Object { -not (Test-PartitionDenied $_) -and -not (Test-PartitionSelected $_) }) $airkPartition.expected_remainder 'Split remainder differs'
foreach ($airkPath in $airkPartition.new_paths) {
    Assert-Design ((Test-PartitionSelected $airkPath.path) -eq $airkPath.destination) 'Future-path partition differs'
}

foreach ($airkCase in @($airkFixtures.policy_cases | Where-Object valid)) {
    $airkPolicy = $airkCase.value
    Assert-Design (@($airkPolicy.roots | Where-Object owner).Count -eq 1) 'Policy needs exactly one owning root'
    Assert-Design (@($airkPolicy.roots.id | Sort-Object -Unique).Count -eq $airkPolicy.roots.Count) 'Duplicate source-root identity'
    foreach ($airkRule in @($airkPolicy.positive_includes) + @($airkPolicy.denies)) {
        if ($null -eq $airkRule) { continue }
        Assert-Design ($airkRule.source_root_id -in $airkPolicy.roots.id) 'Rule points outside policy roots'
    }
}
foreach ($airkCase in @($airkFixtures.receipt_cases | Where-Object valid)) {
    $airkReceipt = $airkCase.value
    Assert-Design ($airkReceipt.observed_from_unix_ms -le $airkReceipt.observed_to_unix_ms) 'Receipt interval reversed'
    foreach ($airkTable in @('origins', 'sources', 'references')) {
        $airkRows = @($airkReceipt.$airkTable)
        Assert-Design (@($airkRows.id | Sort-Object -Unique).Count -eq $airkRows.Count) "Duplicate $airkTable identity"
    }
    foreach ($airkSource in $airkReceipt.sources) {
        $airkOrigin = @($airkReceipt.origins | Where-Object id -CEQ $airkSource.origin_id)
        Assert-Design ($airkOrigin.Count -eq 1) 'Missing source origin'
        foreach ($airkProjectId in $airkSource.project_ids) {
            Assert-Design ($airkProjectId -in $airkOrigin[0].project_ids) 'Source attribution expands origin'
        }
        Assert-Design ($airkSource.observed_from_unix_ms -ge $airkReceipt.observed_from_unix_ms -and $airkSource.observed_to_unix_ms -le $airkReceipt.observed_to_unix_ms -and $airkSource.observed_from_unix_ms -le $airkSource.observed_to_unix_ms) 'Source interval outside receipt'
        if ($airkSource.state -eq 'matched') {
            Assert-Design ($null -ne $airkSource.expected_hash -and $airkSource.expected_hash -ceq $airkSource.observed_hash) 'Matched source lacks equal hashes'
        }
        if ($airkSource.state -eq 'mismatch') {
            Assert-Design ($null -ne $airkSource.observed_hash -and $airkSource.expected_hash -cne $airkSource.observed_hash) 'Mismatch lacks distinct hashes'
        }
        Assert-SameSet $airkSource.reference_ids @($airkReceipt.references | Where-Object source_id -CEQ $airkSource.id | ForEach-Object id) 'Source reference associations differ'
    }
    foreach ($airkReference in $airkReceipt.references) {
        Assert-Design ($airkReference.source_id -in $airkReceipt.sources.id) 'Dangling reference'
    }
    $airkCounts = $airkReceipt.counts
    Assert-Design ($airkCounts.checked_sources -eq ($airkCounts.matched_sources + $airkCounts.mismatched_sources)) 'Comparison arithmetic differs'
    if ($airkReceipt.inventory_complete -and $airkReceipt.source_rows_omitted -eq 0) {
        Assert-Design ($airkCounts.requested_sources -eq @($airkReceipt.sources).Count -and $airkCounts.references -eq @($airkReceipt.references).Count) 'Complete inventory counts differ'
        Assert-Design ($airkCounts.bytes_read -eq (($airkReceipt.sources | Measure-Object bytes_read -Sum).Sum ?? 0)) 'Byte count differs'
        Assert-Design ($airkCounts.matched_sources -eq @($airkReceipt.sources | Where-Object state -EQ matched).Count) 'Match count differs'
        Assert-Design ($airkCounts.mismatched_sources -eq @($airkReceipt.sources | Where-Object state -EQ mismatch).Count) 'Mismatch count differs'
        Assert-Design ($airkCounts.unchecked_sources -eq ($airkCounts.requested_sources - $airkCounts.checked_sources)) 'Unchecked count differs'
    }
}
Assert-SameSet $airkFixtures.workspace_scenarios.scenario (1..24) 'Workspace scenario coverage differs'
foreach ($airkScenario in $airkFixtures.workspace_scenarios) {
    Assert-Design (-not $airkScenario.expected.automatic_registration -and -not $airkScenario.expected.automatic_scope_expansion -and -not $airkScenario.expected.source_scan_on_open) 'Workspace fixture permits implicit work'
    Assert-Design ($airkScenario.expected.behavior.Length -gt 0 -and $airkScenario.qualification_phase -eq 'P6/P14') 'Workspace outcome lacks qualification boundary'
}
Assert-SameSet $airkFixtures.migration_faults.stage @('U0','U1','U2','U3','U4','U5','U6') 'Migration stage coverage differs'
Assert-Design (-not $airkFixtures.legacy_retention.new_profiles_active) 'P3 enables unimplemented profiles'
$airkExtraction = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../crates/airk-core/src/extraction.rs') -Raw
foreach ($airkKind in $airkFixtures.legacy_retention.agent_lean_included) {
    Assert-Design ($airkExtraction.Contains('"' + $airkKind + '"')) "Missing legacy fact kind $airkKind"
}
$airkManifests = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../crates/airk-ingest/src/project_schemas/manifests.rs') -Raw
foreach ($airkKind in $airkFixtures.legacy_retention.agent_lean_omitted_manifest_kinds) {
    Assert-Design ($airkManifests.Contains('"' + $airkKind + '"')) "Missing manifest kind $airkKind"
    Assert-Design ($airkKind -notin $airkFixtures.legacy_retention.agent_lean_included) 'Manifest fixture contradicts legacy allowlist'
}
Write-Output "P3 design checks passed: $airkChecked wire fixtures; scope/attribution, partition, policy, receipt semantics, 24 workspace outcomes and U0-U6. Runtime acceptance remains gated."
