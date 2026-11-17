# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$releaseCommit = 'ec6201f9f4760117746745d4d1ca721239f2a87c'
$producerRun = '39001009698'
$releaseEvidence = Join-Path $PSScriptRoot 'release-1-0-0-publication'
New-Item -ItemType Directory -Force -Path $releaseEvidence | Out-Null
$releaseState = [ordered]@{ commit = $releaseCommit; producer = $producerRun; stage = 'waiting_for_main_ci'; rehearsal = $null; publication = $null }
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'safe.directory'
$env:GIT_CONFIG_VALUE_0 = 'D:/code/Github/airk'

function Save-ReleaseState([string]$Stage) {
    $releaseState.stage = $Stage
    $releaseState | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $releaseEvidence 'status.json')
    Write-Output $Stage
}
function Assert-ReleaseSource {
    $head = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $head -ne $releaseCommit) { throw 'Local checkout moved; publication stopped' }
    $changes = git status --porcelain
    if ($LASTEXITCODE -ne 0 -or $changes) { throw 'Local tracked source changed; publication stopped' }
    $main = gh api repos/iokaio/airk/branches/main --jq .commit.sha
    if ($LASTEXITCODE -ne 0 -or $main -ne $releaseCommit) { throw 'Main moved; publication stopped' }
}
function Wait-ReleaseRun([string]$Run, [string]$Label) {
    gh run watch $Run --exit-status --interval 30 *> (Join-Path $releaseEvidence ($Label + '.log'))
    if ($LASTEXITCODE -ne 0) { throw "$Label failed or could not be watched; no automatic retry" }
}
function Start-ExtensionRelease([bool]$Publish) {
    Assert-ReleaseSource
    $previous = gh run list --workflow extension-release.yml --limit 1 --json databaseId | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect release history' }
    $previousId = if ($previous) { [long]$previous[0].databaseId } else { 0L }
    $publishValue = $Publish.ToString().ToLowerInvariant()
    gh workflow run extension-release.yml --ref main -f version=1.0.0 -f channel=stable -f "publish=$publishValue" -f verify_identity=false -f "candidate_run_id=$producerRun" | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'Could not dispatch extension release' }
    for ($poll = 0; $poll -lt 12; $poll++) {
        $runs = gh run list --workflow extension-release.yml --limit 10 --json databaseId,headSha,event | ConvertFrom-Json
        if ($LASTEXITCODE -ne 0) { throw 'Could not locate dispatched release' }
        $newRuns = @($runs | Where-Object { $_.databaseId -gt $previousId -and $_.event -eq 'workflow_dispatch' })
        if ($newRuns.Count -gt 1) { throw 'Ambiguous concurrent release dispatches; inspect before proceeding' }
        if ($newRuns.Count -eq 1) {
            if ($newRuns[0].headSha -ne $releaseCommit) { throw 'Dispatched workflow belongs to a different source commit' }
            return [string]$newRuns[0].databaseId
        }
        Start-Sleep -Seconds 5
    }
    throw 'Dispatched release did not appear; inspect before retrying'
}
function Get-MarketplaceListing {
    $listing = node packages/vscode/node_modules/@vscode/vsce/vsce show iokaio.airk --json | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect Marketplace listing' }
    return $listing
}

try {
    Save-ReleaseState 'waiting_for_main_ci'
    Wait-ReleaseRun $producerRun 'main-ci'
    Assert-ReleaseSource
    $env:GITHUB_SHA = $releaseCommit
    $env:AIRK_CANDIDATE_RUN_ID = $producerRun
    python scripts/ci-candidates.py resolve --record (Join-Path $releaseEvidence 'producer.json')
    if ($LASTEXITCODE -ne 0) { throw 'Main candidate authentication failed' }
    $listing = Get-MarketplaceListing
    if (@($listing.versions | Where-Object version -eq '1.0.0').Count) { throw 'Marketplace already contains 1.0.0; inspect before publishing' }
    $releaseState.rehearsal = Start-ExtensionRelease $false
    Save-ReleaseState 'rehearsing_stable_packages'
    Wait-ReleaseRun $releaseState.rehearsal 'rehearsal'
    Assert-ReleaseSource
    $rehearsalPackages = Join-Path $releaseEvidence 'rehearsal-packages'
    gh run download $releaseState.rehearsal --pattern 'airk-vsix-*' --dir $rehearsalPackages
    if ($LASTEXITCODE -ne 0) { throw 'Could not download rehearsal packages' }
    python scripts/verify-extension-release.py --directory $rehearsalPackages --commit $releaseCommit --version 1.0.0 --channel stable --output (Join-Path $releaseEvidence 'verified-rehearsal-vsix.json')
    if ($LASTEXITCODE -ne 0) { throw 'Rehearsal package verification failed' }
    $expectedTitle = (Get-Content -Raw packages/vscode/package.json | ConvertFrom-Json).displayName
    $receipts = @(Get-ChildItem -Path $rehearsalPackages -Filter manifest.json -Recurse)
    if ($receipts.Count -ne 4) { throw 'Expected four rehearsal manifests' }
    foreach ($receipt in $receipts) {
        $record = Get-Content -Raw -LiteralPath $receipt.FullName | ConvertFrom-Json
        if ($record.display_name -ne $expectedTitle -or $record.pre_release -ne $false) { throw 'Rehearsal title or channel differs' }
    }
    $listing = Get-MarketplaceListing
    if (@($listing.versions | Where-Object version -eq '1.0.0').Count) { throw 'Marketplace 1.0.0 appeared during rehearsal; inspect before publishing' }
    $releaseState.publication = Start-ExtensionRelease $true
    Save-ReleaseState 'publishing_stable_1_0_0'
    Wait-ReleaseRun $releaseState.publication 'publication'
    Save-ReleaseState 'verifying_marketplace'
    for ($poll = 0; $poll -lt 20; $poll++) {
        $listing = Get-MarketplaceListing
        $versions = @($listing.versions | Where-Object version -eq '1.0.0')
        $platforms = @($versions.targetPlatform | Sort-Object)
        $preview = @($versions | Where-Object { @($_.properties | Where-Object { $_.key -eq 'Microsoft.VisualStudio.Code.PreRelease' -and $_.value -eq 'true' }).Count -gt 0 })
        if ($versions.Count -eq 4 -and ($platforms -join ',') -eq 'darwin-arm64,darwin-x64,linux-x64,win32-x64' -and $preview.Count -eq 0 -and $listing.displayName -eq $expectedTitle) {
            $listing | ConvertTo-Json -Depth 15 | Set-Content -Encoding utf8 (Join-Path $releaseEvidence 'marketplace-verified.json')
            Save-ReleaseState 'published_and_verified'
            exit 0
        }
        Start-Sleep -Seconds 30
    }
    throw 'Publication run succeeded but Marketplace metadata is not fully verified yet; do not republish'
} catch {
    $releaseState.error = $_.Exception.Message
    Save-ReleaseState 'stopped_for_inspection'
    throw
}
