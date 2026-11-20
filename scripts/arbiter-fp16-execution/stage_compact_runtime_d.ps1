# SPDX-License-Identifier: Apache-2.0
# Relocate only the authenticated CU15 payload and the manifest path in the frozen Prepare request.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $root 'scripts/dev/assert-human-execution.ps1')
$identity = Assert-AirkHumanExecution 'S-1-5-21-3678370870-3858642417-826968105-1002'
$campaign = Join-Path $root 'artifacts/v190/final-plan/exec-20261116T052942Z'
$stage = Join-Path $campaign 'windows-cu15-scientific-provider-stage-r1'
$prior = Get-Content (Join-Path $campaign 'compact-fp16-qkv-fp32-metadata-r1/execution.json') -Raw | ConvertFrom-Json
function Validate-Pin($path, $digest) {
    if ((Get-FileHash -LiteralPath $path).Hash -ne $digest) { throw "Mismatched pin for $path" }
}
function Observe($path) {
    $null = Assert-AirkOutputPathObservation $path $root $identity
}
if ((Get-PSDrive D).Free -lt 220GB -or (Test-Path -LiteralPath $stage)) { throw 'Fresh D: stage and campaign reservation required' }
Validate-Pin $prior.runtime_manifest.path '9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5'
Validate-Pin $prior.request.path '850cf966a73bc5ecb0aa79cc3d22559cdd3ad855fbc38f73e361f136ea88c953'
$manifest = Get-Content -LiteralPath $prior.runtime_manifest.path -Raw | ConvertFrom-Json
$original = Split-Path $prior.runtime_manifest.path
Observe $stage
[void][IO.Directory]::CreateDirectory($stage)
Observe $stage
$items = @($manifest.files) + @(
    [pscustomobject]@{ name=$manifest.producer_name; sha256=$manifest.producer_sha256 },
    [pscustomobject]@{ name='airk-cuda-runtime.json'; sha256=$prior.runtime_manifest.sha256 }
)
foreach ($item in $items) {
    if ([IO.Path]::GetFileName($item.name) -cne $item.name) { throw 'Invalid payload leaf' }
    $source = Join-Path $original $item.name
    $destination = Join-Path $stage $item.name
    Validate-Pin $source $item.sha256
    Observe $destination
    [IO.File]::Copy($source, $destination, $false)
    Observe $destination
    Validate-Pin $destination $item.sha256
}
$document = Get-Content -LiteralPath $prior.request.path -Raw | ConvertFrom-Json -AsHashtable
if ($document.runtime_manifest.path -cne $prior.runtime_manifest.path) { throw 'Unexpected original logical manifest' }
$document.runtime_manifest.path = Join-Path $stage 'airk-cuda-runtime.json'
$prepare = Join-Path $campaign 'windows-cu15-scientific-prepare-r1.json'
if (Test-Path -LiteralPath $prepare) { throw 'Prepare output already exists' }
Observe $prepare
[IO.File]::WriteAllText($prepare, ($document | ConvertTo-Json -Depth 16))
Observe $prepare
Get-FileHash $prepare
