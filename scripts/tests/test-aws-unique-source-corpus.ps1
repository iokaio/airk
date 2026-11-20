# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../arbiter/analysis/public_source_admission/stage-aws-unique-source-corpus.ps1')

function Expect-AwsRefusal([scriptblock]$Operation) {
    try { & $Operation } catch { return }
    throw 'Expected a refusal'
}

$body = [Text.Encoding]::UTF8.GetBytes("é`r`nfirst`nlast`n")
$span = @{ byte_start=4; byte_end_exclusive=14; line_start=2; line_end_inclusive=3
           span_sha256=(Get-AwsDigest ([Text.Encoding]::UTF8.GetBytes("first`nlast"))) }
Test-AwsSpan $body $span
$span.line_end_inclusive = 2
Expect-AwsRefusal { Test-AwsSpan $body $span }
$span.line_end_inclusive = 3
$span.byte_end_exclusive = $body.Length + 1
Expect-AwsRefusal { Test-AwsSpan $body $span }
$rows = @(
    @{ projection_status='ambiguous_occurrences'; source_span=$null; occurrences_in_document=2 },
    @{ projection_status='unique_literal'; source_span=$span; occurrences_in_document=1 },
    @{ projection_status='unique_markdown_projection'; source_span=$span; occurrences_in_document=1 }
)
$selected = @(Select-AwsUniqueCases $rows)
if ($selected.Count -ne 2 -or $selected[0] -ne $rows[1] -or $selected[1] -ne $rows[2]) { throw 'Retained selectors differ' }
$rows[0].source_span = $span
Expect-AwsRefusal { Select-AwsUniqueCases $rows }
'5 AWS corpus checks passed'
