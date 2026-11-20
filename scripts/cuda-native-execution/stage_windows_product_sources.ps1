# SPDX-License-Identifier: Apache-2.0
# Stage raw authenticated Windows CU15 DLL and legal bytes for native observation.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$HumanSid)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $repo 'scripts/dev/assert-human-execution.ps1')
$null = Assert-AirkHumanExecution $HumanSid
$campaign = Join-Path $repo 'artifacts/v190/final-plan/exec-20261116T052942Z'
$provider = Join-Path $campaign 'windows-cu15-scientific-provider-stage-r1'
$legal = Join-Path $campaign 'windows-product-notice-sources-r3'
$base = Join-Path $campaign 'windows-product-runtime-r1'
$runtime = Join-Path $base 'runtime'
function Hash($p) { (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() }
function Source($p) { $null = Assert-AirkCreatedOutput $p $repo $HumanSid }
function Parent($p) { $null = Assert-AirkOutputParent $p $repo $HumanSid }
function Rows($p, $sha) {
    Source $p
    if ((Hash $p) -ne $sha) { throw "Changed input $p" }
    Get-Content -LiteralPath $p -Raw | ConvertFrom-Json
}
$manifest = Rows (Join-Path $provider 'airk-cuda-runtime.json') '9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5'
$notices = Rows (Join-Path $legal 'legal-owner.json') '98960834a8c607c326127fd359ae330702be3fc5fd05dc50b3e6ece9843f555e'
$rows = @(
    foreach ($row in $manifest.files) {
        if ($row.name -cnotmatch '^[a-z0-9_]+\.dll$') { throw 'Unexpected DLL name' }
        [ordered]@{name=$row.name; source=(Join-Path $provider $row.name); bytes=$row.bytes; sha256=$row.sha256}
    }
    foreach ($row in $notices) {
        if ($row.package_name -cnotmatch '^airk-cuda-notice-[a-zA-Z0-9._-]+$') { throw 'Unexpected notice name' }
        [ordered]@{name=$row.package_name; source=(Join-Path "$legal/notices" $row.package_name); bytes=$row.bytes; sha256=$row.sha256}
    }
)
if ($manifest.files.Count -ne 13 -or $notices.Count -ne 49 -or ($rows.name | Sort-Object -Unique).Count -ne 62) { throw 'Unexpected source roster' }
$volume = Get-Volume -DriveLetter D
$unit = [long]$volume.AllocationUnitSize
if ($unit -le 0) { throw 'Allocation unit unavailable' }
function Rounded($size) { [long]([Math]::Ceiling($size / $unit) * $unit) }
$stagedBytes = 0L
foreach ($r in $rows) {
    Source $r.source
    if ((Get-Item -LiteralPath $r.source).Length -ne $r.bytes -or (Hash $r.source) -ne $r.sha256) { throw "Changed $($r.name)" }
    $stagedBytes += Rounded $r.bytes
}
$existingBytes = 0L
foreach ($dir in (Get-ChildItem -LiteralPath $campaign -Directory -Filter 'windows-product-*')) {
    foreach ($item in (Get-ChildItem -LiteralPath $dir.FullName -Recurse -Force)) {
        $existingBytes += if ($item.PSIsContainer) { $unit } else { Rounded $item.Length }
    }
}
$headroom = 4MB
if (($existingBytes + $stagedBytes + $headroom) -gt 2GB -or $volume.SizeRemaining -lt ($stagedBytes + $headroom)) { throw 'Private product reservation exceeded' }
if (Test-Path -LiteralPath $base) { throw 'Output already exists' }
foreach ($dir in @($base,$runtime)) {
    Parent $dir
    $null = New-Item -ItemType Directory -Path $dir
    Source $dir
}
foreach ($r in $rows) {
    $destination = Join-Path $runtime $r.name
    Parent $destination
    [IO.File]::Copy($r.source, $destination, $false)
    Source $destination
    if ((Get-Item -LiteralPath $destination).Length -ne $r.bytes -or (Hash $destination) -ne $r.sha256) { throw "Output differs $destination" }
}
$receipt = Join-Path $base 'staging.json'
Parent $receipt
[ordered]@{schema='airk.private.windows-product-raw-source-stage/1'; runtime=$runtime; allocation_unit=$unit; physical_stage_bytes=$stagedBytes; prior_product_bytes=$existingBytes; headroom_bytes=$headroom; free_before=$volume.SizeRemaining; files=$rows} |
    ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $receipt -Encoding utf8NoBOM
Source $receipt
Get-FileHash -LiteralPath $receipt
