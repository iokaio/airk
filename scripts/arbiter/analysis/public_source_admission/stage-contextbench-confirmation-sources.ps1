# SPDX-License-Identifier: Apache-2.0
# Checked Windows private staging. Point Receipt at the authenticated six-case
# audit and Destination at a fresh directory below the private campaign root.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Receipt,
    [Parameter(Mandatory)][string]$Destination
)
$ErrorActionPreference = 'Stop'
$clock = [Diagnostics.Stopwatch]::StartNew()
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
. (Join-Path $repoRoot 'scripts/dev/assert-human-execution.ps1')
$humanSid = 'S-1-5-21-3678370870-3858642417-826968105-1002'
$null = Assert-AirkHumanExecution $humanSid
if ((git -C $repoRoot branch --show-current) -ne 'feat/arbiter-final') { throw 'Unexpected campaign branch' }
$campaign = (Resolve-Path (Join-Path $repoRoot 'artifacts/v190/final-plan/exec-20261116T052942Z')).Path
$destinationPath = Get-AirkCheckedPath $Destination $campaign
$receiptPath = Get-AirkCheckedPath $Receipt $campaign
$receiptSha = '34d01f50c1e6f4892c4ffcd54243a32af09abc122406a86a9ff5c2e03795a98a'
$bannerSha = 'ac9e0a92816fa4962d6e824550afc04027ea7f883d8cb78b0dc8a2af6fd59afa'
if ($destinationPath -eq $campaign -or (Test-Path -LiteralPath $destinationPath)) { throw 'Use a fresh staging directory' }
$null = Assert-AirkCreatedOutput $receiptPath $repoRoot $humanSid
if ((Get-FileHash -LiteralPath $receiptPath).Hash -ne $receiptSha) { throw 'Rights receipt identity differs' }
$audit = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json -AsHashtable
$approved = [Collections.Generic.List[object]]::new()
$decisions = [Collections.Generic.List[object]]::new()
foreach ($revision in $audit.revisions) {
    $requiredNotices = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($file in $revision.files) {
        $basis = $file.private_basis
        $stage = $file.stage_private
        if ($basis -eq 'withhold_additional_notice_review' -and $revision.repo -eq 'angular/angular') {
            # The inspected immutable banner names Google LLC and MIT. Keep the
            # exact banner together with both package declarations and the grant.
            $banner = @($revision.materials | Where-Object { $_.path -ceq 'packages/license-banner.txt' })
            if ($revision.base -eq 'f7997256fc23e202e08df39997afd360d202f9f1' -and
                $banner.Count -eq 1 -and $banner[0].sha256 -eq $bannerSha -and
                $file.additional_notice_paths.Count -eq 1 -and $file.additional_notice_paths[0] -ceq $banner[0].path -and
                $file.signals.mit_file_header -and $revision.root_license_api.spdx -eq 'MIT') {
                $stage = $true
                $basis = 'reviewed_mit_banner_file_header_package_and_root'
            }
        }
        $decisions.Add([ordered]@{repo=$revision.repo; base=$revision.base; path=$file.path; staged=$stage; basis=$basis})
        if ($stage) {
            $approved.Add(@{area='sources'; repo=$revision.repo; base=$revision.base; entry=$file})
            foreach ($notice in $file.governing_materials) { $null = $requiredNotices.Add($notice) }
        }
    }
    foreach ($material in $revision.materials) {
        if ($requiredNotices.Contains($material.path)) {
            if ($material.status -ne 'authenticated') { throw 'Governing material unavailable' }
            $approved.Add(@{area='notices'; repo=$revision.repo; base=$revision.base; entry=$material})
        }
    }
}
if (@($approved | Where-Object area -eq 'sources').Count -ne 13) { throw 'Reviewed source population differs' }
# Authenticate the full batch before creating the first output directory.
$payloads = [Collections.Generic.List[object]]::new()
foreach ($item in $approved) {
    $e = $item.entry
    if ($e.type -ne 'blob' -or $e.mode -notin '100644','100755' -or $e.size -gt 2000000) { throw 'Unapproved blob' }
    $path = ($e.path.Split('/') | ForEach-Object { [Uri]::EscapeDataString($_) }) -join '/'
    $url = 'https://raw.githubusercontent.com/' + $item.repo + '/' + $item.base + '/' + $path
    $reply = Invoke-WebRequest -Uri $url -TimeoutSec 20
    try { [byte[]]$bytes = $reply.RawContentStream.ToArray() } finally { $reply.RawContentStream.Dispose() }
    $pin = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
    if ($bytes.Length -ne $e.size -or $pin -ne $e.sha256) { throw 'Staging input SHA or size differs from verified blob' }
    $relative = $item.area + '/' + $item.base + '/' + $e.path
    $full = Get-AirkCheckedPath (Join-Path $destinationPath $relative) $destinationPath
    $descriptor = [ordered]@{area=$item.area; relative_path=$relative; repo=$item.repo; base=$item.base;
                             path=$e.path; git_oid=$e.sha; sha256=$pin; bytes=$bytes.Length}
    $payloads.Add(@{full=$full; data=$bytes; descriptor=$descriptor})
}
$created = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
function New-CheckedDirectory([string]$path) {
    if ($created.Contains($path)) { return }
    if ($path -ne $destinationPath) { New-CheckedDirectory ([IO.Path]::GetDirectoryName($path)) }
    $null = Assert-AirkOutputParent $path $repoRoot $humanSid
    if (Test-Path -LiteralPath $path) { throw 'Staging directory appeared during checked creation' }
    $null = New-Item -ItemType Directory -Path $path
    $null = Assert-AirkCreatedOutput $path $repoRoot $humanSid
    $null = $created.Add($path)
}
function Write-CheckedBytes([string]$path, [byte[]]$data) {
    New-CheckedDirectory ([IO.Path]::GetDirectoryName($path))
    $null = Assert-AirkOutputParent $path $repoRoot $humanSid
    $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($data, 0, $data.Length) } finally { $stream.Dispose() }
    $null = Assert-AirkCreatedOutput $path $repoRoot $humanSid
}
foreach ($item in $payloads) {
    Write-CheckedBytes $item.full $item.data
    if ((Get-FileHash -LiteralPath $item.full).Hash -ne $item.descriptor.sha256) { throw 'Staged bytes differ' }
}
$manifest = [ordered]@{schema='airk.private.contextbench-source-staging/1'; rights_receipt_sha256=$receiptSha;
    script_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant(); staged=$payloads.descriptor;
    decisions=$decisions; cases=$audit.cases; note='Each base has a separate source root. Notices are outside source roots.';
    elapsed_before_manifest_seconds=$clock.Elapsed.TotalSeconds; native_reads=0; admitted=0}
$manifestPath = Join-Path $destinationPath 'manifest.private.json'
Write-CheckedBytes $manifestPath ([Text.Encoding]::UTF8.GetBytes(($manifest | ConvertTo-Json -Depth 10) + [char]10))
[ordered]@{manifest_sha256=(Get-FileHash -LiteralPath $manifestPath).Hash; files=$payloads.Count;
           elapsed_seconds=$clock.Elapsed.TotalSeconds} | ConvertTo-Json -Compress
