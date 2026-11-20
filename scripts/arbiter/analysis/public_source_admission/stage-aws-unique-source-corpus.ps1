# SPDX-License-Identifier: Apache-2.0
# Stage only the uniquely located AWS cases for private source inspection.
[CmdletBinding()]
param([switch]$Run)

$awsWorkspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$awsCampaign = Join-Path $awsWorkspace 'artifacts/v190/final-plan/exec-20261116T052942Z'
$awsSid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
$awsProjectionSha = '9a248fa25c34b0b0b5c4b9caf83a44b2127331971ac547f1885e0c7bc990bf9f'
$awsRevision = '2d06ee9011f3192b2ff38c09f04e01f1ea9e0191'
. (Join-Path $awsWorkspace 'scripts/dev/assert-human-execution.ps1')

function Get-AwsDigest([byte[]]$Data) {
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Data)).ToLowerInvariant()
}

function Select-AwsUniqueCases($Rows) {
    foreach ($row in $Rows) {
        if ($row.projection_status -eq 'ambiguous_occurrences' -and $null -eq $row.source_span) { continue }
        if ($row.projection_status -notin 'unique_literal','unique_markdown_projection' -or
            $row.occurrences_in_document -ne 1 -or $null -eq $row.source_span) {
            throw 'Unexpected AWS projection status or occurrence count'
        }
        $row
    }
}

function Test-AwsSpan([byte[]]$Body, $Span) {
    $start = $Span.byte_start
    $end = $Span.byte_end_exclusive
    if ($start -lt 0 -or $start -ge $end -or $end -gt $Body.Length) { throw 'Invalid span bounds' }
    $hash = Get-AwsDigest $Body[$start..($end - 1)]
    $line = 1
    $first = 1
    for ($i = 0; $i -lt $end; $i++) {
        if ($i -eq $start) { $first = $line }
        if ($i -eq $end - 1) { break }
        if ($Body[$i] -eq 10) { $line++ }
    }
    if ($hash -cne $Span.span_sha256 -or $first -ne $Span.line_start -or $line -ne $Span.line_end_inclusive) {
        throw 'Frozen source span digest or line coordinates differ'
    }
}

function New-AwsCorpusDirectory($Path, $Boundary) {
    $null = Assert-AirkOutputParent $Path $Boundary $awsSid
    $null = [IO.Directory]::CreateDirectory($Path)
    $null = Assert-AirkCreatedOutput $Path $Boundary $awsSid
}

function Write-AwsVerifiedBytes($Path, [byte[]]$Data, $Boundary) {
    $null = Assert-AirkOutputParent $Path $Boundary $awsSid
    if (![IO.File]::Exists($Path)) {
        $stream = [IO.File]::Open($Path, 'CreateNew', 'Write', 'None')
        try { $stream.Write($Data, 0, $Data.Length) } finally { $stream.Dispose() }
    }
    $null = Assert-AirkCreatedOutput $Path $Boundary $awsSid
    if ((Get-FileHash -LiteralPath $Path).Hash -ne (Get-AwsDigest $Data)) { throw "Staged file bytes differ: $Path" }
}

function Invoke-AwsUniqueCorpus {
    $ErrorActionPreference = 'Stop'
    $null = Assert-AirkHumanExecution $awsSid
    $inputPath = Join-Path $awsCampaign 'aws-techqa-style-source-projection-r1/aws-source-projection.json'
    $null = Assert-AirkCreatedOutput $inputPath $awsWorkspace $awsSid
    $projectionBytes = [IO.File]::ReadAllBytes($inputPath)
    if ((Get-AwsDigest $projectionBytes) -ne $awsProjectionSha) { throw 'Unrecognized AWS source receipt' }
    $projection = [Text.Encoding]::UTF8.GetString($projectionBytes) | ConvertFrom-Json -AsHashtable
    $cases = @(Select-AwsUniqueCases $projection.candidates)
    $grouped = @($cases | Group-Object { $_.document.path } | Sort-Object Name)
    if ($projection.source_revision -cne $awsRevision -or $cases.Count -ne 41 -or $grouped.Count -ne 27 -or
        $projection.candidates.Count -ne 65) { throw 'AWS cohort differs from the sealed projection' }

    $output = Join-Path $awsCampaign 'aws-unique-source-corpus-r1'
    $corpus = Join-Path $output 'corpus'
    $documents = [ordered]@{}
    $expected = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $directories = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $client = [Net.Http.HttpClient]::new()
    $client.Timeout = [TimeSpan]::FromSeconds(15)
    $client.MaxResponseContentBufferSize = 2MB
    try {
        foreach ($group in $grouped) {
            $doc = $group.Group[0].document
            $relative = $doc.path
            if ($relative -cnotmatch '^documents/[a-z0-9-]+/doc_source/[A-Za-z0-9_.-]+\.md$') { throw 'Unexpected source path' }
            $path = Get-AirkCheckedPath (Join-Path $corpus $relative) $corpus
            $null = $expected.Add($path)
            $parent = [IO.Path]::GetDirectoryName($path)
            while ($parent -ne $output) {
                $null = $directories.Add($parent)
                $parent = [IO.Path]::GetDirectoryName($parent)
            }
            $url = "https://raw.githubusercontent.com/siagholami/aws-documentation/$awsRevision/$relative"
            $body = $client.GetByteArrayAsync($url).GetAwaiter().GetResult()
            if ($body.Length -ne $doc.bytes -or (Get-AwsDigest $body) -cne $doc.sha256) { throw 'Source document bytes differ' }
            foreach ($row in $group.Group) { Test-AwsSpan $body $row.source_span }
            $documents[$relative] = $body
        }
    } finally { $client.Dispose() }

    # Complete all source authentication before opening the private destination.
    New-AwsCorpusDirectory $output $awsWorkspace
    $manifestPath = Join-Path $output 'manifest.private.json'
    $null = $expected.Add($manifestPath)
    foreach ($item in Get-ChildItem -LiteralPath $output -Force -Recurse) {
        $null = Get-AirkCheckedPath $item.FullName $output
        if (!$directories.Contains($item.FullName) -and !$expected.Contains($item.FullName)) { throw 'Unlisted entry in private output' }
    }
    foreach ($dir in $directories | Sort-Object Length,{ $_ }) { New-AwsCorpusDirectory $dir $awsWorkspace }
    foreach ($relative in $documents.Keys) { Write-AwsVerifiedBytes (Join-Path $corpus $relative) $documents[$relative] $awsWorkspace }

    # The index root contains only unmodified publisher bytes. Keep labels and
    # source notices outside it so they cannot become offered retrieval choices.
    $manifest = [ordered]@{
        schema = 'airk.private.aws-unique-source-corpus/1'
        collector_sha256 = (Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
        corpus_root_relative_to_manifest = 'corpus'
        source_repository = 'https://github.com/siagholami/aws-documentation'
        source_revision = $awsRevision
        source_projection_sha256 = $awsProjectionSha
        excluded_ambiguous_cases = 24
        source_document_count = $documents.Count
        case_count = $cases.Count
        publisher_rights_artifacts = $projection.publisher_rights_artifacts
        cases = $cases
        native_visibility_verified = $false
        evidence_sufficiency_judged = $false
        complete_candidate_menu_frozen = $false
    }
    $json = ConvertTo-Json -InputObject $manifest -Depth 12
    Write-AwsVerifiedBytes $manifestPath ([Text.Encoding]::UTF8.GetBytes($json.Replace("`r`n","`n") + "`n")) $awsWorkspace
    [pscustomobject]@{Output=$manifestPath; ManifestSha256=(Get-FileHash -LiteralPath $manifestPath).Hash; Cases=$cases.Count; Documents=$documents.Count}
}
if ($Run) { Invoke-AwsUniqueCorpus }
