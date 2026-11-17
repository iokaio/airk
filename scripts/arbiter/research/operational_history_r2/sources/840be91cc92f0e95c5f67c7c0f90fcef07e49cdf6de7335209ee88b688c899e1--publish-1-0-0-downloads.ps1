# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$releaseCommit = 'ec6201f9f4760117746745d4d1ca721239f2a87c'
$producerRun = '39001009698'
$evidence = Join-Path $PSScriptRoot 'release-1-0-0-publication'
$marketplaceStatePath = Join-Path $evidence 'status.json'
$downloadsStatePath = Join-Path $evidence 'downloads-status.json'
$downloadsState = [ordered]@{ commit = $releaseCommit; producer = $producerRun; stage = 'waiting_for_marketplace'; run = $null; tag = $null }
function Save-DownloadsState([string]$Stage) {
    $downloadsState.stage = $Stage
    $downloadsState | ConvertTo-Json | Set-Content -Encoding utf8 $downloadsStatePath
    Write-Output $Stage
}
function Read-GitHubJson([string[]]$Arguments) {
    $result = & gh @Arguments
    if ($LASTEXITCODE -ne 0) { throw "GitHub command failed: $($Arguments -join ' ')" }
    return ($result | ConvertFrom-Json)
}
try {
    if (Test-Path -LiteralPath $downloadsStatePath) { throw 'Downloads sequence already has state; inspect before restarting' }
    Save-DownloadsState 'waiting_for_marketplace'
    $deadline = (Get-Date).AddHours(3)
    while ($true) {
        if ((Get-Date) -gt $deadline) { throw 'Timed out waiting for Marketplace; no GitHub release dispatched' }
        try { $marketplace = Get-Content -Raw -LiteralPath $marketplaceStatePath | ConvertFrom-Json } catch { Start-Sleep -Seconds 5; continue }
        if ($marketplace.commit -ne $releaseCommit -or $marketplace.producer -ne $producerRun) { throw 'Marketplace sequence source identity changed' }
        if ($marketplace.stage -eq 'stopped_for_inspection') { throw "Marketplace sequence stopped: $($marketplace.error)" }
        if ($marketplace.stage -eq 'published_and_verified') { break }
        Start-Sleep -Seconds 30
    }
    $publication = Read-GitHubJson @('run', 'view', $marketplace.publication, '--repo', 'iokaio/airk', '--json', 'status,conclusion,headSha')
    if ($publication.status -ne 'completed' -or $publication.conclusion -ne 'success' -or $publication.headSha -ne $releaseCommit) { throw 'Marketplace workflow is not successful on the expected source' }
    $listing = node packages/vscode/node_modules/@vscode/vsce/vsce show iokaio.airk --json | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cannot reverify Marketplace' }
    $versions = @($listing.versions | Where-Object version -eq '1.0.0')
    $preview = @($versions | Where-Object { @($_.properties | Where-Object { $_.key -eq 'Microsoft.VisualStudio.Code.PreRelease' -and $_.value -eq 'true' }).Count -gt 0 })
    if ($versions.Count -ne 4 -or (($versions.targetPlatform | Sort-Object) -join ',') -ne 'darwin-arm64,darwin-x64,linux-x64,win32-x64' -or $preview.Count) { throw 'Marketplace stable 1.0.0 platform set differs' }
    $main = Read-GitHubJson @('api', 'repos/iokaio/airk/branches/main')
    if ($main.commit.sha -ne $releaseCommit) { throw 'Main moved; inspect before releasing another source' }
    $history = @(Read-GitHubJson @('run', 'list', '--repo', 'iokaio/airk', '--workflow', 'github-release.yml', '--limit', '1', '--json', 'databaseId'))
    $previousId = if ($history.Count) { [long]$history[0].databaseId } else { 0L }
    Save-DownloadsState 'dispatching_github_release'
    gh workflow run github-release.yml --repo iokaio/airk --ref main -f channel=stable -f publish=true -f "candidate_run_id=$producerRun"
    if ($LASTEXITCODE -ne 0) { throw 'GitHub release dispatch failed; inspect before retrying' }
    for ($poll = 0; $poll -lt 12; $poll++) {
        $runs = @(Read-GitHubJson @('run', 'list', '--repo', 'iokaio/airk', '--workflow', 'github-release.yml', '--limit', '10', '--json', 'databaseId,headSha,event,number'))
        $newRuns = @($runs | Where-Object { $_.databaseId -gt $previousId -and $_.event -eq 'workflow_dispatch' })
        if ($newRuns.Count -gt 1) { throw 'Concurrent dispatches are ambiguous; inspect before continuing' }
        if ($newRuns.Count -eq 1) {
            if ($newRuns[0].headSha -ne $releaseCommit) { throw 'Dispatched release source differs' }
            $downloadsState.run = [string]$newRuns[0].databaseId
            $downloadsState.tag = 'v1.0.0-build.' + $newRuns[0].number
            break
        }
        Start-Sleep -Seconds 5
    }
    if (-not $downloadsState.run) { throw 'Could not find dispatched release; do not dispatch a duplicate' }
    Save-DownloadsState 'signing_publishing_and_deploying'
    gh run watch $downloadsState.run --repo iokaio/airk --exit-status --interval 30 *> (Join-Path $evidence 'github-release.log')
    if ($LASTEXITCODE -ne 0) { throw 'GitHub release or Pages deployment failed; inspect retained jobs before retrying' }
    Save-DownloadsState 'verifying_live_downloads'
    $tag = $downloadsState.tag
    $release = Read-GitHubJson @('api', "repos/iokaio/airk/releases/tags/$tag")
    if ($release.draft -ne $false -or $release.prerelease -ne $false -or $release.target_commitish -ne $releaseCommit) { throw 'Published release identity or channel differs' }
    $reference = Read-GitHubJson @('api', "repos/iokaio/airk/git/ref/tags/$tag")
    if ($reference.object.sha -ne $releaseCommit) { throw 'Published tag source differs' }
    $release | ConvertTo-Json -Depth 20 | Set-Content -Encoding utf8 (Join-Path $evidence 'github-release-verified.json')
    $expected = @('release.json', 'release-notes.md', 'SHA256SUMS')
    foreach ($platform in @('win32-x64', 'linux-x64', 'darwin-arm64', 'darwin-x64')) { $expected += "airk-1.0.0-$platform.vsix"; $expected += "airk-1.0.0-$platform.vsix-manifest.json" }
    foreach ($target in @('x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu', 'aarch64-apple-darwin', 'x86_64-apple-darwin')) { $expected += "airk-1.0.0-$target.zip"; $expected += "airk-1.0.0-$target.native-manifest.json" }
    if ($release.assets.Count -ne 19 -or (Compare-Object ($expected | Sort-Object) ($release.assets.name | Sort-Object))) { throw 'Published asset set differs' }
    foreach ($asset in $release.assets) {
        if ($asset.state -ne 'uploaded' -or $asset.size -le 0 -or $asset.digest -notmatch '^sha256:[a-f0-9]{64}$') { throw 'Published asset is incomplete' }
    }
    $downloadNames = @($expected | Where-Object { $_ -match '\.(zip|vsix)$' })
    for ($poll = 0; $poll -lt 20; $poll++) {
        $page = Invoke-WebRequest -Uri 'https://airk.ioka.io/' -TimeoutSec 30
        $missing = @($downloadNames | Where-Object { -not $page.Content.Contains("https://github.com/iokaio/airk/releases/download/$tag/$_") })
        if ($page.StatusCode -eq 200 -and $missing.Count -eq 0 -and $page.Content.Contains('id="stable-title"')) {
            $page.Content | Set-Content -Encoding utf8 (Join-Path $evidence 'live-downloads.html')
            Save-DownloadsState 'published_and_site_verified'
            exit 0
        }
        Start-Sleep -Seconds 30
    }
    throw 'Release and deployment succeeded, but live site links have not yet been verified; do not republish'
} catch {
    $downloadsState['error'] = $_.Exception.Message
    Save-DownloadsState 'stopped_for_inspection'
    throw
}
