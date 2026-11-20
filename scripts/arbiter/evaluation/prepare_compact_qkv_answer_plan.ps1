# SPDX-License-Identifier: Apache-2.0
# Bind the original synthetic Answer workload to a sealed private QKV service image.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ExpectedSid,
    [Parameter(Mandatory)][string]$ServiceImage,
    [Parameter(Mandatory)][string]$ServiceSha256,
    [Parameter(Mandatory)][string]$OutputName
)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$null = Assert-AirkHumanExecution $ExpectedSid
$campaign = Join-Path $repo 'artifacts/v190/final-plan/exec-20261116T052942Z'
$layout = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'airk-final-exec-20261116T052942Z/cu15/gpu-provider-stage'
function Get-Pin($Path, $Expected, $CustodyRoot) {
    $null = Assert-AirkCreatedOutput $Path $CustodyRoot $ExpectedSid
    if ($Expected -notmatch '^[a-fA-F0-9]{64}$' -or (Get-FileHash -LiteralPath $Path).Hash -ne $Expected) { throw "File identity differs: $Path" }
    [ordered]@{path=[IO.Path]::GetFullPath($Path); sha256=$Expected.ToLowerInvariant()}
}
if ($OutputName -notmatch '^compact-qkv-answer-plan-r[1-9][0-9]*\.json$' -or
    [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ServiceImage)) -ne [IO.Path]::GetFullPath($layout) -or
    [IO.Path]::GetExtension($ServiceImage) -ne '.exe') { throw 'A fresh campaign plan and the reviewed sibling runtime are required' }
$output = Join-Path $campaign $OutputName
$base = Get-Pin (Join-Path $campaign 'fp32-paired-answer-r1/cuda-launch-current-roster.json') 'c9243f9256d39b3a17ddcac93c6bfc4165ddb891b0bc910c094acef3455f8698' $campaign
$plan = Get-Content -Raw -LiteralPath $base.path | ConvertFrom-Json -AsHashtable
$null = Get-Pin (Join-Path $layout 'airk-cuda-runtime.json') '9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5' $layout
$plan.service = Get-Pin $ServiceImage $ServiceSha256 $layout
$plan.inputs = Get-Pin (Join-Path $campaign 'compact-qkv-answer-roster-r1/inputs.json') '3902ca16223f3dbbcfa55bf59cd94338d40ae50cb224638446f152d9287a1e14' $campaign
$plan.graph = Get-Pin (Join-Path $campaign 'compact-fp16-qkv-fp32-native-graph-r2/native-result.json') '42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4' $campaign
$plan.manifest = 'dabbc7e062f4f5a350ed486c43c4af2a5cfe5db9dd6c1693739c03a70a5603cf'
$null = Assert-AirkOutputParent $output $campaign $ExpectedSid
$bytes = [Text.Encoding]::UTF8.GetBytes((ConvertTo-Json -InputObject $plan -Depth 12) + [char]10)
$file = [IO.File]::Open($output, 'CreateNew', 'Write', 'None')
try { $file.Write($bytes) } finally { $file.Dispose() }
$null = Assert-AirkCreatedOutput $output $campaign $ExpectedSid
Get-FileHash -LiteralPath $output
