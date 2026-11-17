# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$query = @{
    filters = @(@{ criteria = @(@{ filterType = 7; value = 'iokaio.airk' }); pageNumber = 1; pageSize = 10 })
    assetTypes = @()
    flags = 147
} | ConvertTo-Json -Depth 8
$catalog = Invoke-RestMethod -Method Post -Uri 'https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery' -ContentType 'application/json' -Headers @{ Accept = 'application/json;api-version=7.2-preview.1' } -Body $query -TimeoutSec 60
$extension = @($catalog.results.extensions | Where-Object { $_.publisher.publisherName -eq 'iokaio' -and $_.extensionName -eq 'airk' })
if ($extension.Count -ne 1) { throw 'Expected exactly one AIRK Marketplace entry.' }
$versions = @($extension[0].versions | Where-Object version -eq '1.1.0')
$expected = @('darwin-arm64', 'darwin-x64', 'linux-x64', 'win32-x64')
if ($versions.Count -ne 4 -or (Compare-Object $expected @($versions.targetPlatform | Sort-Object))) { throw 'Marketplace does not yet list all four 1.1.0 platforms.' }
foreach ($version in $versions) {
    if (@($version.properties | Where-Object { $_.key -eq 'Microsoft.VisualStudio.Code.PreRelease' -and $_.value -eq 'true' }).Count) { throw 'Unexpected preview version.' }
}
$extension[0] | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'marketplace-verified.json') -Encoding utf8
$versions | Select-Object version, targetPlatform, lastUpdated | Format-Table -AutoSize
Write-Output 'Verified stable Marketplace 1.1.0 for all four platforms.'
