# SPDX-License-Identifier: Apache-2.0
# Stage the frozen model-only roster for the genuine private Windows CUDA Answer service.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$ExpectedSid, [Parameter(Mandatory)][string]$OutputName)

$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$null = Assert-AirkHumanExecution $ExpectedSid
$artifacts = Join-Path $repo 'artifacts'
$campaign = Join-Path $artifacts 'v190/final-plan/exec-20261116T052942Z'
if ($OutputName -notmatch '^compact-qkv-answer-roster-r[1-9][0-9]*$') { throw 'Invalid fresh roster name' }
$output = Join-Path $campaign $OutputName
$assets = Join-Path $output 'compact'
function Read-PinnedFile($Path, $Sha) {
    $null = Assert-AirkCreatedOutput $Path $artifacts $ExpectedSid
    if ((Get-FileHash -LiteralPath $Path).Hash -ne $Sha) { throw "Input SHA differs: $Path" }
    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -AsHashtable
}
function New-Directory($Path) {
    $null = Assert-AirkOutputParent $Path $campaign $ExpectedSid
    $null = New-Item -ItemType Directory -Path $Path -ErrorAction Stop
    $null = Assert-AirkCreatedOutput $Path $campaign $ExpectedSid
}
$original = Read-PinnedFile (Join-Path $campaign 'cal100-focus1024-launch-r1/inputs.json') '8fbde10f3fc8a42d1d05ec0f362a1e0527840ad49fa6686224f1feee4b08373d'
$native = Read-PinnedFile (Join-Path $campaign 'compact-fp16-qkv-fp32-metadata-r1/metadata/native-input.json') 'aada5c3b999eb27eb99b5e4b08fda7fcff9e8240225f02cdd6dfbfe3897932ae'
$profile = Read-PinnedFile $native.profile.path $native.profile.sha256
$graph = Read-PinnedFile (Join-Path $campaign 'compact-fp16-qkv-fp32-native-graph-r2/native-result.json') '42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4'
if ($profile.manifest -ne $graph.profile -or $profile.model -ne 'arbiter-context-qwen3-17b' -or
    $graph.source_mask.verified_candidate_graph -ne 'e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14') { throw 'Native model identity differs' }
$replace = @()
for ($i = 0; $i -lt $original.Count; $i++) {
    $role = Read-PinnedFile $original[$i].profile $original[$i].profile_sha256
    if ($role.model -eq $profile.model) { $replace += $i }
}
if ($replace.Count -ne 1 -or $original.Count -ne 3 -or $profile.assets.Count -ne $native.files.Count) { throw 'Native role roster differs' }
$fixture = @($profile.assets | Where-Object role -eq 'fixture')
if ($fixture.Count -ne 1) { throw 'Exact compiler fixture missing' }
New-Directory $output
New-Directory $assets
foreach ($asset in $profile.assets) {
    if ([IO.Path]::GetFileName($asset.name) -cne $asset.name -or !$native.files.Contains($asset.name)) { throw 'Invalid model asset' }
    $source = $native.files[$asset.name]
    $target = Join-Path $assets $asset.name
    $null = Assert-AirkCreatedOutput $source $artifacts $ExpectedSid
    $null = Assert-AirkOutputParent $target $campaign $ExpectedSid
    if ((Get-Item -LiteralPath $source).Length -ne $asset.bytes) { throw "Invalid source length: $source" }
    [IO.File]::Copy($source, $target, $false)
    $null = Assert-AirkCreatedOutput $target $campaign $ExpectedSid
    if ((Get-Item -LiteralPath $target).Length -ne $asset.bytes -or (Get-FileHash -LiteralPath $target).Hash -ne $asset.sha256) { throw "Staged model bytes differ: $target" }
}
$original[$replace[0]] = [ordered]@{profile=$native.profile.path; profile_sha256=$native.profile.sha256; assets=$assets; fixtures=(Join-Path $assets $fixture[0].name)}
$roster = Join-Path $output 'inputs.json'
$null = Assert-AirkOutputParent $roster $campaign $ExpectedSid
$bytes = [Text.Encoding]::UTF8.GetBytes((ConvertTo-Json -InputObject $original -Depth 8) + [char]10)
$file = [IO.File]::Open($roster, 'CreateNew', 'Write', 'None')
try { $file.Write($bytes) } finally { $file.Dispose() }
$null = Assert-AirkCreatedOutput $roster $campaign $ExpectedSid
Get-FileHash -LiteralPath $roster
